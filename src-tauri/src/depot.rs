//! Read the plaintext manifests Steam already cached for the installed depots.
//! ContentManifestPayload/Metadata use the field numbers from content_manifest.proto.
use prost::Message;
use std::collections::BTreeMap;
use std::path::Path;

use crate::platform;
use crate::{modpack, progress, switcher::Report};
use sha1::{Digest, Sha1};
use std::io::Read;
use std::os::windows::fs::MetadataExt;

#[derive(Debug, PartialEq, Eq)]
pub struct OfficialFile {
    pub size: u64,
    pub sha1: String,
}

#[derive(Message)]
struct Payload {
    #[prost(message, repeated, tag = "1")]
    files: Vec<Mapping>,
}
#[derive(Message)]
struct Mapping {
    #[prost(string, required, tag = "1")]
    name: String,
    #[prost(uint64, tag = "2")]
    size: u64,
    #[prost(uint32, tag = "3")]
    flags: u32,
    #[prost(bytes = "vec", tag = "5")]
    file_hash: Vec<u8>,
    #[prost(string, tag = "7")]
    link_target: String,
}
#[derive(Message)]
struct Metadata {
    #[prost(uint32, tag = "1")]
    depot: u32,
    #[prost(uint64, tag = "2")]
    manifest: u64,
    #[prost(bool, tag = "4")]
    encrypted_names: bool,
    #[prost(uint32, optional, tag = "9")]
    clear_crc: Option<u32>,
}

// A normal game session can generate Crashpad bookkeeping and diagnostic dumps.
// Recognize their exact shape, so a DLL placed in the same directory is still reported.
pub fn is_crash_record(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/").to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "crashdumps/metadata" | "crashdumps/settings.dat"
    ) {
        return true;
    }
    let parts: Vec<_> = normalized.split('/').collect();
    if parts.len() != 3
        || parts[0] != "crashdumps"
        || !matches!(parts[1], "reports" | "pending" | "completed")
    {
        return false;
    }
    let Some(id) = parts[2].strip_suffix(".dmp") else {
        return false;
    };
    id.len() == 36
        && id.bytes().enumerate().all(|(index, b)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn section<'a>(input: &mut &'a [u8], magic: u32) -> Result<&'a [u8], String> {
    let (header, tail) = input
        .split_at_checked(8)
        .ok_or("Steam 파일 목록이 잘렸습니다.")?;
    if header[..4] != magic.to_le_bytes() {
        return Err("Steam 파일 목록 형식을 확인할 수 없습니다.".into());
    }
    let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    let (body, rest) = tail
        .split_at_checked(length)
        .ok_or("Steam 파일 목록 길이가 잘못되었습니다.")?;
    *input = rest;
    Ok(body)
}

fn read_manifest(data: &[u8], depot: &str, manifest: &str) -> Result<Vec<Mapping>, String> {
    let mut rest = data;
    let payload = section(&mut rest, 0x71f617d0)?;
    let metadata = section(&mut rest, 0x1f4812be)?;
    let _signature = section(&mut rest, 0x1b81b817)?;
    if rest != 0x32c415abu32.to_le_bytes() {
        return Err("Steam 파일 목록 끝부분을 확인할 수 없습니다.".into());
    }
    let info = Metadata::decode(metadata).map_err(|e| e.to_string())?;
    if info.depot.to_string() != depot
        || info.manifest.to_string() != manifest
        || info.encrypted_names
    {
        return Err("현재 설치 버전의 읽을 수 있는 Steam 파일 목록이 필요합니다.".into());
    }
    let mut crc = crc32fast::Hasher::new();
    crc.update(&(payload.len() as u32).to_le_bytes());
    crc.update(payload);
    if info.clear_crc != Some(crc.finalize()) {
        return Err("Steam 파일 목록의 체크섬이 일치하지 않습니다.".into());
    }
    Ok(Payload::decode(payload).map_err(|e| e.to_string())?.files)
}

pub fn expected_files(game: &Path) -> Result<BTreeMap<String, OfficialFile>, String> {
    let depots = platform::installed_depots(game)?;
    let mut caches = vec![platform::steam_root()?.join("depotcache")];
    if let Some(steamapps) = game.ancestors().nth(3) {
        caches.push(steamapps.join("depotcache"));
        if let Some(library) = steamapps.parent() {
            caches.push(library.join("depotcache"));
        }
    }
    let mut files = BTreeMap::new();
    for (depot, manifest) in depots {
        if depot.parse::<u32>().is_err() || manifest.parse::<u64>().is_err() {
            return Err("Steam 설치 목록의 식별자가 잘못되었습니다.".into());
        }
        let name = format!("{depot}_{manifest}.manifest");
        let path = caches.iter().map(|d| d.join(&name)).find(|p| p.is_file())
            .ok_or_else(|| format!("Steam의 {name} 파일이 없습니다. Steam에서 설치 파일 무결성을 확인한 후 다시 시도하세요."))?;
        if path.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
            return Err("Steam 파일 목록의 크기를 확인하세요.".into());
        }
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let mappings = read_manifest(&data, &depot, &manifest)
            .map_err(|e| format!("{name} · {e} Steam에서 설치 파일을 확인하세요."))?;
        for mapping in mappings {
            let normalized = mapping.name.replace('\\', "/").to_lowercase();
            let Some(relative) = normalized.strip_prefix("game/") else {
                continue;
            };
            if mapping.flags & 64 != 0 {
                continue;
            } // Depot directory record.
            if relative
                .split('/')
                .any(|p| matches!(p, "" | "." | "..") || p.contains(':'))
                || !mapping.link_target.is_empty()
                || mapping.file_hash.len() != 20
            {
                return Err(format!(
                    "지원하지 않는 Steam 파일 항목입니다. {}",
                    mapping.name
                ));
            }
            let entry = OfficialFile {
                size: mapping.size,
                sha1: mapping
                    .file_hash
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            };
            if files.get(relative).is_some_and(|present| present != &entry) {
                return Err(format!(
                    "Steam 목록 사이에 서로 다른 항목이 있습니다. {relative}"
                ));
            }
            files.insert(relative.to_owned(), entry);
        }
    }
    if [
        "eldenring.exe",
        "start_protected_game.exe",
        "regulation.bin",
    ]
    .iter()
    .any(|p| !files.contains_key(*p))
        || !files.keys().any(|p| p.starts_with("easyanticheat/"))
    {
        return Err("Steam 목록에서 엘든 링과 보호 실행 파일을 확인할 수 없습니다.".into());
    }
    Ok(files)
}

/// Compare an existing install before any profile or baseline has been created.
pub fn audit_install(game: &Path) -> Result<Report, String> {
    let build = platform::steam_build(game)?;
    let depots = platform::installed_depots(game)?;
    let mut expected = expected_files(game)?;
    let files = modpack::walk(game)?;
    let key = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    let total = files
        .iter()
        .filter(|(p, info)| expected.get(&key(p)).is_some_and(|v| v.size == info.len()))
        .map(|(_, info)| info.len())
        .sum();
    let inventory = |entries: &[(std::path::PathBuf, std::fs::Metadata)]| -> BTreeMap<_, _> {
        entries
            .iter()
            .map(|(path, meta)| {
                (
                    path.clone(),
                    (meta.len(), meta.last_write_time(), meta.creation_time()),
                )
            })
            .collect()
    };
    let before_inventory = inventory(&files);
    progress::start(total);
    let mut checked = 0;
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut buffer = vec![0; 2 * 1024 * 1024];
    for (path, before) in files {
        let name = key(&path);
        let Some(official) = expected.remove(&name) else {
            if !is_crash_record(&name) {
                added.push(name);
            }
            continue;
        };
        checked += 1;
        if before.len() != official.size {
            changed.push(name);
            continue;
        }
        let full = game.join(path);
        let mut input =
            std::fs::File::open(&full).map_err(|e| format!("{} · {e}", full.display()))?;
        let mut sha1 = Sha1::new();
        loop {
            let n = input
                .read(&mut buffer)
                .map_err(|e| format!("{} · {e}", full.display()))?;
            if n == 0 {
                break;
            }
            sha1.update(&buffer[..n]);
            progress::advance(n as u64);
        }
        if format!("{:x}", sha1.finalize()) != official.sha1
            || modpack::metadata(&full)?.last_write_time() != before.last_write_time()
        {
            changed.push(name);
        }
    }
    let after_build = platform::steam_build(game)?;
    let stale = build != after_build || depots != platform::installed_depots(game)?;
    if inventory(&modpack::walk(game)?) != before_inventory {
        return Err(
            "검사 도중 게임 폴더가 변경되었습니다. 다운로드나 파일 복사를 마친 후 다시 검사하세요."
                .into(),
        );
    }
    added.sort();
    changed.sort();
    Ok(Report {
        matches: !stale && added.is_empty() && changed.is_empty() && expected.is_empty(),
        checked,
        added,
        changed,
        missing: expected.into_keys().collect(),
        stale,
        baseline_build: Some(build),
        steam_build: Some(after_build),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_files_are_recognized_narrowly() {
        assert!(is_crash_record(
            r"CrashDumps\REPORTS\3025052d-f5f6-4c47-b08e-918a4448a0cc.dmp"
        ));
        assert!(!is_crash_record("crashdumps/reports/plugin.dll"));
        assert!(!is_crash_record("other/settings.dat"));
    }

    #[test]
    fn incomplete_depot_never_yields_a_partial_baseline() {
        assert!(read_manifest(&[0; 12], "1", "1").is_err());
    }

    #[test]
    #[ignore = "read-only check against this machine's installed Steam game"]
    fn live_steam_inventory_and_audit() {
        let game = platform::detect_game()
            .unwrap()
            .expect("Steam installation");
        let report = audit_install(&game).unwrap();
        println!(
            "Steam build {:?}, checked {}, extra {}, changed {}, missing {}, matches {}",
            report.steam_build,
            report.checked,
            report.added.len(),
            report.changed.len(),
            report.missing.len(),
            report.matches
        );
        assert!(report.checked > 0 && !report.stale);
    }
}
