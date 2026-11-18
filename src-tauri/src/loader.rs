use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

type Result<T> = std::result::Result<T, String>;
const CREATE_NO_WINDOW: u32 = 0x08000000;
const PORTABLE: &str = "me3-0.13.0";
const BINARIES: [(&str, &str); 3] = [
    (
        "me3.exe",
        "203a726ab40ecac6ee9deaba1aa96f741d87e12def549cdeb48f99e5fe16d52d",
    ),
    (
        "me3-launcher.exe",
        "4195535ac8448d20e620bc911772a18fc75462e4c36668968164842d787d0011",
    ),
    (
        "me3_mod_host.dll",
        "e08af3dd9d7be2655e2f245fa2bfc316aa8c6af74a3f2dc7cb0598f96c769317",
    ),
];

#[derive(Serialize)]
pub(crate) struct Status {
    pub installed: bool,
    pub path: Option<PathBuf>,
}

fn usable(exe: &Path) -> bool {
    exe.is_absolute()
        && exe
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("me3.exe"))
        && exe
            .parent()
            .is_some_and(|p| BINARIES.iter().all(|(file, _)| p.join(file).is_file()))
}

fn bundled() -> Option<PathBuf> {
    Some(
        std::env::current_exe()
            .ok()?
            .parent()?
            .join(PORTABLE)
            .join("bin/me3.exe"),
    )
}

pub(crate) fn status(data: &Path) -> Status {
    let selected = fs::read(data.join("me3-path.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<PathBuf>(&b).ok());
    let registry: Option<PathBuf> = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\me3")
        .ok()
        .and_then(|key| key.get_value::<String, _>("InstallLocation").ok())
        .map(|p| Path::new(&p).join("bin/me3.exe"));
    let local = std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("Programs/garyttierney/me3/bin/me3.exe"));
    let path = [selected, bundled(), registry, local]
        .into_iter()
        .flatten()
        .find(|p| usable(p));
    Status {
        installed: path.is_some(),
        path,
    }
}

pub(crate) fn select(data: &Path, english: Option<bool>) -> Result<Option<Status>> {
    let Some(exe) = rfd::FileDialog::new()
        .set_title(if english == Some(true) { "Select me3.exe from the installed me3 bin folder" } else { "설치한 me3의 bin 폴더에서 me3.exe 선택" })
        .add_filter(if english == Some(true) { "me3 executable" } else { "me3 실행 파일" }, &["exe"])
        .pick_file()
    else {
        return Ok(None);
    };
    if !usable(&exe) {
        return Err("me3.exe와 공식 배포본의 보조 파일이 함께 있는 bin 폴더를 선택하세요.".into());
    }
    fs::create_dir_all(data).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::NamedTempFile::new_in(data).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut tmp, &exe).map_err(|e| e.to_string())?;
    tmp.flush()
        .and_then(|_| tmp.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    tmp.persist(data.join("me3-path.json"))
        .map_err(|e| e.to_string())?;
    Ok(Some(Status {
        installed: true,
        path: Some(exe),
    }))
}

fn verify_bundle(exe: &Path) -> Result<()> {
    if bundled().as_deref() == Some(exe) {
        let bin = exe.parent().ok_or("me3 폴더를 찾을 수 없습니다.")?;
        for (name, digest) in BINARIES {
            let mut file = File::open(bin.join(name)).map_err(|e| e.to_string())?;
            let mut hash = Sha256::new();
            io::copy(&mut file, &mut hash).map_err(|e| e.to_string())?;
            if format!("{:x}", hash.finalize()) != digest {
                return Err(format!(
                    "함께 배포된 me3 파일이 공식 배포본과 다릅니다. {name}"
                ));
            }
        }
    }
    Ok(())
}

fn command(exe: &Path, profile: &Path, game: &Path) -> Command {
    let bin = exe.parent().expect("verified executable directory");
    let mut child = Command::new(exe);
    child
        .current_dir(bin)
        .arg("--crash-reporting=false")
        .arg("--windows-binaries-dir")
        .arg(bin)
        .args(["launch", "--game", "eldenring", "--online=false", "--exe"])
        .arg(game.join("eldenring.exe"))
        .arg("--profile")
        .arg(profile.join(crate::modpack::PROFILE_FILE))
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
    child
}

pub(crate) fn launch(data: &Path, game: &Path, profile: &Path) -> Result<()> {
    crate::platform::steam_build(game)?;
    crate::modpack::check_profile(profile)?;
    if profile.join("preserved").is_dir()
        && crate::modpack::walk(&profile.join(crate::modpack::PACKAGE_DIR))?.is_empty()
        && crate::modpack::native_bytes(profile)? == 0
    {
        return Err("이 프로필에는 별도 보관 파일만 있습니다. 프로필 폴더의 preserved 내용을 확인하고 제작자의 안내에 따라 파일 또는 DLL 모드를 추가하세요.".into());
    }
    let exe = status(data)
        .path
        .ok_or("me3 로더가 없습니다. 설치 설정에서 me3.exe를 선택하세요.")?;
    verify_bundle(&exe)?;
    let log_path = data.join("me3-last-launch.log");
    let log = File::create(&log_path).map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    crate::platform::ensure_game_closed()?;
    let mut child = command(&exe, profile, game)
        .stdout(log)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("me3를 시작할 수 없습니다. {e}"))?;
    // Surface immediate CLI/configuration failures without blocking the app for the play session.
    for _ in 0..12 {
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            return if exit.success() {
                Ok(())
            } else {
                Err(format!(
                    "me3 실행이 종료되었습니다({exit}). 실행 기록을 확인하세요. {}",
                    log_path.display()
                ))
            };
        }
        std::thread::sleep(Duration::from_millis(125));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_uses_exact_game_and_managed_profile_as_separate_arguments() {
        let game = Path::new(r"C:\Steam Library\ELDEN RING\Game");
        let profile = Path::new(r"D:\모드 & 자료\profile-1");
        let cmd = command(Path::new(r"C:\Assist\me3\bin\me3.exe"), profile, game);
        let args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--profile", profile.join("profile.me3").to_str().unwrap()]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--exe", game.join("eldenring.exe").to_str().unwrap()]));
        assert!(args.iter().any(|a| a == "--online=false"));
    }
}
