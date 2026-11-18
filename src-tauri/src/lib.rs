mod adoption;
mod depot;
mod loader;
mod modpack;
mod platform;
mod progress;
mod slots;
mod switcher;

use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager};

type Result<T> = std::result::Result<T, String>;

async fn worker<T: Send + 'static>(
    action: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(action)
        .await
        .map_err(|e| format!("작업이 중단되었습니다. {e}"))?
}

fn data_directory(app: &AppHandle) -> Result<PathBuf> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| format!("앱 저장 폴더를 찾을 수 없습니다. {e}"))
}

// The OS releases the lock on exit. A second window cannot interleave a save
// restore, profile transition or Steam handoff with an operation in progress.
async fn operation<T: Send + 'static>(
    app: AppHandle,
    action: impl FnOnce(&Path) -> Result<T> + Send + 'static,
) -> Result<T> {
    let data = data_directory(&app)?;
    worker(move || {
        fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(data.join("operation.lock")).map_err(|e| e.to_string())?;
        lock.try_lock_exclusive().map_err(|_| "다른 창에서 작업 중입니다. 완료 후 다시 시도하세요.".to_owned())?;
        progress::start(0);
        // Steam starts the protected game asynchronously. Cover the gap before
        // the game or EAC process exists, including when the app is restarted.
        let handoff = data.join("steam-handoff");
        if let Ok(info) = handoff.metadata() {
            if SystemTime::now().duration_since(info.modified().map_err(|e| e.to_string())?)
                .unwrap_or_default() < Duration::from_secs(60) {
                return Err("게임 실행 또는 Steam 검사를 요청했습니다. 작업을 마치고 요청 시점에서 1분 후 다시 시도하세요.".into());
            }
        }
        platform::ensure_game_closed()?;
        action(&data)
    }).await
}

#[tauri::command]
fn progress() -> progress::Progress {
    progress::read()
}

#[tauri::command]
async fn detect_game() -> Result<Option<PathBuf>> {
    worker(platform::detect_game).await
}

#[tauri::command]
async fn pick_game(english: Option<bool>) -> Result<Option<PathBuf>> {
    worker(move || Ok(platform::pick_folder(if english == Some(true) { "Select the Game folder containing eldenring.exe" } else { "eldenring.exe가 있는 Game 폴더 선택" }))).await
}

#[tauri::command]
async fn pick_source(english: Option<bool>) -> Result<Option<PathBuf>> {
    worker(move || Ok(platform::pick_folder(if english == Some(true) { "Select a folder to import into the profile" } else { "프로필에 가져올 폴더 선택" }))).await
}

#[tauri::command]
async fn status(game_path: PathBuf) -> Result<switcher::View> {
    worker(move || switcher::inspect(&game_path)).await
}

#[tauri::command]
async fn onboard_modded(
    app: AppHandle,
    game_path: PathBuf,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| switcher::onboard_modded(&game_path, &name)).await
}

#[tauri::command]
async fn baseline(app: AppHandle, game_path: PathBuf) -> Result<switcher::View> {
    operation(app, move |_| switcher::baseline(&game_path)).await
}

#[tauri::command]
async fn create_profile(
    app: AppHandle,
    game_path: PathBuf,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| switcher::create_profile(&game_path, &name)).await
}

#[tauri::command]
async fn import_profile(
    app: AppHandle,
    game_path: PathBuf,
    source: PathBuf,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| {
        switcher::import_profile(&game_path, &source, &name)
    })
    .await
}

#[tauri::command]
async fn preview_mods(
    app: AppHandle,
    game_path: PathBuf,
    source: PathBuf,
) -> Result<modpack::ModPreview> {
    operation(app, move |_| switcher::preview_mods(&game_path, &source)).await
}

#[tauri::command]
async fn profile_from_mods(
    app: AppHandle,
    game_path: PathBuf,
    source: PathBuf,
    name: String,
    review_token: String,
    keep_existing: Vec<String>,
) -> Result<switcher::View> {
    operation(app, move |_| {
        switcher::profile_from_review(&game_path, &source, &name, &review_token, &keep_existing)
    })
    .await
}

#[tauri::command]
async fn switch_profile(
    app: AppHandle,
    game_path: PathBuf,
    id: String,
    save_backup: Option<bool>,
) -> Result<switcher::View> {
    operation(app, move |data| {
        if save_backup.unwrap_or(true)
            && switcher::inspect(&game_path)?.active_id.as_deref() != Some(id.as_str())
        {
            platform::backup_on_switch(data)?;
        }
        switcher::switch(&game_path, &id)
    })
    .await
}

#[tauri::command]
async fn undo_park(app: AppHandle, game_path: PathBuf, id: String) -> Result<switcher::View> {
    operation(app, move |_| switcher::undo_park(&game_path, &id)).await
}

#[tauri::command]
async fn rename_profile(
    app: AppHandle,
    game_path: PathBuf,
    id: String,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| {
        switcher::rename_profile(&game_path, &id, &name)
    })
    .await
}

#[tauri::command]
async fn release_profile(app: AppHandle, game_path: PathBuf, id: String) -> Result<PathBuf> {
    operation(app, move |_| switcher::release_profile(&game_path, &id)).await
}

#[tauri::command]
async fn recover(app: AppHandle, game_path: PathBuf) -> Result<switcher::View> {
    operation(app, move |_| switcher::recover(&game_path)).await
}

#[tauri::command]
async fn verify_vanilla(app: AppHandle, game_path: PathBuf) -> Result<switcher::Report> {
    operation(app, move |_| switcher::verify(&game_path)).await
}

#[tauri::command]
async fn audit_install(app: AppHandle, game_path: PathBuf) -> Result<switcher::Report> {
    operation(app, move |_| depot::audit_install(&game_path)).await
}

#[tauri::command]
async fn launch_vanilla(app: AppHandle, game_path: PathBuf) -> Result<()> {
    operation(app, move |data| {
        platform::ensure_saves_settled(data)?;
        switcher::launch_verified(&game_path, || {
            handoff(data, || platform::steam_launch(&game_path))
        })
    })
    .await
}

fn handoff(data: &Path, open: impl FnOnce() -> Result<()>) -> Result<()> {
    let marker = data.join("steam-handoff");
    fs::write(&marker, b"").map_err(|e| e.to_string())?;
    if let Err(e) = open() {
        let _ = fs::remove_file(&marker);
        return Err(e);
    }
    Ok(())
}

#[tauri::command]
async fn steam_verify(app: AppHandle, game_path: PathBuf) -> Result<()> {
    operation(app, move |data| {
        let state = switcher::inspect(&game_path)?;
        if state.phase == "recovery" || state.active_id.as_deref().is_some_and(|id| id != "vanilla")
        {
            return Err("전환 복구를 마치고 바닐라로 전환한 뒤 Steam 검사를 시작하세요.".into());
        }
        handoff(data, || platform::steam_verify(&game_path))
    })
    .await
}

#[tauri::command]
async fn open_game_folder(game_path: PathBuf) -> Result<()> {
    worker(move || {
        switcher::inspect(&game_path)?;
        platform::open_folder(&game_path)
    })
    .await
}

#[tauri::command]
async fn convert_profile(
    app: AppHandle,
    game_path: PathBuf,
    id: String,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| {
        switcher::convert_profile(&game_path, &id, &name)
    })
    .await
}

#[tauri::command]
async fn open_profile_folder(game_path: PathBuf, id: String) -> Result<()> {
    worker(move || platform::open_folder(&switcher::package_folder(&game_path, &id)?)).await
}

#[tauri::command]
async fn launch_modded(app: AppHandle, game_path: PathBuf) -> Result<()> {
    operation(app, move |data| {
        platform::ensure_saves_settled(data)?;
        let profile = switcher::active_package(&game_path)?;
        handoff(data, || loader::launch(data, &game_path, &profile))
    })
    .await
}

#[tauri::command]
async fn loader_status(app: AppHandle) -> Result<loader::Status> {
    let data = data_directory(&app)?;
    worker(move || Ok(loader::status(&data))).await
}

#[tauri::command]
async fn select_loader(app: AppHandle, english: Option<bool>) -> Result<Option<loader::Status>> {
    let data = data_directory(&app)?;
    worker(move || loader::select(&data, english)).await
}

#[tauri::command]
async fn open_loader_download() -> Result<()> {
    worker(platform::open_loader_download).await
}

#[tauri::command]
async fn open_korean_map() -> Result<()> {
    worker(platform::open_korean_map).await
}

#[tauri::command]
async fn open_smithbox() -> Result<()> {
    worker(platform::open_smithbox).await
}

#[tauri::command]
async fn pick_native_entry(source: PathBuf, english: Option<bool>) -> Result<Option<PathBuf>> {
    worker(move || {
        if !source.is_absolute() || !source.is_dir() {
            return Err("압축을 푼 모드 폴더를 선택하세요.".into());
        }
        Ok(rfd::FileDialog::new()
            .set_title(if english == Some(true) { "Select the mod DLL specified by the author for me3" } else { "제작자가 me3용으로 지정한 모드 DLL 선택" })
            .set_directory(source)
            .add_filter(if english == Some(true) { "Mod DLL" } else { "모드 DLL" }, &["dll"])
            .pick_file())
    })
    .await
}

#[tauri::command]
async fn profile_from_native(
    app: AppHandle,
    game_path: PathBuf,
    source: PathBuf,
    entry: PathBuf,
    name: String,
) -> Result<switcher::View> {
    operation(app, move |_| {
        switcher::profile_from_native(&game_path, &source, &entry, &name)
    })
    .await
}

#[tauri::command]
async fn list_backups(app: AppHandle) -> Result<Vec<platform::SaveBackup>> {
    let data = data_directory(&app)?;
    worker(move || platform::list_backups(&data)).await
}

#[tauri::command]
async fn backup_saves(app: AppHandle) -> Result<Vec<platform::SaveBackup>> {
    operation(app, |data| {
        platform::backup_saves(data)?;
        platform::list_backups(data)
    })
    .await
}

#[tauri::command]
async fn restore_saves(app: AppHandle, id: String) -> Result<Vec<platform::SaveBackup>> {
    operation(app, move |data| {
        platform::restore_saves(data, &id)?;
        platform::list_backups(data)
    })
    .await
}

#[tauri::command]
async fn list_save_files() -> Result<Vec<platform::SaveFile>> {
    worker(platform::list_save_files).await
}

#[tauri::command]
async fn save_slots(account: String, name: String) -> Result<Vec<slots::Slot>> {
    worker(move || slots::read(&platform::live_save_path(&account, &name)?)).await
}

#[tauri::command]
async fn backup_selected(
    app: AppHandle,
    label: String,
    account: Option<String>,
    format: Option<String>,
) -> Result<Vec<platform::SaveBackup>> {
    operation(app, move |data| {
        platform::backup_selected(
            data,
            &label,
            account.as_deref(),
            format.as_deref().map(|f| f.trim_start_matches('.')),
        )?;
        platform::list_backups(data)
    })
    .await
}

#[tauri::command]
async fn export_backup(app: AppHandle, id: String, english: Option<bool>) -> Result<Option<PathBuf>> {
    let Some(parent) =
        worker(move || Ok(platform::pick_folder(if english == Some(true) { "Select a destination for the backup copy" } else { "백업 복사본을 저장할 폴더 선택" }))).await?
    else {
        return Ok(None);
    };
    operation(app, move |data| {
        if let Some(game) = platform::detect_game()? {
            let canonical = fs::canonicalize(&parent).map_err(|e| e.to_string())?;
            if game
                .parent()
                .and_then(|p| fs::canonicalize(p).ok())
                .is_some_and(|install| canonical.starts_with(&install))
            {
                return Err("게임 설치 폴더 밖에 백업을 보관하세요.".into());
            }
        }
        platform::export_backup(data, &id, &parent).map(Some)
    })
    .await
}

#[tauri::command]
async fn import_backup(app: AppHandle, english: Option<bool>) -> Result<Option<Vec<platform::SaveBackup>>> {
    let Some(source) = worker(move || {
        Ok(platform::pick_folder(
            if english == Some(true) { "Select the exported backup folder containing manifest.json" } else { "내보낸 백업의 manifest.json이 있는 폴더 선택" },
        ))
    })
    .await?
    else {
        return Ok(None);
    };
    operation(app, move |data| {
        platform::import_backup(data, &source)?;
        platform::list_backups(data).map(Some)
    })
    .await
}

use platform::saves::periodic;

#[tauri::command]
async fn periodic_status(app: AppHandle) -> Result<periodic::Status> {
    let data = data_directory(&app)?;
    worker(move || periodic::status(&data)).await
}

#[tauri::command]
async fn set_periodic(app: AppHandle, settings: periodic::Settings) -> Result<periodic::Status> {
    let data = data_directory(&app)?;
    worker(move || periodic::configure(&data, settings)).await
}

#[tauri::command]
async fn pin_periodic(app: AppHandle, id: String, pinned: bool) -> Result<()> {
    let data = data_directory(&app)?;
    worker(move || periodic::pin(&data, &id, pinned)).await
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            periodic::start(data_directory(app.handle())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            periodic_status,
            set_periodic,
            pin_periodic,
            preview_mods,
            open_korean_map,
            open_smithbox,
            loader_status,
            select_loader,
            open_loader_download,
            launch_modded,
            convert_profile,
            open_profile_folder,
            pick_native_entry,
            profile_from_native,
            detect_game,
            pick_game,
            pick_source,
            status,
            progress,
            audit_install,
            onboard_modded,
            baseline,
            create_profile,
            import_profile,
            profile_from_mods,
            switch_profile,
            recover,
            verify_vanilla,
            launch_vanilla,
            steam_verify,
            open_game_folder,
            list_backups,
            backup_saves,
            restore_saves,
            list_save_files,
            save_slots,
            backup_selected,
            undo_park,
            rename_profile,
            export_backup,
            import_backup,
            release_profile
        ])
        .run(tauri::generate_context!())
        .expect("Elden Ring Mod Assist 창을 열 수 없습니다.");
}
