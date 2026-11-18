use super::*;
use fs2::FileExt;
use std::time::Duration;

const SETTINGS_FILE: &str = "periodic-backups.json";
const RETIRED: &str = ".expired-periodic-";

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    enabled: bool,
    interval_minutes: u32,
    keep_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: 5,
            keep_count: 12,
        }
    }
}

impl Settings {
    fn validate(&self) -> Result<()> {
        if !(1..=120).contains(&self.interval_minutes) || !(1..=100).contains(&self.keep_count) {
            return Err("백업 간격은 1~120분, 보관 개수는 1~100개로 선택하세요.".into());
        }
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    settings: Settings,
    last_checked: u64,
    last_backup: u64,
    last_backup_id: Option<String>,
    next_check: u64,
    message: String,
}

pub fn status(data: &Path) -> Result<Status> {
    let path = data.join(SETTINGS_FILE);
    if metadata(&path)?.is_none() {
        return Ok(Status::default());
    }
    let value: Status = read_json(&path)?;
    value.settings.validate()?;
    Ok(value)
}

fn replace_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_file_name(format!(".periodic-{}.tmp", unique_id()));
    let result = write_json(&tmp, value).and_then(|_| {
        rename_with_retry(&tmp, path, true, || {
            metadata(path)?;
            Ok(())
        })
    });
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub fn configure(data: &Path, settings: Settings) -> Result<Status> {
    settings.validate()?;
    let (_store, _lock) = lock_store(data)?;
    let mut value = status(data)?;
    value.settings = settings;
    value.next_check = 0;
    value.message.clear();
    replace_json(&data.join(SETTINGS_FILE), &value)?;
    Ok(value)
}

pub fn pin(data: &Path, id: &str, pinned: bool) -> Result<()> {
    let (store, _lock) = lock_store(data)?;
    let mut record = manifest(&store, id)?;
    if record.reason != BackupReason::Periodic {
        return Err("주기 백업만 고정 상태를 바꿀 수 있습니다.".into());
    }
    record.pinned = pinned;
    replace_json(&store.join(id).join("manifest.json"), &record)
}

// Never acquire write-denying handles on saves while the game may be running.
// Compare the complete set's high-resolution timestamps before and after the
// existing copy/hash check, and wait out a write that has only just finished.
pub(super) fn stable_stamps(
    root: &Path,
    files: &[(String, String)],
) -> Result<Vec<(u64, SystemTime)>> {
    let now = SystemTime::now();
    files
        .iter()
        .map(|(account, name)| {
            let path = root.join(account).join(name);
            let info = metadata(&path)?
                .filter(|m| m.is_file())
                .ok_or("세이브를 교체하는 중입니다.")?;
            let modified = info.modified().map_err(|e| error_at(&path, e))?;
            if info.len() == 0
                || now.duration_since(modified).unwrap_or_default() < Duration::from_secs(2)
            {
                return Err("세이브 쓰기가 끝나기를 기다리고 있습니다.".into());
            }
            Ok((info.len(), modified))
        })
        .collect()
}

// Unpublish first so the backup list never exposes a half-deleted snapshot.
// The hidden manifest doubles as a removal journal. Only its listed payloads
// are removed; unexpected files are left intact.
fn remove_retired(slot: &Path) -> Result<()> {
    if metadata(&slot.join("manifest.json"))?.is_some() {
        let record = manifest_dir(slot)?;
        if record.reason != BackupReason::Periodic || record.pinned {
            return Err(error_at(
                slot,
                "보관할 백업이므로 자동 정리를 중단했습니다.",
            ));
        }
        let mut accounts = BTreeSet::new();
        for item in record.files {
            let dir = slot.join(&item.account);
            if dir_exists(&dir)? {
                let path = item.path(slot);
                if metadata(&path)?.is_some_and(|m| m.is_file()) {
                    fs::remove_file(&path).map_err(|e| error_at(&path, e))?;
                }
                accounts.insert(dir);
            }
        }
        for dir in accounts {
            fs::remove_dir(&dir).map_err(|e| error_at(&dir, e))?;
        }
        let path = slot.join("manifest.json");
        fs::remove_file(&path).map_err(|e| error_at(&path, e))?;
    }
    fs::remove_dir(slot).map_err(|e| error_at(slot, e))
}

fn rotate(store: &Path, keep: usize, last_backup_id: Option<&str>) -> Result<()> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(store).map_err(|e| error_at(store, e))? {
        let entry = entry.map_err(|e| error_at(store, e))?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.strip_prefix(RETIRED).is_some_and(valid_id) && dir_exists(&entry.path())? {
            remove_retired(&entry.path())?;
        } else if valid_id(&id) {
            let record = manifest(store, &id)?;
            if record.reason == BackupReason::Periodic && !record.pinned {
                // A backwards wall clock can date a freshly verified snapshot
                // before older copies. Keep the latest published one first.
                candidates.push((Some(id.as_str()) == last_backup_id, record.created, id));
            }
        }
    }
    candidates.sort_unstable_by(|a, b| b.cmp(a));
    for (_, _, id) in candidates.into_iter().skip(keep) {
        let retired = store.join(format!("{RETIRED}{id}"));
        rename_with_retry(&store.join(&id), &retired, false, || Ok(()))?;
        remove_retired(&retired)?;
    }
    Ok(())
}

fn due(state: &Status, now: u64) -> bool {
    state.settings.enabled && (now >= state.next_check || now < state.last_checked)
}

fn tick(data: &Path, root: &Path, now: u64) -> Result<()> {
    if !due(&status(data)?, now) {
        return Ok(());
    }
    let lock_path = data.join("operation.lock");
    metadata(&lock_path)?;
    let operation = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| error_at(&lock_path, e))?;
    if FileExt::try_lock_exclusive(&operation).is_err() {
        return Ok(());
    }
    let (store, _lock) = lock_store(data)?;
    let mut state = status(data)?;
    if !due(&state, now) {
        return Ok(());
    }
    let outcome = (|| -> Result<String> {
        ensure_saves_settled(data)?;
        let saved = snapshot(
            root,
            &store,
            "주기 백업",
            BackupReason::Periodic,
            Scope::default(),
        )?;
        let message = match saved {
            Some(backup) => {
                state.last_backup = backup.created;
                state.last_backup_id = Some(backup.id);
                format!("변경된 세이브 {}개를 백업했습니다.", backup.files)
            }
            None if paths(root, Scope::default())?.is_empty() => {
                return Ok("백업할 세이브가 없습니다.".into());
            }
            None => "이미 백업한 내용과 같습니다.".into(),
        };
        rotate(
            &store,
            state.settings.keep_count,
            state.last_backup_id.as_deref(),
        )?;
        Ok(message)
    })();
    state.last_checked = now;
    let delay = if outcome.is_ok() {
        u64::from(state.settings.interval_minutes) * 60
    } else {
        15
    };
    state.next_check = now.saturating_add(delay);
    state.message = outcome.unwrap_or_else(|error| format!("15초 후 다시 확인합니다. {error}"));
    replace_json(&data.join(SETTINGS_FILE), &state)
}

pub fn start(data: PathBuf) {
    std::thread::spawn(move || loop {
        if let Ok(root) = save_path() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = tick(&data, &root, now);
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_settled(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(10))
            .unwrap();
    }

    fn enable(data: &Path, keep_count: usize) {
        configure(
            data,
            Settings {
                enabled: true,
                interval_minutes: 1,
                keep_count,
            },
        )
        .unwrap();
    }

    #[test]
    fn persisted_timer_deduplicates_and_keeps_manual_imported_and_pinned_backups() {
        let dir = tempdir().unwrap();
        let data = dir.path().join("app");
        let live = dir.path().join("live");
        let file = live.join("123/ER0000.sl2");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_settled(&file, b"one");
        tick(&data, &live, 100).unwrap();
        assert!(!data.exists());
        enable(&data, 1);
        tick(&data, &live, 100).unwrap();
        let first = list_backups(&data).unwrap().pop().unwrap();
        assert_eq!(first.reason, BackupReason::Periodic);
        assert_eq!(status(&data).unwrap().next_check, 160);
        tick(&data, &live, 160).unwrap();
        assert_eq!(list_backups(&data).unwrap().len(), 1);
        pin(&data, &first.id, true).unwrap();
        let store = store_path(&data);
        let out = dir.path().join("out");
        fs::create_dir(&out).unwrap();
        let exported = copy_snapshot_dir(
            &store.join(&first.id),
            &out,
            &first.id,
            &manifest(&store, &first.id).unwrap(),
        )
        .unwrap();
        let imported = import_at(&store, &exported, &live).unwrap();
        assert!(imported.pinned);
        write_settled(&file, b"two");
        let manual = snapshot(
            &live,
            &store,
            "boss",
            BackupReason::Manual,
            Scope::default(),
        )
        .unwrap()
        .unwrap();
        write_settled(&file, b"three");
        tick(&data, &live, 190).unwrap();
        assert_eq!(list_backups(&data).unwrap().len(), 3);
        tick(&data, &live, 220).unwrap();
        let middle = status(&data).unwrap().last_backup_id.unwrap();
        let mut future_dated = manifest(&store, &middle).unwrap();
        future_dated.created += 86400;
        replace_json(&store.join(&middle).join("manifest.json"), &future_dated).unwrap();
        write_settled(&file, b"four");
        tick(&data, &live, 280).unwrap();
        assert!(!store.join(middle).exists());
        assert!(store.join(manual.id).exists());
        assert!(store.join(&first.id).exists());
        assert!(store.join(imported.id).exists());
        let latest = status(&data).unwrap().last_backup_id.unwrap();
        assert_eq!(
            fs::read(store.join(&latest).join("123/ER0000.sl2")).unwrap(),
            b"four"
        );
        pin(&data, &first.id, false).unwrap();
        tick(&data, &live, 340).unwrap();
        assert!(!store.join(first.id).exists());
        restore_at(&live, &store, &latest, || Ok(())).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"four");
    }

    #[test]
    fn unfinished_writes_global_lock_and_restore_journal_defer_periodic_backup() {
        let dir = tempdir().unwrap();
        let data = dir.path().join("app");
        let live = dir.path().join("live");
        let file = live.join("123/ER0000.co2");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_settled(&file, b"stable");
        enable(&data, 1);
        let op = File::create(data.join("operation.lock")).unwrap();
        FileExt::lock_exclusive(&op).unwrap();
        tick(&data, &live, 100).unwrap();
        assert!(list_backups(&data).unwrap().is_empty());
        drop(op);
        tick(&data, &live, 100).unwrap();
        let first = status(&data).unwrap().last_backup_id.unwrap();
        fs::write(&file, b"still writing").unwrap();
        tick(&data, &live, 160).unwrap();
        assert_eq!(status(&data).unwrap().next_check, 175);
        assert_eq!(list_backups(&data).unwrap().len(), 1);
        let record = store_path(&data).join("restore.json");
        write_json(
            &record,
            &Restore {
                target: first.clone(),
                before: None,
            },
        )
        .unwrap();
        write_settled(&file, b"final");
        tick(&data, &live, 175).unwrap();
        assert_eq!(status(&data).unwrap().next_check, 190);
        assert!(store_path(&data).join(&first).exists());
        fs::remove_file(&record).unwrap();
        tick(&data, &live, 190).unwrap();
        let latest = status(&data).unwrap().last_backup_id.unwrap();
        assert_ne!(first, latest);
        // Simulate exit after a rotation rename and resume cleanup.
        let store = store_path(&data);
        let retired = store.join(format!("{RETIRED}{latest}"));
        fs::rename(store.join(latest), &retired).unwrap();
        tick(&data, &live, 250).unwrap();
        assert!(!retired.exists());
        assert_eq!(list_backups(&data).unwrap().len(), 1);
    }

    #[test]
    fn pre_switch_checkpoint_survives_periodic_rotation_and_duplicate_switches() {
        let dir = tempdir().unwrap();
        let data = dir.path().join("app");
        let live = dir.path().join("live");
        let file = live.join("123/ER0000.sl2");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_settled(&file, b"before mod");
        enable(&data, 1);
        tick(&data, &live, 100).unwrap();
        let first = status(&data).unwrap().last_backup_id.unwrap();
        let store = store_path(&data);
        let save_switch = || {
            snapshot(
                &live,
                &store,
                "전환 전",
                BackupReason::BeforeSwitch,
                Scope::default(),
            )
            .unwrap()
        };
        let before_switch = save_switch().unwrap();
        assert!(save_switch().is_none());
        write_settled(&file, b"after mod");
        tick(&data, &live, 160).unwrap();
        assert!(!store.join(first).exists());
        assert!(verify_backup(&store, &before_switch.id).is_ok());
    }

    #[test]
    fn native_timer_creates_a_restore_point_without_an_open_save_page() {
        worker_fixture(false);
    }

    #[test]
    #[ignore = "release check: wait one real minute for the second automatic backup"]
    fn native_timer_rotates_after_a_real_minute() {
        worker_fixture(true);
    }

    fn worker_fixture(wait_for_rotation: bool) {
        const CHILD_DATA: &str = "ERMA_PERIODIC_TEST_APP_DIRECTORY";
        if let Some(path) = std::env::var_os(CHILD_DATA) {
            let data = PathBuf::from(path);
            let root = data.parent().unwrap().join("roaming/EldenRing");
            assert_eq!(save_path().unwrap(), root);
            start(data.clone());
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            while status(&data).unwrap().last_backup_id.is_none() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "worker did not publish"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            if wait_for_rotation {
                let initial = status(&data).unwrap();
                let started = std::time::Instant::now();
                write_settled(&root.join("123/ER0000.sl2"), b"changed during play");
                while status(&data).unwrap().last_backup_id == initial.last_backup_id {
                    assert!(
                        started.elapsed() < Duration::from_secs(90),
                        "second backup did not publish"
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
                assert!(status(&data).unwrap().last_checked >= initial.last_checked + 60);
                assert!(!store_path(&data)
                    .join(initial.last_backup_id.unwrap())
                    .exists());
            }
            return;
        }
        let dir = tempdir().unwrap();
        let data = dir.path().join("app");
        let roaming = dir.path().join("roaming");
        let file = roaming.join("EldenRing/123/ER0000.sl2");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        write_settled(&file, b"isolated save");
        enable(&data, 1);
        let current = std::thread::current();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--include-ignored", "--exact", current.name().unwrap()])
            .env(CHILD_DATA, &data)
            .env("APPDATA", &roaming)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        let created = status(&data).unwrap().last_backup_id.unwrap();
        assert_eq!(
            fs::read(store_path(&data).join(created).join("123/ER0000.sl2")).unwrap(),
            if wait_for_rotation {
                &b"changed during play"[..]
            } else {
                &b"isolated save"[..]
            }
        );
    }
}
