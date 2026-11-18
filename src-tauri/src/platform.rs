pub(crate) mod saves;
mod steam;

pub use saves::{
    backup_on_switch, backup_saves, backup_selected, ensure_saves_settled, export_backup,
    import_backup, list_backups, list_save_files, live_save_path, restore_saves, SaveBackup,
    SaveFile,
};
pub use steam::{
    detect_game, installed_depots, steam_build, steam_install_state, steam_launch, steam_root,
    steam_verify,
};

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

type Result<T> = std::result::Result<T, String>;

fn error_at(path: &Path, error: impl std::fmt::Display) -> String {
    format!("{} — {error}", path.display())
}

// On Windows Rust flags all name-surrogate reparse tags (including junctions)
// as symlinks. Ordinary OneDrive placeholders are still files/directories.
fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() || !(info.is_file() || info.is_dir()) => {
            Err(format!(
                "연결된 파일이나 폴더는 처리할 수 없습니다. {}",
                path.display()
            ))
        }
        Ok(info) => Ok(Some(info)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(error_at(path, e)),
    }
}

pub fn ensure_game_closed() -> Result<()> {
    // Toolhelp returns only process names here; no process is opened or modified.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(format!(
                "실행 중인 게임을 확인할 수 없습니다. {error}",
                error = io::Error::last_os_error()
            ));
        }
        let mut entry = PROCESSENTRY32W::default();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let result = if Process32FirstW(snapshot, &mut entry) == 0 {
            Err("프로세스 목록을 읽을 수 없습니다.".to_owned())
        } else {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "eldenring.exe"
                        | "start_protected_game.exe"
                        | "ersc_launcher.exe"
                        | "easyanticheat.exe"
                        | "easyanticheat_eos.exe"
                        | "eldenring_alt_saves.exe"
                        | "me3-launcher.exe"
                ) {
                    break Err(format!(
                        "{name} 실행 중입니다. 게임과 보호 런처가 종료된 뒤 다시 시도하세요."
                    ));
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    let code = GetLastError();
                    break if code == ERROR_NO_MORE_FILES {
                        Ok(())
                    } else {
                        Err(format!(
                            "프로세스 목록을 끝까지 확인할 수 없습니다. Windows 오류 {code}"
                        ))
                    };
                }
            }
        };
        CloseHandle(snapshot);
        result
    }
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

fn shell_open(target: &OsStr) -> Result<()> {
    let target: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            ptr::null(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        return Err(format!("Windows에서 항목을 열 수 없습니다. 오류 {result}"));
    }
    Ok(())
}

pub fn open_loader_download() -> Result<()> {
    shell_open(OsStr::new("https://github.com/garyttierney/me3/releases"))
}

pub fn open_korean_map() -> Result<()> {
    shell_open(OsStr::new("https://edring.lsdl.kr/map"))
}

pub fn open_smithbox() -> Result<()> {
    shell_open(OsStr::new("https://github.com/vawser/Smithbox"))
}

pub fn open_folder(path: &Path) -> Result<()> {
    if !path.is_absolute() || !metadata(path)?.is_some_and(|m| m.is_dir()) {
        return Err("열 수 있는 폴더가 없습니다.".into());
    }
    shell_open(path.as_os_str())
}
