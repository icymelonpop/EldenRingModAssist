mod native;
mod preview;
pub(crate) use native::{inherit_natives, native_bytes, NativeOverlay};
pub(crate) use preview::ModPreview;

use crate::progress;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io::{Read, Write};
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

pub(crate) const PACKAGE_DIR: &str = "mod";
pub(crate) const PROFILE_FILE: &str = "profile.me3";
const PROFILE: &str = include_str!("modpack/profile.me3");

// Elden Ring loose assets use game-relative virtual paths. A package replaces the
// entry with that path, so an existing regulation.bin is replaced as a whole.
const ASSET_DIRS: &[&str] = &[
    "action",
    "asset",
    "chr",
    "cutscene",
    "event",
    "facegen",
    "font",
    "gamedata",
    "map",
    "material",
    "menu",
    "movie",
    "movie_dlc",
    "msg",
    "mtd",
    "obj",
    "param",
    "parts",
    "script",
    "sd",
    "sfx",
    "shader",
    "sound",
];

pub(crate) fn metadata(path: &Path) -> Result<Metadata, String> {
    let info = fs::symlink_metadata(path).map_err(|e| format!("{} · {e}", path.display()))?;
    // Windows treats junctions and other name-surrogate reparse points as
    // symlinks. Ordinary OneDrive placeholders can still be regular files.
    if info.file_type().is_symlink() || !(info.is_file() || info.is_dir()) {
        return Err(format!(
            "연결 경로 또는 특수 파일을 확인하세요. {}",
            path.display()
        ));
    }
    Ok(info)
}

pub(crate) fn walk(root: &Path) -> Result<Vec<(PathBuf, Metadata)>, String> {
    if !metadata(root)?.is_dir() {
        return Err(format!("폴더를 선택하세요. {}", root.display()));
    }
    let mut files = Vec::new();
    let mut directories = vec![root.to_owned()];
    while let Some(dir) = directories.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| format!("{} · {e}", dir.display()))?;
        for item in entries {
            let path = item.map_err(|e| e.to_string())?.path();
            let info = metadata(&path)?;
            if info.is_dir() {
                directories.push(path);
            } else {
                files.push((
                    path.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_owned(),
                    info,
                ));
            }
        }
    }
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

fn asset_directory(name: &std::ffi::OsStr) -> bool {
    ASSET_DIRS
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
}

pub(crate) fn is_loose_asset(relative: &Path) -> bool {
    if relative.components().any(|c| {
        !matches!(c, Component::Normal(name) if name.to_str().is_some_and(|s|
            !s.contains(':') && !s.ends_with(['.', ' ']) && !s.starts_with('.')))
    }) {
        return false;
    }
    if relative.as_os_str().eq_ignore_ascii_case("regulation.bin") {
        return true;
    }
    let mut components = relative.iter();
    let Some(first) = components.next() else {
        return false;
    };
    if !asset_directory(first) || components.next().is_none() {
        return false;
    }
    let extension = relative
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    !matches!(
        extension.as_str(),
        "exe"
            | "dll"
            | "bat"
            | "cmd"
            | "com"
            | "sys"
            | "msi"
            | "ps1"
            | "vbs"
            | "lnk"
            | "reg"
            | "url"
            | "me3"
            | "toml"
            | "zip"
            | "7z"
            | "rar"
    )
}

pub(crate) fn validate_asset(relative: &Path) -> Result<(), String> {
    if is_loose_asset(relative) {
        Ok(())
    } else {
        Err(format!(
            "파일 모드 형식으로 가져올 수 없습니다. {} · regulation.bin이나 parts·chr 등의 게임 하위 폴더가 들어 있는 위치를 선택하세요. DLL 모드는 DLL 프로필 가져오기를 사용하고, 전용 실행기가 필요한 모드는 제작자 설치 안내를 확인하세요.",
            relative.display()))
    }
}

fn package_directory(root: &Path) -> Result<PathBuf, String> {
    let package = root.join(PACKAGE_DIR);
    if !metadata(root)?.is_dir() || !metadata(&package)?.is_dir() {
        return Err("프로필의 mod 폴더를 찾을 수 없습니다.".into());
    }
    Ok(package)
}

fn check_manifest(path: &Path, expected: &str) -> Result<(), String> {
    let info = metadata(path)?;
    if !info.is_file()
        || info.len() > (2 * expected.len() + 32) as u64
        || fs::read_to_string(path)
            .map_err(|e| e.to_string())?
            .replace("\r\n", "\n")
            != expected.replace("\r\n", "\n")
    {
        return Err(
            "관리 중인 me3 설정 파일의 내용이 달라졌습니다. 모드 폴더로 새 프로필을 만드세요."
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn write_profile(root: &Path) -> Result<(), String> {
    if !metadata(root)?.is_dir() {
        return Err("프로필을 저장할 폴더를 확인하세요.".into());
    }
    let package = root.join(PACKAGE_DIR);
    match fs::create_dir(&package) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            package_directory(root)?;
        }
        result => result.map_err(|e| format!("{} · {e}", package.display()))?,
    }
    let manifest = root.join(PROFILE_FILE);
    let config = native::expected_config(root)?;
    match fs::symlink_metadata(&manifest) {
        Ok(_) => return check_manifest(&manifest, &config),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format!("{} · {e}", manifest.display())),
    }
    let mut temp = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    temp.write_all(config.as_bytes())
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    match temp.persist_noclobber(&manifest) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            check_manifest(&manifest, &config)
        }
        Err(e) => Err(format!("{} · {}", manifest.display(), e.error)),
    }
}

pub(crate) fn check_profile(root: &Path) -> Result<(), String> {
    check_manifest(&root.join(PROFILE_FILE), &native::expected_config(root)?)?;
    native::validate_contents(root)?;
    for (relative, _) in walk(&package_directory(root)?)? {
        validate_asset(&relative)?;
    }
    Ok(())
}

fn is_documentation(relative: &Path, direct_asset_dir: bool) -> bool {
    let name = relative
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = relative
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let first = relative
        .iter()
        .next()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if first.eq_ignore_ascii_case("__MACOSX")
        || matches!(name.as_str(), ".ds_store" | "thumbs.db" | "desktop.ini")
    {
        return true;
    }
    if direct_asset_dir
        || relative
            .parent()
            .is_some_and(|p| p.iter().any(asset_directory))
    {
        return false;
    }
    let text = matches!(ext.as_str(), "" | "md" | "txt" | "pdf" | "html" | "rtf");
    let picture = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp");
    (text
        && ([
            "readme",
            "read me",
            "license",
            "licence",
            "changelog",
            "credits",
            "설치",
            "설명",
        ]
        .iter()
        .any(|s| name.starts_with(s))
            || matches!(ext.as_str(), "txt" | "md" | "pdf" | "rtf")))
        || picture
}

fn unwrapped_root(root: &Path, files: &[(PathBuf, Metadata)]) -> PathBuf {
    let mut prefix = PathBuf::new();
    // A single enclosing directory is unambiguous (including Game/, mod/, or
    // a release-name wrapper). Never guess which of several variants to use.
    loop {
        let mut names = files
            .iter()
            .filter_map(|(p, _)| p.strip_prefix(&prefix).ok());
        let Some(first) = names.next() else { break };
        let mut components = first.iter();
        let Some(dir) = components.next() else { break };
        if components.next().is_none()
            || asset_directory(dir)
            || names.any(|path| path.iter().next() != Some(dir) || path.iter().count() < 2)
        {
            break;
        }
        prefix.push(dir);
    }
    root.join(prefix)
}

struct SourceFile {
    source: PathBuf,
    relative: PathBuf,
    before: Metadata,
    reviewed_hash: Option<[u8; 32]>,
}

pub struct Overlay {
    files: Vec<SourceFile>,
    bytes: u64,
}

impl Overlay {
    pub fn read(source: &Path, game: &Path, vault: &Path) -> Result<Self, String> {
        metadata(source)?;
        let canonical =
            |p: &Path| fs::canonicalize(p).map_err(|e| format!("{} · {e}", p.display()));
        let root = canonical(source)?;
        for location in [game, vault] {
            let location = canonical(location)?;
            if location.starts_with(&root) || root.starts_with(&location) {
                return Err("게임과 프로필 보관함 밖에서 압축을 푼 모드 폴더를 선택하세요.".into());
            }
        }
        let direct_asset_dir = root.file_name().filter(|n| asset_directory(n));
        let files: Vec<_> = walk(&root)?
            .into_iter()
            .filter(|(p, _)| !is_documentation(p, direct_asset_dir.is_some()))
            .collect();
        if files.is_empty() {
            return Err("선택한 폴더에서 모드 파일을 찾을 수 없습니다.".into());
        }
        let package_root = if direct_asset_dir.is_some() {
            root.clone()
        } else {
            unwrapped_root(&root, &files)
        };
        let mut inputs = Vec::with_capacity(files.len());
        for (relative, before) in files {
            let source = root.join(relative);
            let mut relative = source
                .strip_prefix(&package_root)
                .map_err(|e| e.to_string())?
                .to_owned();
            if let Some(dir) = direct_asset_dir {
                relative = Path::new(dir).join(relative);
            }
            inputs.push(SourceFile {
                source,
                relative,
                before,
                reviewed_hash: None,
            });
        }
        Self::from_inputs(inputs)
    }

    /// Import only the added or changed game-relative files identified by the
    /// Steam or local baseline comparison, including an empty delta.
    pub fn selected(root: &Path, paths: Vec<PathBuf>) -> Result<Self, String> {
        if !metadata(root)?.is_dir() {
            return Err("설치본 폴더를 확인하세요.".into());
        }
        let mut files = Vec::with_capacity(paths.len());
        for relative in paths {
            validate_asset(&relative)?;
            let source = root.join(&relative);
            let mut component_path = root.to_owned();
            for component in relative.iter() {
                component_path.push(component);
                metadata(&component_path)?;
            }
            let before = metadata(&source)?;
            if !before.is_file() {
                return Err(format!("파일을 확인하세요. {}", source.display()));
            }
            files.push(SourceFile {
                source,
                relative,
                before,
                reviewed_hash: None,
            });
        }
        Self::from_inputs(files)
    }

    fn from_inputs(files: Vec<SourceFile>) -> Result<Self, String> {
        let mut seen = HashSet::new();
        let mut bytes = 0u64;
        for file in &files {
            validate_asset(&file.relative)?;
            if !seen.insert(file.relative.to_string_lossy().to_lowercase()) {
                return Err(format!(
                    "같은 게임 경로가 두 번 포함되어 있습니다. {}",
                    file.relative.display()
                ));
            }
            bytes = bytes
                .checked_add(file.before.len())
                .ok_or("모드 폴더 크기를 계산할 수 없습니다.")?;
        }
        Ok(Self { files, bytes })
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn apply(&self, staged: &Path) -> Result<(), String> {
        copy_files(&self.files, staged)
    }
}

fn copy_files(files: &[SourceFile], staged: &Path) -> Result<(), String> {
    if !metadata(staged)?.is_dir() {
        return Err("모드 파일을 저장할 폴더를 확인하세요.".into());
    }
    let mut buffer = vec![0; 1024 * 1024];
    for file in files {
        let source = &file.source;
        let destination = staged.join(&file.relative);
        let mut write = || -> Result<(), String> {
            if !metadata(source)?.is_file() {
                return Err("원본 파일이 변경되었습니다.".into());
            }
            let mut reader = File::open(source).map_err(|e| e.to_string())?;
            let parent = destination.parent().ok_or("대상 폴더를 확인하세요.")?;
            let mut directory = staged.to_owned();
            for subdir in file.relative.parent().unwrap_or(Path::new("")).iter() {
                directory.push(subdir);
                match fs::create_dir(&directory) {
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        if !metadata(&directory)?.is_dir() {
                            return Err(format!(
                                "기존 파일과 모드의 하위 폴더 경로가 겹칩니다. {}",
                                directory.display()
                            ));
                        }
                    }
                    result => result.map_err(|e| e.to_string())?,
                }
            }
            let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            let mut written = 0u64;
            let mut digest = file.reviewed_hash.map(|_| Sha256::new());
            loop {
                let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                if let Some(hash) = &mut digest {
                    hash.update(&buffer[..n]);
                }
                written += n as u64;
                progress::advance(n as u64);
            }
            if written != file.before.len()
                || metadata(source)?.last_write_time() != file.before.last_write_time()
            {
                return Err("복사 중에 파일 내용이 변경되었습니다. 다시 시도하세요.".into());
            }
            if digest.map(|h| <[u8; 32]>::from(h.finalize())) != file.reviewed_hash {
                return Err("미리보기 이후 파일 내용이 바뀌었습니다. 다시 비교하세요.".into());
            }
            output.as_file().sync_all().map_err(|e| e.to_string())?;
            // Only adjust the private copy; a staged inherited asset might
            // still carry the original package's read-only attribute.
            match fs::symlink_metadata(&destination) {
                Ok(info) if info.is_file() && !info.file_type().is_symlink() => {
                    let mut permissions = info.permissions();
                    if permissions.readonly() {
                        // The Windows read-only attribute belongs to this staged file.
                        #[cfg_attr(windows, allow(clippy::permissions_set_readonly_false))]
                        permissions.set_readonly(false);
                        fs::set_permissions(&destination, permissions)
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(_) => return Err("대상 경로가 일반 파일이 아닙니다.".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.to_string()),
            }
            output
                .persist(&destination)
                .map_err(|e| e.error.to_string())?;
            Ok(())
        };
        write().map_err(|e| format!("{} · {e}", file.relative.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(base: &Path, relative: &str, bytes: &[u8]) {
        let path = base.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn common_wrappers_and_asset_folders_retain_game_relative_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let game = root.join("Game");
        let vault = root.join("vault");
        let profile = root.join("profile");
        for p in [&game, &vault, &profile] {
            fs::create_dir(p).unwrap();
        }
        let archive = root.join("archive");
        put(
            &archive,
            "Fancy Armor v2/Game/mod/parts/body.partsbnd.dcx",
            b"armor",
        );
        put(
            &archive,
            "Fancy Armor v2/Game/mod/menu/texture/preview.png",
            b"texture",
        );
        put(&archive, "Fancy Armor v2/Game/mod/regulation.bin", b"reg");
        put(&archive, "Fancy Armor v2/설명.txt", b"note");
        put(&archive, "preview.png", b"documentation");
        put(&game, "Data0.bdt", &[0u8; 8192]);
        write_profile(&profile).unwrap();
        let overlay = Overlay::read(&archive, &game, &vault).unwrap();
        assert_eq!(overlay.bytes(), 15);
        let mod_dir = profile.join(PACKAGE_DIR);
        overlay.apply(&mod_dir).unwrap();
        let actual = walk(&mod_dir).unwrap();
        assert_eq!(actual.len(), 3);
        assert!(mod_dir.join("parts/body.partsbnd.dcx").is_file());
        assert!(mod_dir.join("menu/texture/preview.png").is_file());
        assert!(!mod_dir.join("Data0.bdt").exists());
        // A directory picker may point at the actual chr directory.
        let other = root.join("another/chr");
        put(&other, "c0000.chrbnd.dcx", b"actor");
        Overlay::read(&other, &game, &vault)
            .unwrap()
            .apply(&mod_dir)
            .unwrap();
        assert!(mod_dir.join("chr/c0000.chrbnd.dcx").is_file());
        check_profile(&profile).unwrap();
    }

    #[test]
    fn selected_delta_does_not_duplicate_baseline_and_latest_override_wins() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let full = root.join("old-install");
        let profile = root.join("profile");
        fs::create_dir(&profile).unwrap();
        put(&full, "Data1.bdt", &[0; 131072]);
        put(&full, "regulation.bin", b"new");
        put(&profile, "mod/regulation.bin", b"earlier");
        put(&profile, "mod/msg/english/item.msgbnd.dcx", b"text");
        write_profile(&profile).unwrap();
        let mod_dir = profile.join(PACKAGE_DIR);
        let previous = mod_dir.join("regulation.bin");
        let mut perms = fs::metadata(&previous).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&previous, perms).unwrap();
        let delta = Overlay::selected(&full, vec!["regulation.bin".into()]).unwrap();
        assert_eq!(delta.bytes(), 3);
        delta.apply(&mod_dir).unwrap();
        assert_eq!(fs::read(&previous).unwrap(), b"new");
        assert_eq!(walk(&mod_dir).unwrap().len(), 2);
        assert!(!mod_dir.join("Data1.bdt").exists());
        assert!(Overlay::selected(&full, vec!["../regulation.bin".into()]).is_err());
        assert!(Overlay::selected(&full, Vec::new()).unwrap().bytes() == 0);
    }

    #[test]
    fn mod_review_refuses_file_directory_conflicts_in_both_directions() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("base");
        let source = temp.path().join("source");
        put(&base, "parts/armor", b"old");
        put(&source, "parts/armor/a.dcx", b"new");
        let mut current = Overlay::selected(&base, vec!["parts/armor".into()]).unwrap();
        let mut incoming = Overlay::selected(&source, vec!["parts/armor/a.dcx".into()]).unwrap();
        assert!(ModPreview::compare(
            &mut incoming,
            Some(&mut current),
            "profile-1",
            "A".into(),
            None
        )
        .is_err());
        assert!(ModPreview::compare(
            &mut current,
            Some(&mut incoming),
            "profile-1",
            "A".into(),
            None
        )
        .is_err());
    }

    #[test]
    fn mod_review_verifies_bytes_at_copy_time_even_if_timestamp_matches() {
        use std::fs::FileTimes;
        let temp = tempfile::tempdir().unwrap();
        let incoming = temp.path().join("source");
        let stage = temp.path().join("stage");
        fs::create_dir(&stage).unwrap();
        put(&incoming, "regulation.bin", b"older");
        let mut overlay = Overlay::selected(&incoming, vec!["regulation.bin".into()]).unwrap();
        let review =
            ModPreview::compare(&mut overlay, None, "vanilla", "바닐라".into(), None).unwrap();
        review
            .select(&review.token, &[], &mut overlay, None)
            .unwrap();
        let changed = incoming.join("regulation.bin");
        let previous = fs::metadata(&changed).unwrap().modified().unwrap();
        fs::write(&changed, b"newer").unwrap();
        File::options()
            .write(true)
            .open(&changed)
            .unwrap()
            .set_times(FileTimes::new().set_modified(previous))
            .unwrap();
        assert!(overlay.apply(&stage).is_err());
        assert!(!stage.join("regulation.bin").exists());
    }

    #[test]
    fn unsupported_bundled_launchers_and_mod_variants_are_not_silently_dropped() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let game = root.join("Game");
        let vault = root.join("vault");
        for p in [&game, &vault] {
            fs::create_dir(p).unwrap();
        }
        let mixed = root.join("mixed");
        put(&mixed, "mod/regulation.bin", b"data");
        put(&mixed, "natives/plugin.dll", b"dependency");
        let error = Overlay::read(&mixed, &game, &vault).err().unwrap();
        assert!(error.contains("DLL"));
        let choices = root.join("choices");
        put(&choices, "optionA/regulation.bin", b"a");
        put(&choices, "optionB/regulation.bin", b"b");
        assert!(Overlay::read(&choices, &game, &vault).is_err());
        assert!(!is_loose_asset(Path::new("parts/nested/tool.exe")));
        assert!(Overlay::read(&vault, &game, &vault).is_err());
    }

    #[test]
    fn portable_manifest_is_idempotent_and_substituted_settings_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        put(root, "adoption.json", b"state");
        write_profile(root).unwrap();
        write_profile(root).unwrap();
        check_profile(root).unwrap();
        let file = root.join(PROFILE_FILE);
        let manifest = fs::read_to_string(&file).unwrap();
        assert!(manifest.contains("game = \"eldenring\""));
        assert!(manifest.contains("path = \"mod\""));
        fs::write(
            &file,
            manifest + "\n[[natives]]\npath = \"unexpected.dll\"\n",
        )
        .unwrap();
        assert!(check_profile(root).is_err());
        assert!(write_profile(root).is_err());
        assert!(fs::read_to_string(file).unwrap().contains("unexpected.dll"));
    }
}
