use super::R;
use crate::depot::OfficialFile;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

pub(super) fn io(path: &Path, e: std::io::Error) -> String {
    format!("{} — {}", path.display(), e)
}

pub(super) fn meta(path: &Path) -> R<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                m.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = m.file_type().is_symlink();
            if linked || !(m.is_file() || m.is_dir()) {
                Err(format!(
                    "연결 경로 또는 특수 파일을 처리할 수 없습니다. {}",
                    path.display()
                ))
            } else {
                Ok(Some(m))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(path, e)),
    }
}

pub(super) fn directory(path: &Path) -> R<bool> {
    match meta(path)? {
        None => Ok(false),
        Some(m) if m.is_dir() => Ok(true),
        _ => Err(format!(
            "폴더가 필요한 위치에 파일이 있습니다. {}",
            path.display()
        )),
    }
}

pub(super) fn check_game(path: &Path, vanilla: bool) -> R<()> {
    if !directory(path)? || !meta(&path.join("eldenring.exe"))?.is_some_and(|m| m.is_file()) {
        return Err(format!(
            "eldenring.exe가 있는 게임 폴더가 필요합니다. {}",
            path.display()
        ));
    }
    if vanilla {
        if !meta(&path.join("start_protected_game.exe"))?.is_some_and(|m| m.is_file())
            || !directory(&path.join("EasyAntiCheat"))?
        {
            return Err(
                "공식 실행기 또는 EasyAntiCheat 폴더가 없습니다. Steam에서 게임 파일을 복구하세요."
                    .into(),
            );
        }
        for item in [
            "dinput8.dll",
            "dxgi.dll",
            "mods",
            "modengine2",
            "SeamlessCoop",
            "ersc_launcher.exe",
            "mod_loader_config.ini",
            "elden_mod_loader.log",
            "launchmod_eldenring.bat",
        ] {
            if meta(&path.join(item))?.is_some() {
                return Err(format!("모드 구성 요소 {item}이 남아 있습니다. 모드 파일을 따로 보관하고 깨끗한 바닐라 기준을 등록하세요."));
            }
        }
    }
    Ok(())
}

pub(super) fn move_dir(source: &Path, dest: &Path) -> R<()> {
    for attempt in 0.. {
        if !directory(source)? || meta(dest)?.is_some() {
            return Err(format!(
                "이동할 폴더 또는 목적지 상태를 확인하세요. {} → {}",
                source.display(),
                dest.display()
            ));
        }
        match fs::rename(source, dest) {
            Ok(()) => return Ok(()),
            Err(e) if transient_lock(&e, attempt) => (),
            Err(e) => return Err(io(source, e)),
        }
    }
    unreachable!()
}

fn transient_lock(e: &std::io::Error, attempt: u64) -> bool {
    if attempt >= 8 || !matches!(e.raw_os_error(), Some(5 | 32 | 33)) {
        return false;
    }
    std::thread::sleep(std::time::Duration::from_millis(25 * (attempt + 1)));
    true
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Entry {
    // Directories have no digest, including empty ones.
    digest: Option<String>,
    bytes: u64,
}
pub(super) type Tree = BTreeMap<String, Entry>;

fn walk(root: &Path, mut visit: impl FnMut(&Path, &Path, &Metadata) -> R<()>) -> R<()> {
    if !directory(root)? {
        return Err(format!("게임 폴더가 없습니다. {}", root.display()));
    }
    let mut pending = vec![PathBuf::new()];
    while let Some(rel_dir) = pending.pop() {
        let current = root.join(&rel_dir);
        let entries = fs::read_dir(&current).map_err(|e| io(&current, e))?;
        for item in entries {
            let item = item.map_err(|e| io(&current, e))?;
            let path = item.path();
            let rel = rel_dir.join(item.file_name());
            let m = meta(&path)?
                .ok_or_else(|| format!("검사 중 파일이 사라졌습니다. {}", path.display()))?;
            visit(&path, &rel, &m)?;
            if m.is_dir() {
                pending.push(rel);
            }
        }
    }
    Ok(())
}

type Inventory = BTreeMap<PathBuf, (bool, u64, Option<std::time::SystemTime>)>;

fn inventory(root: &Path) -> R<(u64, Inventory)> {
    let mut bytes = 0u64;
    let mut stamps = Inventory::new();
    walk(root, |_, rel, m| {
        if m.is_file() {
            bytes = bytes.saturating_add(m.len());
        }
        stamps.insert(rel.to_owned(), (m.is_file(), m.len(), m.modified().ok()));
        Ok(())
    })?;
    Ok((bytes, stamps))
}

fn ensure_unchanged(root: &Path, before: &Inventory) -> R<()> {
    if inventory(root)?.1 != *before {
        return Err(format!(
            "처리 중 폴더의 파일 구성이 바뀌었습니다. 다른 파일 작업이 끝난 뒤 다시 시도하세요. {}",
            root.display()
        ));
    }
    Ok(())
}

pub(super) fn scan(
    root: &Path,
    official: Option<&BTreeMap<String, OfficialFile>>,
    reference: Option<&Tree>,
) -> R<Tree> {
    let (bytes, before) = inventory(root)?;
    crate::progress::start(bytes);
    let mut tree = Tree::new();
    let mut matched = std::collections::BTreeSet::new();
    let mut buffer = vec![0u8; 2 * 1024 * 1024];
    walk(root, |path, rel, m| {
        let key = rel
            .to_str()
            .ok_or_else(|| format!("경로 이름을 읽을 수 없습니다. {}", path.display()))?
            .replace('\\', "/");
        let folded = key.to_lowercase();
        // Crashpad creates and rotates its own metadata as Elden Ring runs.
        // Its folders may appear on first crash. Every child file is still visited.
        if m.is_dir() && (folded == "crashdumps" || folded.starts_with("crashdumps/")) {
            return Ok(());
        }
        if m.is_file()
            && crate::depot::is_crash_record(&key)
            && !official.is_some_and(|files| files.contains_key(&folded))
            && !reference.is_some_and(|tree| tree.contains_key(&key))
        {
            crate::progress::advance(m.len());
            return Ok(());
        }
        let digest = if m.is_file() {
            let expected = match official {
                Some(files) => {
                    let f = files
                        .get(&folded)
                        .ok_or_else(|| format!("Steam 공식 파일 목록에 없는 파일입니다. {key}"))?;
                    if f.size != m.len() {
                        return Err(format!("Steam 원본과 파일 크기가 다릅니다. {key}"));
                    }
                    matched.insert(folded);
                    Some(f)
                }
                None => None,
            };
            let mut file = File::open(path).map_err(|e| io(path, e))?;
            let mut hasher = Sha256::new();
            let mut depot_hash = expected.map(|_| Sha1::new());
            let mut bytes = 0u64;
            loop {
                let n = file.read(&mut buffer).map_err(|e| io(path, e))?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                hasher.update(&buffer[..n]);
                if let Some(s) = &mut depot_hash {
                    s.update(&buffer[..n]);
                }
                crate::progress::advance(n as u64);
            }
            let after = file.metadata().map_err(|e| io(path, e))?;
            if bytes != m.len() || after.modified().ok() != m.modified().ok() {
                return Err(format!("검사 중 파일이 변경되었습니다. {}", path.display()));
            }
            if let (Some(entry), Some(s)) = (expected, depot_hash) {
                if !entry
                    .sha1
                    .eq_ignore_ascii_case(&format!("{:x}", s.finalize()))
                {
                    return Err(format!("Steam 공식 파일 내용과 다릅니다. {key}"));
                }
            }
            Some(format!("{:x}", hasher.finalize()))
        } else {
            None
        };
        tree.insert(
            key,
            Entry {
                digest,
                bytes: if m.is_file() { m.len() } else { 0 },
            },
        );
        Ok(())
    })?;
    if let Some(files) = official {
        if let Some(missing) = files.keys().find(|k| !matched.contains(*k)) {
            return Err(format!("Steam 공식 파일이 누락되었습니다. {missing}"));
        }
    }
    ensure_unchanged(root, &before)?;
    Ok(tree)
}

/// Compare a complete legacy install to the registered baseline. Reading game data is
/// necessary once for conversion; only files that actually differ become mod assets.
pub(super) struct Difference {
    pub(super) source: PathBuf,
    pub(super) paths: Vec<PathBuf>,
    before: Inventory,
}
impl Difference {
    pub(super) fn compare(source: &Path, baseline: &Tree) -> R<Self> {
        let before = inventory(source)?.1;
        let actual = scan(source, None, Some(baseline))?;
        let by_name = |tree: &Tree| {
            tree.iter()
                .filter(|(_, e)| e.digest.is_some())
                .map(|(name, e)| (name.to_lowercase(), (name.to_owned(), e.clone())))
                .collect::<BTreeMap<_, _>>()
        };
        let current = by_name(&actual);
        let expected = by_name(baseline);
        if let Some((_, (missing, _))) = expected
            .iter()
            .find(|(name, _)| !current.contains_key(*name))
        {
            return Err(format!("설치본에 기준 파일 {missing}이 없습니다. 삭제된 원본은 외부 모드 폴더로 표현할 수 없습니다. 원본 파일을 복구하거나 배포된 모드 폴더를 선택하세요."));
        }
        let paths = current
            .into_iter()
            .filter(|(key, (_, entry))| {
                expected
                    .get(key)
                    .is_none_or(|(_, reference)| reference != entry)
            })
            .map(|(_, (name, _))| PathBuf::from(name))
            .collect();
        Ok(Self {
            source: source.to_owned(),
            before,
            paths,
        })
    }
    pub(super) fn unchanged(&self) -> R<()> {
        ensure_unchanged(&self.source, &self.before)
    }
}

// Files that require a different loader are retained as inert data outside mod/.
pub(super) struct Archive {
    pub(super) bytes: u64,
    root: PathBuf,
    paths: Vec<PathBuf>,
}
impl Archive {
    pub(super) fn select(root: &Path, paths: Vec<PathBuf>) -> R<Option<Self>> {
        if paths.is_empty() {
            return Ok(None);
        }
        let mut bytes = 0u64;
        for rel in &paths {
            let info = Self::file(root, rel)?;
            bytes = bytes
                .checked_add(info.len())
                .ok_or("보관할 파일이 너무 큽니다.")?;
        }
        Ok(Some(Self {
            bytes,
            root: root.to_owned(),
            paths,
        }))
    }

    fn file(root: &Path, rel: &Path) -> R<Metadata> {
        let mut current = root.to_owned();
        if !directory(root)? {
            return Err("원본 폴더가 없습니다.".into());
        }
        for component in rel.components() {
            let std::path::Component::Normal(part) = component else {
                return Err("원본 파일 경로를 확인하세요.".into());
            };
            if part.to_string_lossy().contains(':') {
                return Err("원본 파일 이름을 확인하세요.".into());
            }
            current.push(part);
            meta(&current)?.ok_or("원본이 변경되었습니다.")?;
        }
        meta(&current)?
            .filter(|m| m.is_file())
            .ok_or_else(|| format!("원본 파일이 없습니다. {}", current.display()))
    }

    pub(super) fn copy_into(&self, destination: &Path) -> R<()> {
        fs::create_dir(destination).map_err(|e| io(destination, e))?;
        for rel in &self.paths {
            let src = self.root.join(rel);
            let dest = destination.join(rel);
            let before = Self::file(&self.root, rel)?;
            let parent = dest.parent().ok_or("목적지 폴더가 없습니다.")?;
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            let mut input = File::open(&src).map_err(|e| io(&src, e))?;
            let mut output = File::create_new(&dest).map_err(|e| io(&dest, e))?;
            let written = std::io::copy(&mut input, &mut output).map_err(|e| io(&dest, e))?;
            output.sync_all().map_err(|e| io(&dest, e))?;
            if written != before.len()
                || Self::file(&self.root, rel)?.modified().ok() != before.modified().ok()
            {
                return Err(format!("복사 중 원본이 변경되었습니다. {}", src.display()));
            }
            crate::progress::advance(written);
        }
        Ok(())
    }
}
pub(super) fn payload_stats(root: &Path) -> R<(u64, usize)> {
    let mut total = 0u64;
    let mut count = 0;
    walk(root, |_, _, m| {
        if m.is_file() {
            total = total.saturating_add(m.len());
            count += 1;
        }
        Ok(())
    })?;
    Ok((total, count))
}

pub(super) fn payload_bytes(root: &Path) -> R<u64> {
    payload_stats(root).map(|(bytes, _)| bytes)
}

pub(super) fn check_space(dest: &Path, total: u64) -> R<()> {
    crate::progress::start(total);
    let free = fs2::available_space(dest).map_err(|e| io(dest, e))?;
    if free < total {
        return Err(format!(
            "모드 파일 복사에 {}바이트가 필요합니다. 저장 공간은 {}바이트 남았습니다.",
            total, free
        ));
    }
    Ok(())
}

pub(super) fn atomic_json(path: &Path, value: &impl Serialize) -> R<()> {
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(|e| io(path, e))?;
    {
        let mut writer = BufWriter::new(tmp.as_file_mut());
        serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| io(path, e))?;
    }
    tmp.as_file().sync_all().map_err(|e| io(path, e))?;
    // Windows can transiently deny replacement while a scanner still has
    // the last revision open. Keep the durable new file and retry the rename.
    let mut staged = tmp.into_temp_path();
    for attempt in 0.. {
        match staged.persist(path) {
            Ok(()) => return Ok(()),
            Err(e) if transient_lock(&e.error, attempt) => {
                staged = e.path;
            }
            Err(e) => return Err(io(path, e.error)),
        }
    }
    unreachable!()
}

pub(super) const PARTIAL: &str = ".eldenring-mod-assist.partial";

pub(super) fn discard_partial(dir: &Path, staging: &str) -> R<()> {
    if !directory(dir)? {
        return Ok(());
    }
    let marker = dir.join(PARTIAL);
    if meta(&marker)?.is_none() {
        // An interruption can occur between removing the marker and its empty folder.
        return fs::remove_dir(dir).map_err(|e| io(dir, e));
    }
    if fs::read(&marker).map_err(|e| io(&marker, e))? != staging.as_bytes() {
        return Err(format!(
            "부분 복사 폴더의 소유 표식을 확인하세요. {}",
            dir.display()
        ));
    }
    // Only app-created, recorded staging folders are eligible. Refuse links even
    // inside the staging tree and leave the marker until all children are gone.
    walk(dir, |_, _, _| Ok(()))?;
    for entry in fs::read_dir(dir).map_err(|e| io(dir, e))? {
        let path = entry.map_err(|e| io(dir, e))?.path();
        if path == marker {
            continue;
        }
        if entry_is_directory(&path)? {
            fs::remove_dir_all(&path).map_err(|e| io(&path, e))?;
        } else {
            fs::remove_file(&path).map_err(|e| io(&path, e))?;
        }
    }
    fs::remove_file(&marker).map_err(|e| io(&marker, e))?;
    fs::remove_dir(dir).map_err(|e| io(dir, e))
}

fn entry_is_directory(path: &Path) -> R<bool> {
    meta(path)?
        .map(|m| m.is_dir())
        .ok_or_else(|| format!("파일이 사라졌습니다. {}", path.display()))
}
