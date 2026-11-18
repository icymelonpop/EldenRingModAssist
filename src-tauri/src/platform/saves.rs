use super::{ensure_game_closed, error_at, metadata, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING};

pub mod periodic;

#[derive(Clone, Serialize)]
pub struct SaveBackup {
    pub id: String,
    pub created: u64,
    pub files: usize,
    pub bytes: u64,
    pub label: String,
    pub reason: BackupReason,
    pub pinned: bool,
    pub accounts: Vec<String>,
    pub formats: Vec<String>,
}

#[derive(Serialize)]
pub struct SaveFile {
    pub account: String,
    pub name: String,
    pub bytes: u64,
    pub modified: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupReason {
    #[default]
    Manual,
    BeforeRestore,
    BeforeSwitch,
    Periodic,
}

#[derive(Serialize, Deserialize)]
struct Item {
    account: String,
    name: String,
    bytes: u64,
    sha256: String,
}

impl Item {
    fn path(&self, root: &Path) -> PathBuf {
        root.join(&self.account).join(&self.name)
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    created: u64,
    #[serde(default)]
    label: String,
    #[serde(default)]
    reason: BackupReason,
    #[serde(default)]
    pinned: bool,
    files: Vec<Item>,
}

impl Manifest {
    fn summary(&self, id: &str) -> SaveBackup {
        SaveBackup {
            id: id.to_owned(),
            created: self.created,
            files: self.files.len(),
            label: self.label.clone(),
            reason: self.reason,
            pinned: self.pinned,
            accounts: self
                .files
                .iter()
                .map(|f| f.account.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            formats: self
                .files
                .iter()
                .filter_map(|f| save_format(&f.name))
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            bytes: self
                .files
                .iter()
                .fold(0u64, |sum, item| sum.saturating_add(item.bytes)),
        }
    }
}

#[derive(PartialEq, Serialize, Deserialize)]
struct Restore {
    target: String,
    before: Option<String>,
}

// Antivirus and indexers can briefly keep a newly closed file/directory open
// without delete sharing. Validate again on each attempt, including after a wait.
fn retry_locked(
    path: &Path,
    mut validate: impl FnMut() -> Result<()>,
    mut action: impl FnMut() -> io::Result<()>,
) -> Result<()> {
    for attempt in 0..=8 {
        validate()?;
        match action() {
            Ok(()) => return Ok(()),
            Err(e) if attempt < 8 && matches!(e.raw_os_error(), Some(5 | 32 | 33)) => {
                std::thread::sleep(std::time::Duration::from_millis(40 * (attempt + 1)));
            }
            Err(e) => return Err(error_at(path, e)),
        }
    }
    unreachable!()
}

fn rename_with_retry(
    source: &Path,
    target: &Path,
    replace: bool,
    validate: impl FnMut() -> Result<()>,
) -> Result<()> {
    let wide = |path: &Path| -> Result<Vec<u16>> {
        let parent = path.parent().ok_or("백업 경로의 상위 폴더가 없습니다.")?;
        let name = path.file_name().ok_or("백업 파일명이 없습니다.")?;
        // canonicalize the existing parent, so a still-missing destination also
        // uses Windows' extended-length path syntax.
        let resolved = parent
            .canonicalize()
            .map_err(|e| error_at(parent, e))?
            .join(name);
        Ok(resolved.as_os_str().encode_wide().chain(Some(0)).collect())
    };
    let from = wide(source)?;
    let to = wide(target)?;
    let flags = if replace {
        MOVEFILE_REPLACE_EXISTING
    } else {
        0
    };
    retry_locked(target, validate, || {
        // New backup directories and journal files must never replace an
        // existing object, even if it appeared after our inspection.
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
}

fn store_path(app_data: &Path) -> PathBuf {
    app_data.join("save-backups")
}

fn save_path() -> Result<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join("EldenRing"))
        .ok_or_else(|| "Windows 사용자 세이브 경로를 찾을 수 없습니다.".into())
}

fn dir_exists(path: &Path) -> Result<bool> {
    match metadata(path)? {
        None => Ok(false),
        Some(info) if info.is_dir() => Ok(true),
        _ => Err(format!("폴더가 필요한 위치입니다. {}", path.display())),
    }
}

fn ensure_dir(path: &Path) -> Result<()> {
    if !dir_exists(path)? {
        fs::create_dir(path).map_err(|e| error_at(path, e))?;
    }
    Ok(())
}

fn lock_store(app_data: &Path) -> Result<(PathBuf, File)> {
    if !dir_exists(app_data)? {
        fs::create_dir_all(app_data).map_err(|e| error_at(app_data, e))?;
    }
    let store = store_path(app_data);
    ensure_dir(&store)?;
    let path = store.join("lock");
    metadata(&path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| error_at(&path, e))?;
    lock.try_lock()
        .map_err(|_| "다른 세이브 작업이 진행 중입니다. 잠시 후 다시 시도하세요.".to_owned())?;
    Ok((store, lock))
}

fn unique_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}-{}-{}",
        now.as_nanos(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn valid_id(id: &str) -> bool {
    id.len() <= 80
        && id.split('-').count() == 3
        && id
            .split('-')
            .all(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
}

fn valid_account(name: &str) -> bool {
    !name.is_empty() && name.len() <= 20 && name.bytes().all(|c| c.is_ascii_digit())
}

const FORMATS: [&str; 5] = ["sl2", "co2", "cnv", "cnvco2", "rd2"];

fn save_format(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    let base = lower.strip_suffix(".bak").unwrap_or(&lower);
    let (_, ext) = base.rsplit_once('.')?;
    FORMATS.into_iter().find(|f| *f == ext)
}

fn save_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if save_format(name).is_none() {
        return false;
    }
    let stem = lower.split('.').next().unwrap_or("").trim_end();
    !stem.is_empty()
        && !matches!(stem, "con" | "nul" | "aux" | "prn" | "conin$" | "conout$")
        && !["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|s| {
                matches!(
                    s,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
        && !name
            .chars()
            .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
}

#[derive(Default, Clone, Copy)]
struct Scope<'a> {
    account: Option<&'a str>,
    format: Option<&'a str>,
}

fn paths(root: &Path, scope: Scope) -> Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    if !dir_exists(root)? {
        return Ok(found);
    }
    for account in fs::read_dir(root).map_err(|e| error_at(root, e))? {
        let entry = account.map_err(|e| error_at(root, e))?;
        let account_id = entry.file_name().to_string_lossy().into_owned();
        if !valid_account(&account_id) || scope.account.is_some_and(|f| f != account_id) {
            continue;
        }
        let dir = entry.path();
        if !dir_exists(&dir)? {
            continue;
        }
        for save in fs::read_dir(&dir).map_err(|e| error_at(&dir, e))? {
            let save = save.map_err(|e| error_at(&dir, e))?;
            let filename = save.file_name().to_string_lossy().into_owned();
            if save_name(&filename)
                && scope
                    .format
                    .is_none_or(|f| save_format(&filename) == Some(f))
                && metadata(&save.path())?.is_some_and(|m| m.is_file())
            {
                found.push((account_id.clone(), filename));
            }
        }
    }
    found.sort();
    Ok(found)
}

pub fn list_save_files() -> Result<Vec<SaveFile>> {
    let root = save_path()?;
    paths(&root, Scope::default())?
        .into_iter()
        .map(|(account, name)| {
            let path = root.join(&account).join(&name);
            let info = metadata(&path)?
                .ok_or_else(|| error_at(&path, "목록을 읽는 동안 파일이 이동되었습니다."))?;
            let modified = info
                .modified()
                .map_err(|e| error_at(&path, e))?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            Ok(SaveFile {
                account,
                name,
                bytes: info.len(),
                modified,
            })
        })
        .collect()
}

pub fn live_save_path(account: &str, name: &str) -> Result<PathBuf> {
    if !valid_account(account) || !save_name(name) {
        return Err("목록에서 세이브 파일을 선택하세요.".into());
    }
    let root = save_path()?;
    let found = paths(
        &root,
        Scope {
            account: Some(account),
            format: None,
        },
    )?
    .into_iter()
    .any(|(_, file)| file == name);
    if !found {
        return Err("선택한 세이브가 목록에서 사라졌습니다. 목록을 갱신하세요.".into());
    }
    Ok(root.join(account).join(name))
}

fn file_hash(path: &Path) -> Result<String> {
    if !metadata(path)?.is_some_and(|m| m.is_file()) {
        return Err(error_at(path, "파일을 찾을 수 없습니다."));
    }
    let mut input = File::open(path).map_err(|e| error_at(path, e))?;
    let mut sha = Sha256::new();
    io::copy(&mut input, &mut sha).map_err(|e| error_at(path, e))?;
    Ok(format!("{:x}", sha.finalize()))
}

fn copy_new(source: &Path, target: &Path) -> Result<u64> {
    let mut input = File::open(source).map_err(|e| error_at(source, e))?;
    let mut output = File::create_new(target).map_err(|e| error_at(target, e))?;
    let bytes = io::copy(&mut input, &mut output).map_err(|e| error_at(target, e))?;
    output.sync_all().map_err(|e| error_at(target, e))?;
    Ok(bytes)
}

fn write_json(path: &Path, data: &impl Serialize) -> Result<()> {
    let mut file = File::create_new(path).map_err(|e| error_at(path, e))?;
    let bytes = serde_json::to_vec(data).map_err(|e| error_at(path, e))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| error_at(path, e))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    if !metadata(path)?.is_some_and(|m| m.is_file() && m.len() <= 16 * 1024 * 1024) {
        return Err(error_at(path, "백업 정보를 읽을 수 없습니다."));
    }
    let file = File::open(path).map_err(|e| error_at(path, e))?;
    serde_json::from_reader(file).map_err(|e| error_at(path, e))
}

fn manifest(store: &Path, id: &str) -> Result<Manifest> {
    if !valid_id(id) {
        return Err("백업 ID가 올바르지 않습니다.".into());
    }
    manifest_dir(&store.join(id))
}

fn manifest_dir(slot: &Path) -> Result<Manifest> {
    if !dir_exists(slot)? {
        return Err("지정된 백업을 찾을 수 없습니다.".into());
    }
    let manifest: Manifest = read_json(&slot.join("manifest.json"))?;
    if manifest.label.chars().count() > 80
        || manifest.label.chars().any(char::is_control)
        || manifest.created > 253402300799
    {
        return Err("백업 이름 또는 날짜가 올바르지 않습니다.".into());
    }
    let mut seen = HashSet::new();
    for item in &manifest.files {
        if !valid_account(&item.account)
            || !save_name(&item.name)
            || item.sha256.len() != 64
            || !item.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || !seen.insert((&item.account, item.name.to_lowercase()))
        {
            return Err("백업 파일 목록이 손상되었습니다.".into());
        }
    }
    if manifest.files.is_empty() {
        return Err("백업에 세이브가 없습니다.".into());
    }
    Ok(manifest)
}

fn snapshot(
    root: &Path,
    store: &Path,
    label: &str,
    reason: BackupReason,
    scope: Scope,
) -> Result<Option<SaveBackup>> {
    let files = paths(root, scope)?;
    if files.is_empty() {
        return Ok(None);
    }
    let stamps = if reason == BackupReason::Periodic {
        Some(periodic::stable_stamps(root, &files)?)
    } else {
        None
    };
    if matches!(reason, BackupReason::BeforeSwitch | BackupReason::Periodic)
        && already_saved(root, store, &files, reason)?
    {
        return Ok(None);
    }
    let id = unique_id();
    let stage = store.join(format!(".partial-{id}"));
    fs::create_dir(&stage).map_err(|e| error_at(&stage, e))?;
    let result = (|| {
        let mut manifest = Manifest {
            created: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            files: Vec::new(),
            label: label.trim().into(),
            reason,
            pinned: false,
        };
        for (account, name) in &files {
            ensure_dir(&stage.join(account))?;
            let source = root.join(account).join(name);
            let target = stage.join(account).join(name);
            let bytes = copy_new(&source, &target)?;
            let sha256 = file_hash(&target)?;
            manifest.files.push(Item {
                account: account.clone(),
                name: name.clone(),
                bytes,
                sha256,
            });
        }
        if paths(root, scope)? != files {
            return Err("백업 중 세이브 파일 목록이 변경되었습니다. 다시 시도하세요.".into());
        }
        for item in &manifest.files {
            if file_hash(&item.path(root))? != item.sha256 {
                return Err("백업 중 세이브 내용이 변경되었습니다. 게임과 동기화가 끝난 뒤 다시 시도하세요.".into());
            }
        }
        if let Some(original) = stamps {
            if periodic::stable_stamps(root, &files)? != original || paths(root, scope)? != files {
                return Err("세이브를 쓰는 중입니다. 다음 확인 때 백업합니다.".into());
            }
        }
        write_json(&stage.join("manifest.json"), &manifest)?;
        rename_with_retry(&stage, &store.join(&id), false, || Ok(()))?;
        Ok(Some(manifest.summary(&id)))
    })();
    if result.is_err() {
        // This is the unpublished directory created by this call under the locked store.
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

// Deduplicate only against the latest backup with the same complete set of
// source paths. A damaged backup is never treated as a reusable copy.
fn already_saved(
    root: &Path,
    store: &Path,
    files: &[(String, String)],
    reason: BackupReason,
) -> Result<bool> {
    let mut ids = Vec::new();
    for entry in fs::read_dir(store).map_err(|e| error_at(store, e))? {
        let id = entry
            .map_err(|e| error_at(store, e))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if valid_id(&id) {
            ids.push(id);
        }
    }
    ids.sort_unstable_by(|a, b| b.cmp(a));
    for id in ids {
        let Ok(previous) = manifest(store, &id) else {
            continue;
        };
        // A rotating restore point cannot satisfy a permanent pre-switch backup:
        // even a pinned periodic snapshot may be unpinned in a later session.
        if reason == BackupReason::BeforeSwitch && previous.reason == BackupReason::Periodic {
            continue;
        }
        if previous.files.len() != files.len()
            || previous
                .files
                .iter()
                .zip(files)
                .any(|(f, (account, name))| &f.account != account || &f.name != name)
        {
            continue;
        }
        for item in previous.files {
            if file_hash(&item.path(root))? != item.sha256 {
                return Ok(false);
            }
        }
        return Ok(verify_backup(store, &id).is_ok());
    }
    Ok(false)
}

fn verify_backup(store: &Path, id: &str) -> Result<Manifest> {
    let record = manifest(store, id)?;
    let slot = store.join(id);
    verify_files(&slot, &record)?;
    Ok(record)
}

fn verify_files(slot: &Path, record: &Manifest) -> Result<()> {
    for item in &record.files {
        dir_exists(&slot.join(&item.account))?;
        let path = item.path(slot);
        if !metadata(&path)?.is_some_and(|info| info.is_file() && info.len() == item.bytes)
            || file_hash(&path)? != item.sha256
        {
            return Err(error_at(&path, "백업 내용 검증에 실패했습니다."));
        }
    }
    Ok(())
}

fn absolute_dir(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || !dir_exists(path)? {
        return Err(error_at(path, "폴더의 전체 경로를 선택하세요."));
    }
    path.canonicalize().map_err(|e| error_at(path, e))
}

fn disjoint(a: &Path, b: &Path) -> Result<()> {
    let left = PathBuf::from(a.to_string_lossy().to_lowercase());
    let right = PathBuf::from(b.to_string_lossy().to_lowercase());
    if left.starts_with(&right) || right.starts_with(&left) {
        return Err("내보내거나 가져올 폴더는 세이브 원본 및 앱 보관함 밖에서 선택하세요.".into());
    }
    Ok(())
}

fn check_portable_path(path: &Path, store: &Path, live: &Path) -> Result<()> {
    disjoint(path, &absolute_dir(store)?)?;
    let real_live = if dir_exists(live)? {
        absolute_dir(live)?
    } else {
        let parent = live.parent().ok_or("Windows 세이브 경로를 확인하세요.")?;
        absolute_dir(parent)?.join(live.file_name().ok_or("세이브 폴더명이 없습니다.")?)
    };
    disjoint(path, &real_live)
}

fn copy_snapshot_dir(source: &Path, parent: &Path, id: &str, record: &Manifest) -> Result<PathBuf> {
    let target = parent.join(id);
    if metadata(&target)?.is_some() {
        return Err(error_at(&target, "같은 이름의 백업이 이미 있습니다."));
    }
    let staged = tempfile::Builder::new()
        .prefix(".partial-")
        .tempdir_in(parent)
        .map_err(|e| error_at(parent, e))?;
    for item in &record.files {
        ensure_dir(&staged.path().join(&item.account))?;
        let src = item.path(source);
        if !dir_exists(&source.join(&item.account))?
            || !metadata(&src)?.is_some_and(|m| m.is_file() && m.len() == item.bytes)
        {
            return Err(error_at(&src, "백업 파일이 변경되었습니다."));
        }
        copy_new(&src, &item.path(staged.path()))?;
    }
    verify_files(staged.path(), record)?;
    write_json(&staged.path().join("manifest.json"), record)?;
    rename_with_retry(staged.path(), &target, false, || Ok(()))?;
    Ok(target)
}

pub fn export_backup(app_data: &Path, id: &str, destination_parent: &Path) -> Result<PathBuf> {
    let (store, _lock) = lock_store(app_data)?;
    let record = manifest(&store, id)?;
    let dest = absolute_dir(destination_parent)?;
    check_portable_path(&dest.join(id), &store, &save_path()?)?;
    copy_snapshot_dir(&store.join(id), &dest, id, &record)?;
    Ok(destination_parent.join(id))
}

fn import_at(store: &Path, source: &Path, live: &Path) -> Result<SaveBackup> {
    let source = absolute_dir(source)?;
    check_portable_path(&source, store, live)?;
    let mut record = manifest_dir(&source)?;
    if record.reason == BackupReason::Periodic {
        record.pinned = true;
    }
    let id = unique_id();
    copy_snapshot_dir(&source, store, &id, &record)?;
    Ok(record.summary(&id))
}

pub fn import_backup(app_data: &Path, source: &Path) -> Result<SaveBackup> {
    let (store, _lock) = lock_store(app_data)?;
    import_at(&store, source, &save_path()?)
}

// Every write is a whole-file replacement on the destination volume. The published
// restore record allows another run to complete a set interrupted between files.
fn finish_restore(
    root: &Path,
    store: &Path,
    job: &Restore,
    check_closed: fn() -> Result<()>,
) -> Result<()> {
    let target = verify_backup(store, &job.target)?;
    let before = job
        .before
        .as_deref()
        .map(|id| manifest(store, id))
        .transpose()?;
    ensure_dir(root)?;
    let mut pending = Vec::new();
    for file in &target.files {
        let dir = root.join(&file.account);
        dir_exists(&dir)?;
        let path = file.path(root);
        let current = metadata(&path)?.map(|_| file_hash(&path)).transpose()?;
        if current.as_deref() == Some(&file.sha256) {
            continue;
        }
        let original = before
            .as_ref()
            .and_then(|b| {
                b.files.iter().find(|item| {
                    item.account == file.account && item.name.eq_ignore_ascii_case(&file.name)
                })
            })
            .map(|i| i.sha256.as_str());
        if current.as_deref() != original {
            return Err(error_at(&path, "복구 도중 외부에서 세이브가 변경되었습니다. 현재 파일을 별도로 보관하고 복구 이전 상태로 되돌린 다음 다시 시도하세요."));
        }
        pending.push((file, current));
    }
    for (file, original) in pending {
        let dir = root.join(&file.account);
        ensure_dir(&dir)?;
        let stage = dir.join(format!(".erma-{}.tmp", unique_id()));
        let result = (|| {
            copy_new(&file.path(&store.join(&job.target)), &stage)?;
            if file_hash(&stage)? != file.sha256 {
                return Err("복사한 세이브의 내용이 일치하지 않습니다.".into());
            }
            let destination = file.path(root);
            rename_with_retry(&stage, &destination, original.is_some(), || {
                let current = metadata(&destination)?
                    .map(|_| file_hash(&destination))
                    .transpose()?;
                if current != original {
                    return Err(error_at(
                        &destination,
                        "복구 중 세이브가 변경되었습니다. 게임과 동기화 상태를 확인하세요.",
                    ));
                }
                check_closed()
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(stage);
        }
        result?;
    }
    let record = store.join("restore.json");
    retry_locked(
        &record,
        || {
            if read_json::<Restore>(&record)? != *job {
                return Err(error_at(&record, "세이브 복구 기록이 변경되었습니다."));
            }
            Ok(())
        },
        || fs::remove_file(&record),
    )
}

pub fn list_backups(app_data: &Path) -> Result<Vec<SaveBackup>> {
    if !dir_exists(app_data)? {
        return Ok(Vec::new());
    }
    let store = store_path(app_data);
    if !dir_exists(&store)? {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for slot in fs::read_dir(&store).map_err(|e| error_at(&store, e))? {
        let slot = slot.map_err(|e| error_at(&store, e))?;
        let id = slot.file_name().to_string_lossy().into_owned();
        if valid_id(&id) {
            match manifest(&store, &id) {
                Ok(record) => result.push(record.summary(&id)),
                Err(_) if metadata(&slot.path())?.is_none() => continue,
                Err(error) => return Err(error),
            }
        }
    }
    result.sort_unstable_by(|a, b| b.created.cmp(&a.created).then_with(|| b.id.cmp(&a.id)));
    Ok(result)
}

pub fn backup_saves(app_data: &Path) -> Result<SaveBackup> {
    backup_selected(app_data, "", None, None)
}

pub fn ensure_saves_settled(app_data: &Path) -> Result<()> {
    if !dir_exists(app_data)? {
        return Ok(());
    }
    let store = store_path(app_data);
    if !dir_exists(&store)? {
        return Ok(());
    }
    let path = store.join("restore.json");
    if metadata(&path)?.is_some() {
        let job: Restore = read_json(&path)?;
        let backup = manifest(&store, &job.target)?;
        let label = backup.label.as_str();
        let id = job.target;
        return Err(match (backup.reason, label) {
            (BackupReason::BeforeRestore, "세이브 복구 전")
            | (BackupReason::BeforeSwitch, "프로필 전환 전")
            | (BackupReason::Periodic, "주기 백업") => format!(
                "중단된 세이브 복구가 있습니다. 세이브 백업 목록에서 복구를 다시 누르세요. {backup_kind} ({id})",
                backup_kind = label,
            ),
            _ => format!(
                "세이브 복구가 중단되었습니다. 세이브 백업 목록에서 복구를 다시 누르세요. {label} ({id})"
            ),
        });
    }
    Ok(())
}

pub fn backup_selected(
    app_data: &Path,
    label: &str,
    account: Option<&str>,
    format: Option<&str>,
) -> Result<SaveBackup> {
    if label.chars().count() > 80 || label.chars().any(char::is_control) {
        return Err("백업 이름은 한 줄에 80자까지 입력할 수 있습니다.".into());
    }
    let format = format.map(|ext| ext.strip_prefix('.').unwrap_or(ext));
    if account.is_some_and(|s| !valid_account(s)) || format.is_some_and(|s| !FORMATS.contains(&s)) {
        return Err("세이브 계정 또는 형식을 목록에서 선택하세요.".into());
    }
    ensure_game_closed()?;
    let root = save_path()?;
    let (store, _lock) = lock_store(app_data)?;
    ensure_saves_settled(app_data)?;
    snapshot(
        &root,
        &store,
        label,
        BackupReason::Manual,
        Scope { account, format },
    )?
    .ok_or_else(|| "선택한 조건에 해당하는 세이브 파일이 없습니다.".into())
}

pub fn backup_on_switch(app_data: &Path) -> Result<Option<SaveBackup>> {
    ensure_game_closed()?;
    let root = save_path()?;
    let (store, _lock) = lock_store(app_data)?;
    ensure_saves_settled(app_data)?;
    snapshot(
        &root,
        &store,
        "프로필 전환 전",
        BackupReason::BeforeSwitch,
        Scope::default(),
    )
}

fn restore_at(
    root: &Path,
    store: &Path,
    id: &str,
    check_closed: fn() -> Result<()>,
) -> Result<Option<SaveBackup>> {
    let record = store.join("restore.json");
    if metadata(&record)?.is_some() {
        let job: Restore = read_json(&record)?;
        if job.target != id {
            return Err(format!(
                "중단된 복구를 먼저 완료하세요. 백업 ID {}",
                job.target
            ));
        }
        finish_restore(root, store, &job, check_closed)?;
        return job
            .before
            .map(|key| Ok(manifest(store, &key)?.summary(&key)))
            .transpose();
    }
    verify_backup(store, id)?;
    let before = snapshot(
        root,
        store,
        "세이브 복구 전",
        BackupReason::BeforeRestore,
        Scope::default(),
    )?;
    let job = Restore {
        target: id.to_owned(),
        before: before.as_ref().map(|s| s.id.clone()),
    };
    let tmp = store.join(format!(".restore-{}.tmp", unique_id()));
    write_json(&tmp, &job)?;
    rename_with_retry(&tmp, &record, false, || Ok(()))?;
    finish_restore(root, store, &job, check_closed)?;
    Ok(before)
}

pub fn restore_saves(app_data: &Path, id: &str) -> Result<Option<SaveBackup>> {
    ensure_game_closed()?;
    let (store, _lock) = lock_store(app_data)?;
    restore_at(&save_path()?, &store, id, ensure_game_closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use tempfile::tempdir;

    #[test]
    fn publication_retries_a_real_windows_sharing_violation_without_overwriting() {
        let folder = tempdir().unwrap();
        let source = folder.path().join("new.json");
        let destination = folder.path().join("restore.json");
        fs::write(&source, b"new").unwrap();
        // Allow reads and writes, but deny a rename while the handle is held.
        let no_delete = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&source)
            .unwrap();
        let mut held = Some(no_delete);
        let mut checks = 0;
        rename_with_retry(&source, &destination, false, || {
            checks += 1;
            if checks == 2 {
                drop(held.take());
            }
            Ok(())
        })
        .unwrap();
        assert!((2..=9).contains(&checks));
        fs::write(&source, b"later").unwrap();
        assert!(rename_with_retry(&source, &destination, false, || Ok(())).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"new");
    }

    #[test]
    fn replacement_refuses_a_live_edit_between_rename_attempts() {
        let folder = tempdir().unwrap();
        let source = folder.path().join("restore.tmp");
        let dest = folder.path().join("file.sl2");
        fs::write(&source, b"wanted").unwrap();
        fs::write(&dest, b"previous").unwrap();
        let initial_hash = file_hash(&dest).unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&dest)
            .unwrap();
        let mut attempts = 0;
        let result = rename_with_retry(&source, &dest, true, || {
            attempts += 1;
            if attempts == 2 {
                fs::write(&dest, b"changed").unwrap();
            }
            if file_hash(&dest)? != initial_hash {
                return Err("changed".into());
            }
            Ok(())
        });
        drop(locked);
        assert_eq!(attempts, 2);
        assert_eq!(result.unwrap_err(), "changed");
        assert_eq!(fs::read(dest).unwrap(), b"changed");
        assert_eq!(fs::read(source).unwrap(), b"wanted");
    }

    #[test]
    fn scoped_backup_restores_only_original_account_and_extension_and_backs_up_everything() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("saves");
        let (store, _lock) = lock_store(&dir.path().join("data")).unwrap();
        for account in ["123", "456"] {
            fs::create_dir_all(live.join(account)).unwrap();
            for name in [
                "ER0000.cnv",
                "ER0000.cnv.bak",
                "ER0000.cnvco2",
                "ER0000.rd2",
                "ER0000.co2",
                "ER0000.sl2",
            ] {
                fs::write(live.join(account).join(name), "original").unwrap();
            }
        }
        let scope = Scope {
            account: Some("123"),
            format: Some("cnv"),
        };
        let scoped = snapshot(&live, &store, "첫 보스 직전", BackupReason::Manual, scope)
            .unwrap()
            .unwrap();
        assert_eq!(scoped.files, 2);
        assert_eq!(scoped.accounts, vec!["123"]);
        assert_eq!(scoped.formats, vec!["cnv"]);
        for (account, name) in paths(&live, Scope::default()).unwrap() {
            fs::write(live.join(account).join(name), "newer").unwrap();
        }
        let safety = restore_at(&live, &store, &scoped.id, || Ok(()))
            .unwrap()
            .unwrap();
        assert_eq!(safety.files, 12);
        assert_eq!(safety.reason, BackupReason::BeforeRestore);
        assert_eq!(
            fs::read(live.join("123/ER0000.cnv.bak")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read(live.join("456/ER0000.cnv.bak")).unwrap(), b"newer");
        assert_eq!(fs::read(live.join("123/ER0000.cnvco2")).unwrap(), b"newer");
        assert_eq!(fs::read(live.join("123/ER0000.rd2")).unwrap(), b"newer");
    }

    #[test]
    fn switch_backups_skip_identical_content_and_replace_damaged_backups() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("saves");
        let (store, _lock) = lock_store(&dir.path().join("data")).unwrap();
        let make = || {
            snapshot(
                &live,
                &store,
                "전환 전",
                BackupReason::BeforeSwitch,
                Scope::default(),
            )
            .unwrap()
        };
        assert!(make().is_none());
        fs::create_dir_all(live.join("123")).unwrap();
        let path = live.join("123/mod.RD2.bak");
        fs::write(&path, "123").unwrap();
        let first = make().unwrap();
        assert_eq!(first.reason, BackupReason::BeforeSwitch);
        assert_eq!(first.formats, vec!["rd2"]);
        assert!(make().is_none());
        fs::write(store.join(first.id).join("123/mod.RD2.bak"), "bad").unwrap();
        assert!(make().is_some());
        fs::write(&path, "321").unwrap();
        assert!(make().is_some());
        assert!(make().is_none());
    }

    #[test]
    fn portable_roundtrip_checks_payloads_and_publishes_as_new_backup() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("saves");
        fs::create_dir_all(live.join("123")).unwrap();
        fs::write(live.join("123/game.cnvco2"), b"hello").unwrap();
        let (store, _lock) = lock_store(&dir.path().join("data")).unwrap();
        let original = snapshot(
            &live,
            &store,
            "여행 전",
            BackupReason::Manual,
            Scope::default(),
        )
        .unwrap()
        .unwrap();
        let out = dir.path().join("documents");
        fs::create_dir(&out).unwrap();
        let source = export_backup(&dir.path().join("other-data"), &original.id, &out);
        assert!(source.is_err());
        let record = manifest(&store, &original.id).unwrap();
        let exported =
            copy_snapshot_dir(&store.join(&original.id), &out, &original.id, &record).unwrap();
        assert!(copy_snapshot_dir(&store.join(&original.id), &out, &original.id, &record).is_err());
        assert!(import_at(&store, &store.join(&original.id), &live).is_err());
        let imported = import_at(&store, &exported, &live).unwrap();
        assert_ne!(original.id, imported.id);
        assert_eq!(imported.label, "여행 전");
        assert_eq!(imported.formats, vec!["cnvco2"]);
        assert_eq!(
            fs::read(store.join(imported.id).join("123/game.cnvco2")).unwrap(),
            b"hello"
        );
        fs::write(exported.join("123/game.cnvco2"), b"wrong").unwrap();
        assert!(import_at(&store, &exported, &live).is_err());
        assert_eq!(list_backups(&dir.path().join("data")).unwrap().len(), 2);
    }

    #[test]
    fn older_manifest_keeps_its_backup_without_a_label() {
        let old = r#"{"created":123,"files":[{"account":"11","name":"ER0000.sl2","bytes":3,"sha256":"a"}]}"#;
        let m: Manifest = serde_json::from_str(old).unwrap();
        let info = m.summary("1-2-3");
        assert_eq!(info.reason, BackupReason::Manual);
        assert_eq!(info.label, "");
        assert_eq!(info.formats, vec!["sl2"]);
    }

    #[test]
    fn restore_preserves_prior_saves_and_unrelated_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("saves");
        let data = dir.path().join("data");
        let (store, _lock) = lock_store(&data).unwrap();
        for account in ["1234", "5678"] {
            fs::create_dir_all(root.join(account)).unwrap();
            for name in [
                "ER0000.sl2",
                "ER0000.sl2.bak",
                "ER0000.co2",
                "ER0000.co2.bak",
                "custom.txt",
            ] {
                fs::write(root.join(account).join(name), name).unwrap();
            }
        }
        let original = snapshot(&root, &store, "", BackupReason::Manual, Scope::default())
            .unwrap()
            .unwrap();
        assert_eq!(original.files, 8);
        let save = root.join("5678/ER0000.sl2");
        let unrelated = root.join("1234/custom.txt");
        fs::write(&save, "newer").unwrap();
        fs::write(&unrelated, "keep").unwrap();
        let prior = restore_at(&root, &store, &original.id, || Ok(()))
            .unwrap()
            .unwrap();
        assert_eq!(fs::read_to_string(&save).unwrap(), "ER0000.sl2");
        assert_eq!(fs::read_to_string(&unrelated).unwrap(), "keep");
        assert_eq!(
            fs::read_to_string(store.join(prior.id).join("5678/ER0000.sl2")).unwrap(),
            "newer"
        );
        assert_eq!(list_backups(&data).unwrap().len(), 2);
    }

    #[test]
    fn refuses_damaged_or_escaping_snapshot_before_modification() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("saves");
        let (store, _lock) = lock_store(&dir.path().join("data")).unwrap();
        fs::create_dir_all(root.join("1234")).unwrap();
        let path = root.join("1234/ER0000.sl2");
        fs::write(&path, "original").unwrap();
        let saved = snapshot(&root, &store, "", BackupReason::Manual, Scope::default())
            .unwrap()
            .unwrap();
        fs::write(&path, "keep").unwrap();
        fs::write(store.join(&saved.id).join("1234/ER0000.sl2"), "damaged").unwrap();
        assert!(restore_at(&root, &store, &saved.id, || Ok(())).is_err());
        assert!(restore_at(&root, &store, "../saves", || Ok(())).is_err());
        assert!(!store.join("restore.json").exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep");
        assert!(!save_name("../outside.sl2"));
        assert!(!save_name("NUL.sl2"));
    }

    #[test]
    fn interrupted_restore_can_resume_without_overwriting_an_external_edit() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("saves");
        let (store, _lock) = lock_store(&dir.path().join("data")).unwrap();
        fs::create_dir_all(root.join("1234")).unwrap();
        let save = root.join("1234/ER0000.sl2");
        fs::write(&save, "target").unwrap();
        let target = snapshot(&root, &store, "", BackupReason::Manual, Scope::default())
            .unwrap()
            .unwrap();
        fs::write(&save, "before").unwrap();
        let before = snapshot(&root, &store, "", BackupReason::Manual, Scope::default())
            .unwrap()
            .unwrap();
        write_json(
            &store.join("restore.json"),
            &Restore {
                target: target.id.clone(),
                before: Some(before.id.clone()),
            },
        )
        .unwrap();
        fs::write(&save, "outside edit").unwrap();
        assert!(restore_at(&root, &store, &target.id, || Ok(())).is_err());
        assert_eq!(fs::read_to_string(&save).unwrap(), "outside edit");
        fs::write(&save, "before").unwrap();
        assert_eq!(
            restore_at(&root, &store, &target.id, || Ok(()))
                .unwrap()
                .unwrap()
                .id,
            before.id
        );
        assert_eq!(fs::read_to_string(&save).unwrap(), "target");
        assert!(!store.join("restore.json").exists());
    }
}
