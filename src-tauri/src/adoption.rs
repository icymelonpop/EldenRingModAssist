use crate::{depot, modpack};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path};
use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Plan {
    files: Vec<Changed>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Changed {
    relative: String,
    size: u64,
    sha256: String,
    loose: bool,
}

impl Plan {
    pub(crate) fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
    fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for item in &self.files {
            let unsafe_name = item.relative.split('/').any(|part| {
                part.is_empty()
                    || part.ends_with(['.', ' '])
                    || part.contains([':', '\\', '\0'])
                    || !matches!(
                        Path::new(part).components().next(),
                        Some(Component::Normal(_))
                    )
            });
            if unsafe_name
                || item.sha256.len() != 64
                || !item.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !seen.insert(item.relative.to_lowercase())
            {
                return Err("기존 설치를 보관한 파일 목록이 손상되었습니다.".into());
            }
            if item.loose && !modpack::is_loose_asset(Path::new(&item.relative)) {
                return Err("보관 목록의 모드 자산 경로를 확인하세요.".into());
            }
        }
        if self.files.is_empty() {
            return Err("보관할 모드 파일이 없습니다.".into());
        }
        Ok(())
    }
}

fn fingerprint(path: &Path) -> Result<String> {
    if !modpack::metadata(path)?.is_file() {
        return Err(format!("일반 파일이 필요합니다. {}", path.display()));
    }
    let mut hash = Sha256::new();
    io::copy(&mut File::open(path).map_err(|e| e.to_string())?, &mut hash)
        .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn plan(game: &Path) -> Result<Plan> {
    let report = depot::audit_install(game)?;
    if report.stale {
        return Err("Steam 설치 정보가 변경되었습니다. 다시 검사하세요.".into());
    }
    let mut wanted: HashSet<String> = report.changed.into_iter().chain(report.added).collect();
    if wanted.is_empty() {
        return Err(if report.missing.is_empty() {
            "현재 설치가 Steam 원본과 일치합니다. 원본 등록을 선택하세요."
        } else {
            "보관할 모드 파일은 없고 공식 파일이 누락되었습니다. Steam 파일 검사 후 원본을 등록하세요."
        }.into());
    }
    let mut files = Vec::new();
    for (rel, metadata) in modpack::walk(game)? {
        let name = rel
            .to_str()
            .ok_or("경로를 읽을 수 없습니다.")?
            .replace('\\', "/");
        if wanted.remove(&name.to_lowercase()) {
            files.push(Changed {
                relative: name,
                loose: modpack::is_loose_asset(&rel),
                size: metadata.len(),
                sha256: fingerprint(&game.join(&rel))?,
            });
        }
    }
    if !wanted.is_empty() {
        return Err("보관 중 설치 파일이 바뀌었습니다. 다시 시도하세요.".into());
    }
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    let result = Plan { files };
    result.validate()?;
    Ok(result)
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            modpack::metadata(path)?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{} · {e}", path.display())),
    }
}

// Check each directory before walking through it. Only the destination side may
// create missing components, always below the two roots from the switch journal.
fn parents(root: &Path, rel: &Path, create: bool) -> Result<()> {
    let mut current = root.to_owned();
    if !modpack::metadata(root)?.is_dir() {
        return Err("폴더 경로를 확인하세요.".into());
    }
    for component in rel.parent().unwrap_or(Path::new("")).components() {
        let Component::Normal(name) = component else {
            return Err("상대 경로가 잘못되었습니다.".into());
        };
        current.push(name);
        if !exists(&current)? {
            if !create {
                return Ok(());
            }
            fs::create_dir(&current).map_err(|e| format!("{} · {e}", current.display()))?;
        }
        if !modpack::metadata(&current)?.is_dir() {
            return Err(format!("폴더가 필요합니다. {}", current.display()));
        }
    }
    Ok(())
}

fn move_new(source: &Path, target: &Path) -> Result<()> {
    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(Some(0)).collect()
    }
    let src = wide(source.as_os_str());
    let dst = wide(target.as_os_str());
    for attempt in 0..=8 {
        // Same-volume rename, with no flag permitting replacement of a new destination.
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 0) } != 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if attempt == 8 || !matches!(e.raw_os_error(), Some(5 | 32 | 33)) {
            return Err(format!("{} → {} · {e}", source.display(), target.display()));
        }
        std::thread::sleep(std::time::Duration::from_millis(30 * (attempt + 1)));
    }
    unreachable!()
}

fn transfer(game: &Path, root: &Path, plan: &Plan, undo: bool, execute: bool) -> Result<()> {
    plan.validate()?;
    for item in &plan.files {
        let original = Path::new(&item.relative);
        let archive = Path::new(if item.loose { "mod" } else { "preserved" }).join(original);
        let ((src_root, src_rel), (dst_root, dst_rel)) = if undo {
            ((root, archive.as_path()), (game, original))
        } else {
            ((game, original), (root, archive.as_path()))
        };
        parents(src_root, src_rel, false)?;
        parents(dst_root, dst_rel, execute)?;
        let src = src_root.join(src_rel);
        let dst = dst_root.join(dst_rel);
        let (current, pending) = match (exists(&src)?, exists(&dst)?) {
            (true,false) => (&src,true),
            (false,true) => (&dst,false),
            _ => return Err(format!("{} · 파일 위치가 바뀌었거나 복구된 파일이 이미 있습니다. 덮어쓰지 않고 중단했습니다.",item.relative)),
        };
        if modpack::metadata(current)?.len() != item.size || fingerprint(current)? != item.sha256 {
            return Err(format!(
                "{} 내용이 보관 계획과 다릅니다. 현재 파일을 확인하세요.",
                item.relative
            ));
        }
        if execute && pending {
            move_new(&src, &dst)?;
        }
    }
    Ok(())
}

const RECORD: &str = "adoption.json";

fn read_plan(root: &Path) -> Result<Plan> {
    let path = root.join(RECORD);
    if !modpack::metadata(&path)?.is_file() {
        return Err("보관 정보를 읽을 수 없습니다.".into());
    }
    let plan: Plan = serde_json::from_reader(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    plan.validate()?;
    Ok(plan)
}

pub(crate) fn capture(game: &Path, root: &Path, plan: &Plan) -> Result<()> {
    plan.validate()?;
    if !exists(root)? {
        fs::create_dir(root).map_err(|e| e.to_string())?;
    }
    let record = root.join(RECORD);
    if exists(&record)? {
        if read_plan(root)? != *plan {
            return Err("다른 보관 작업의 폴더가 이미 있습니다.".into());
        }
    } else {
        if fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
        {
            return Err("사용 중인 폴더에는 보관할 수 없습니다.".into());
        }
        let mut tmp =
            tempfile::NamedTempFile::new_in(root.parent().ok_or("상위 경로가 없습니다.")?)
                .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut tmp, plan).map_err(|e| e.to_string())?;
        tmp.flush()
            .and_then(|_| tmp.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        tmp.persist_noclobber(record).map_err(|e| e.to_string())?;
    }
    modpack::write_profile(root)?;
    transfer(game, root, plan, false, false)?;
    transfer(game, root, plan, false, true)
}

pub(crate) fn check_undo(game: &Path, root: &Path) -> Result<()> {
    transfer(game, root, &read_plan(root)?, true, false)
}

/// A failed preflight may be abandoned only while the originals are still at
/// home and the journal has no moved payload. Never remove the candidate folder.
pub(crate) fn unmoved(game: &Path, root: &Path, plan: &Plan) -> Result<bool> {
    plan.validate()?;
    let has_root = exists(root)?;
    if has_root {
        if !exists(&root.join(RECORD))? {
            if fs::read_dir(root)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Ok(false);
            }
        } else if read_plan(root)? != *plan {
            return Ok(false);
        }
    }
    for item in &plan.files {
        let rel = Path::new(&item.relative);
        parents(game, rel, false)?;
        if !exists(&game.join(rel))? {
            return Ok(false);
        }
        if has_root {
            let stored = Path::new(if item.loose { "mod" } else { "preserved" }).join(rel);
            parents(root, &stored, false)?;
            if exists(&root.join(stored))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(crate) fn undo(game: &Path, root: &Path) -> Result<()> {
    let plan = read_plan(root)?;
    transfer(game, root, &plan, true, false)?;
    transfer(game, root, &plan, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adoption_moves_only_recorded_changes_and_undo_never_overwrites_steam_repair() {
        let work = tempfile::tempdir().unwrap();
        let game = work.path().join("Game");
        let profile = work.path().join("profile-1");
        fs::create_dir(&game).unwrap();
        fs::write(game.join("Data0.bdt"), vec![1; 1024 * 1024]).unwrap();
        fs::write(game.join("regulation.bin"), b"mod").unwrap();
        fs::write(game.join("extra.log"), b"archive").unwrap();
        let plan = Plan {
            files: ["regulation.bin", "extra.log"]
                .iter()
                .map(|n| Changed {
                    relative: n.to_string(),
                    loose: *n == "regulation.bin",
                    size: fs::metadata(game.join(n)).unwrap().len(),
                    sha256: fingerprint(&game.join(n)).unwrap(),
                })
                .collect(),
        };
        capture(&game, &profile, &plan).unwrap();
        capture(&game, &profile, &plan).unwrap();
        assert_eq!(plan.bytes(), 10);
        assert!(game.join("Data0.bdt").exists() && !profile.join("mod/Data0.bdt").exists());
        assert_eq!(
            fs::read(profile.join("preserved/extra.log")).unwrap(),
            b"archive"
        );
        fs::write(game.join("regulation.bin"), b"original").unwrap();
        assert!(check_undo(&game, &profile).is_err());
        assert!(!game.join("extra.log").exists());
        fs::remove_file(game.join("regulation.bin")).unwrap();
        undo(&game, &profile).unwrap();
        undo(&game, &profile).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"mod");
    }
}
