use super::{ensure_game_closed, error_at, metadata, shell_open, Result};
use std::path::{Component, Path, PathBuf};
use std::{env, fs, io};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};
use winreg::RegKey;

const APP_ID: &str = "1245620";

#[derive(Debug)]
enum Value {
    Text(String),
    Map(Vec<(String, Value)>),
}

impl Value {
    fn pairs(&self) -> &[(String, Value)] {
        if let Self::Map(entries) = self {
            entries
        } else {
            &[]
        }
    }
    fn get(&self, key: &str) -> Option<&Self> {
        self.pairs()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }
    fn text(&self) -> Option<&str> {
        if let Self::Text(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn field(&self, key: &str) -> Option<&str> {
        self.get(key)?.text()
    }
}

// Steam uses KeyValues for both the library list and installed-app records.
// Parse structure so similarly named nested fields cannot stand in for AppState fields.
struct Vdf<'a> {
    input: &'a [u8],
    pos: usize,
}
impl<'a> Vdf<'a> {
    fn space(&mut self) {
        loop {
            while self
                .input
                .get(self.pos)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.pos += 1;
            }
            if !self.input[self.pos..].starts_with(b"//") {
                break;
            }
            self.pos += self.input[self.pos..]
                .iter()
                .position(|&c| c == b'\n')
                .unwrap_or(self.input.len() - self.pos);
        }
    }
    fn word(&mut self) -> Result<String> {
        self.space();
        let mut out = Vec::new();
        if self.input.get(self.pos) == Some(&b'"') {
            self.pos += 1;
            loop {
                let &ch = self
                    .input
                    .get(self.pos)
                    .ok_or("Steam 설정 파일의 문자열이 끊겼습니다.")?;
                self.pos += 1;
                if ch == b'"' {
                    break;
                }
                if ch == b'\\' && matches!(self.input.get(self.pos), Some(b'"' | b'\\')) {
                    out.push(self.input[self.pos]);
                    self.pos += 1;
                } else {
                    out.push(ch);
                }
            }
        } else {
            let start = self.pos;
            while self
                .input
                .get(self.pos)
                .is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'}' | b'{' | b'"'))
            {
                self.pos += 1;
            }
            if self.pos == start {
                return Err("Steam 설정 파일의 항목 형식을 확인하세요.".into());
            }
            out.extend_from_slice(&self.input[start..self.pos]);
        }
        String::from_utf8(out).map_err(|_| "Steam 설정 파일의 문자 인코딩을 확인하세요.".into())
    }
    fn entries(&mut self, depth: usize) -> Result<Value> {
        if depth > 64 {
            return Err("Steam 설정 파일의 중첩 단계가 너무 많습니다.".into());
        }
        let mut pairs = Vec::new();
        loop {
            self.space();
            match self.input.get(self.pos) {
                None if depth == 0 => break,
                Some(b'}') if depth > 0 => {
                    self.pos += 1;
                    break;
                }
                None | Some(b'}') => {
                    return Err("Steam 설정 파일의 중괄호가 일치하지 않습니다.".into())
                }
                _ => (),
            }
            let key = self.word()?;
            self.space();
            let value = if self.input.get(self.pos) == Some(&b'{') {
                self.pos += 1;
                self.entries(depth + 1)?
            } else {
                Value::Text(self.word()?)
            };
            pairs.push((key, value));
        }
        Ok(Value::Map(pairs))
    }
}

fn read_vdf(path: &Path) -> Result<Option<Value>> {
    let content = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(error_at(path, e)),
    };
    let input = content.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&content);
    Vdf { input, pos: 0 }
        .entries(0)
        .map(Some)
        .map_err(|e| error_at(path, e))
}

fn root_from_registry() -> Option<PathBuf> {
    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .ok();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"Software\Valve\Steam", KEY_READ | KEY_WOW64_32KEY)
        .ok();
    let paths = [
        user.and_then(|k| k.get_value::<String, _>("SteamPath").ok())
            .map(PathBuf::from),
        machine
            .and_then(|k| k.get_value::<String, _>("InstallPath").ok())
            .map(PathBuf::from),
        env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Steam")),
    ];
    paths
        .into_iter()
        .flatten()
        .find(|p| p.is_absolute() && p.join("steam.exe").is_file())
}

pub fn steam_root() -> Result<PathBuf> {
    root_from_registry().ok_or_else(|| "Steam 설치 위치를 찾을 수 없습니다.".into())
}

fn libraries(steam: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = vec![steam.to_owned()];
    if let Some(vdf) = read_vdf(&steam.join("steamapps/libraryfolders.vdf"))? {
        let folders = vdf
            .get("libraryfolders")
            .ok_or("Steam 라이브러리 목록을 확인하세요.")?;
        for (index, value) in folders.pairs() {
            if index.parse::<u32>().is_err() {
                continue;
            }
            if let Some(p) = value.field("path").or_else(|| value.text()) {
                let dir = PathBuf::from(p);
                if dir.is_absolute() && !paths.iter().any(|known| same_location(known, &dir)) {
                    paths.push(dir);
                }
            }
        }
    }
    Ok(paths)
}

fn same_location(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x
            .to_string_lossy()
            .eq_ignore_ascii_case(&y.to_string_lossy()),
        _ => a == b,
    }
}

struct Install {
    game: PathBuf,
    manifest: Value,
}

fn installations(steam: &Path) -> Result<Vec<Install>> {
    let mut installs = Vec::new();
    for library in libraries(steam)? {
        let apps = library.join("steamapps");
        let Some(vdf) = read_vdf(&apps.join("appmanifest_1245620.acf"))? else {
            continue;
        };
        let state = vdf
            .get("AppState")
            .ok_or("Steam의 ELDEN RING 설치 기록이 손상되었습니다.")?;
        if state.field("appid") != Some(APP_ID) {
            return Err("Steam 설치 기록의 게임 ID가 다릅니다.".into());
        }
        let dirname = state
            .field("installdir")
            .ok_or("Steam 설치 기록에 폴더명이 없습니다.")?;
        let mut parts = Path::new(dirname).components();
        if !matches!(parts.next(), Some(Component::Normal(_)))
            || parts.next().is_some()
            || dirname.contains(['/', '\\', ':'])
        {
            return Err("Steam 설치 기록의 게임 폴더명이 올바르지 않습니다.".into());
        }
        let game = apps.join("common").join(dirname).join("Game");
        if !installs
            .iter()
            .any(|i: &Install| same_location(i.game.parent().unwrap(), game.parent().unwrap()))
        {
            installs.push(Install {
                game,
                manifest: vdf,
            });
        }
    }
    Ok(installs)
}

pub fn detect_game() -> Result<Option<PathBuf>> {
    let Some(steam) = root_from_registry() else {
        return Ok(None);
    };
    let installs = installations(&steam)?;
    if installs.len() > 1 {
        return Err("여러 Steam 라이브러리에 ELDEN RING 설치 기록이 있습니다. Steam 저장 공간 설정을 확인하세요.".into());
    }
    Ok(installs.into_iter().next().map(|i| i.game))
}

fn match_install(game: &Path) -> Result<Option<Install>> {
    if !game
        .file_name()
        .is_some_and(|s| s.eq_ignore_ascii_case("Game"))
    {
        return Ok(None);
    }
    let Some(steam) = root_from_registry() else {
        return Ok(None);
    };
    let installs = installations(&steam)?;
    let count = installs.len();
    let Some(found) = installs.into_iter().find(|i| {
        game.parent()
            .is_some_and(|p| same_location(p, i.game.parent().unwrap()))
    }) else {
        return Ok(None);
    };
    if count > 1 {
        return Err(
            "ELDEN RING 설치 기록이 여러 개여서 Steam의 실행 경로를 확정할 수 없습니다.".into(),
        );
    }
    // A parked Game directory is expected during a fresh Steam verification.
    metadata(&found.game)?;
    metadata(found.game.parent().unwrap())?;
    Ok(Some(found))
}

fn current_build(state: &Value) -> Result<String> {
    let build = state
        .field("buildid")
        .filter(|s| s.parse::<u64>().is_ok_and(|n| n != 0));
    if state.field("StateFlags") != Some("4") || build.is_none() {
        return Err(
            "Steam에서 ELDEN RING 설치·업데이트·무결성 검사가 끝난 뒤 다시 시도하세요.".into(),
        );
    }
    let build = build.unwrap();
    if state
        .field("TargetBuildID")
        .is_some_and(|s| s != "0" && s != build)
        || state.field("UpdateResult").is_some_and(|s| s != "0")
    {
        return Err(
            "Steam에 대기 중이거나 실패한 게임 업데이트가 있습니다. 다운로드 상태를 확인하세요."
                .into(),
        );
    }
    Ok(build.to_owned())
}

pub fn steam_install_state(game: &Path) -> Result<Option<String>> {
    match_install(game)?
        .map(|i| current_build(i.manifest.get("AppState").unwrap()))
        .transpose()
}

pub fn steam_build(game: &Path) -> Result<String> {
    steam_install_state(game)?
        .ok_or_else(|| "Steam 라이브러리에 등록된 ELDEN RING의 Game 폴더를 선택하세요.".into())
}

pub fn installed_depots(game: &Path) -> Result<Vec<(String, String)>> {
    let install =
        match_install(game)?.ok_or("Steam이 선택한 게임 폴더를 관리하고 있지 않습니다.")?;
    let state = install.manifest.get("AppState").unwrap();
    current_build(state)?;
    let depots = state
        .get("InstalledDepots")
        .ok_or("Steam 설치 기록에 콘텐츠 목록이 없습니다.")?;
    let mut result = Vec::new();
    for (id, info) in depots.pairs() {
        let manifest = info
            .field("manifest")
            .ok_or("Steam 콘텐츠 기록의 버전 정보가 없습니다.")?;
        if id.parse::<u32>().is_err() || manifest.parse::<u64>().is_err() {
            return Err("Steam 콘텐츠 ID 또는 버전 번호가 올바르지 않습니다.".into());
        }
        result.push((id.clone(), manifest.to_owned()));
    }
    if result.is_empty() {
        return Err("Steam 설치 기록에 콘텐츠가 없습니다.".into());
    }
    Ok(result)
}

pub fn steam_verify(game: &Path) -> Result<()> {
    ensure_game_closed()?;
    match_install(game)?.ok_or("Steam에 등록된 ELDEN RING의 Game 폴더를 선택하세요.")?;
    shell_open("steam://validate/1245620".as_ref())
}

fn check_launch_options(steam: &Path) -> Result<()> {
    let userdata = steam.join("userdata");
    let active_id: Option<u32> = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam\ActiveProcess")
        .ok()
        .and_then(|key| key.get_value("ActiveUser").ok())
        .filter(|id| *id != 0);
    for entry in fs::read_dir(&userdata).map_err(|e| error_at(&userdata, e))? {
        let entry = entry.map_err(|e| error_at(&userdata, e))?;
        let Ok(id) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if active_id.is_some_and(|active| active != id) {
            continue;
        }
        let Some(cfg) = read_vdf(&entry.path().join("config/localconfig.vdf"))? else {
            continue;
        };
        let options = [
            "UserLocalConfigStore",
            "Software",
            "Valve",
            "Steam",
            "apps",
            APP_ID,
        ]
        .iter()
        .try_fold(&cfg, |node, key| node.get(key))
        .and_then(|app| app.field("LaunchOptions"));
        if options.is_some_and(|s| !s.trim().is_empty()) {
            return Err(
                "Steam의 ELDEN RING 속성에서 사용자 지정 실행 옵션을 비운 뒤 다시 시도하세요."
                    .into(),
            );
        }
    }
    Ok(())
}

pub fn steam_launch(game: &Path) -> Result<()> {
    ensure_game_closed()?;
    steam_build(game)?;
    check_launch_options(&steam_root()?)?;
    shell_open("steam://rungameid/1245620".as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn discovers_second_library_and_missing_parked_game() {
        let tmp = tempdir().unwrap();
        let steam = tmp.path().join("Steam");
        let library = tmp.path().join("second library");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::create_dir_all(library.join("steamapps/common/ELDEN RING")).unwrap();
        let windows_path = library.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                r#"
            "libraryfolders" {{ "1" {{ "path" "{windows_path}" "apps" {{ "1245620" "12" }} }} }}
        "#
            ),
        )
        .unwrap();
        fs::write(
            library.join("steamapps/appmanifest_1245620.acf"),
            r#"
            "AppState" { "appid" "1245620" "StateFlags" "4" "buildid" "123"
                         "installdir" "ELDEN RING" "InstalledDepots" { "7" { "manifest" "11" } } }
        "#,
        )
        .unwrap();
        let found = installations(&steam).unwrap().pop().unwrap();
        assert_eq!(found.game, library.join("steamapps/common/ELDEN RING/Game"));
        assert!(!found.game.exists());
        assert_eq!(
            current_build(found.manifest.get("AppState").unwrap()).unwrap(),
            "123"
        );
    }

    #[test]
    fn detects_pending_and_malformed_manifests() {
        let state = Vdf {
            input: br#""buildid" "123" "StateFlags" "4" "TargetBuildID" "777""#,
            pos: 0,
        }
        .entries(0)
        .unwrap();
        assert!(current_build(&state).is_err());
        assert!(Vdf {
            input: br#""libraryfolders" {"0" "C:\\""#,
            pos: 0
        }
        .entries(0)
        .is_err());
    }
}
