use super::{check_manifest, copy_files, metadata, walk, SourceFile, PROFILE, PROFILE_FILE};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const INDEX: &str = "native-packages.json";
const DIR: &str = "native";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: u32,
    // Game-independent, forward-slash relative path under native/pkg-<id>/.
    entry: String,
}

impl Entry {
    fn folder(&self) -> String {
        format!("pkg-{}", self.id)
    }
    fn path(&self, root: &Path) -> PathBuf {
        root.join(DIR).join(self.folder())
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Index {
    entries: Vec<Entry>,
}

fn portable(path: &Path) -> Result<String, String> {
    let mut names = Vec::new();
    for c in path.components() {
        let Component::Normal(name) = c else {
            return Err("패키지 내부 상대 경로를 확인하세요.".into());
        };
        let text = name.to_str().ok_or("경로 이름을 읽을 수 없습니다.")?;
        if text.contains([':', '\\', '"'])
            || text.chars().any(char::is_control)
            || text.ends_with([' ', '.'])
        {
            return Err(format!("지원하지 않는 경로 이름입니다. {}", path.display()));
        }
        names.push(text);
    }
    if names.is_empty() {
        return Err("비어 있는 패키지 경로입니다.".into());
    }
    Ok(names.join("/"))
}

fn is_dll(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
}

fn native_file(path: &Path) -> Result<(), String> {
    let name = portable(path)?.to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let first = name.split('/').next().unwrap_or("");
    if matches!(
        first,
        "easyanticheat" | "game" | "modengine2" | ".eldenring-mod-assist"
    ) || matches!(
        extension.as_str(),
        "exe"
            | "com"
            | "msi"
            | "sys"
            | "bat"
            | "cmd"
            | "ps1"
            | "vbs"
            | "reg"
            | "lnk"
            | "me3"
            | "bdt"
            | "bhd"
            | "zip"
            | "rar"
            | "7z"
    ) || matches!(
        name.as_str(),
        "config_eldenring.toml" | "start_protected_game.exe" | "steam_api64.dll"
    ) {
        return Err(format!("{} · 실행기·게임 설치본이 포함되어 있습니다. 제작자가 me3용으로 안내한 DLL과 설정·의존 파일이 있는 전용 폴더를 선택하세요.", path.display()));
    }
    Ok(())
}

fn index(root: &Path) -> Result<Index, String> {
    let file = root.join(INDEX);
    match fs::symlink_metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => (),
    }
    let info = metadata(&file)?;
    if !info.is_file() || info.len() > 2 * 1024 * 1024 {
        return Err("프로필의 DLL 등록 목록을 확인하세요.".into());
    }
    let registry: Index = serde_json::from_slice(&fs::read(&file).map_err(|e| e.to_string())?)
        .map_err(|e| format!("DLL 등록 목록을 읽을 수 없습니다. {e}"))?;
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for item in &registry.entries {
        let path = Path::new(&item.entry);
        native_file(path)?;
        if portable(path)? != item.entry
            || !is_dll(path)
            || item.id == 0
            || !ids.insert(item.id)
            || !names.insert(path.file_name().unwrap().to_string_lossy().to_lowercase())
        {
            return Err("중복되거나 잘못된 DLL 등록 경로입니다.".into());
        }
    }
    Ok(registry)
}

fn config(registry: &Index) -> Result<String, String> {
    let mut profile = PROFILE.to_owned();
    for item in &registry.entries {
        let relative = format!("{DIR}/{}/{}", item.folder(), item.entry);
        let value = serde_json::to_string(&relative).map_err(|e| e.to_string())?;
        profile.push_str(&format!("\n[[natives]]\npath = {value}\n"));
    }
    Ok(profile)
}

pub(super) fn expected_config(root: &Path) -> Result<String, String> {
    config(&index(root)?)
}

fn inventory(base: &Path, entry: &str) -> Result<(Vec<SourceFile>, u64), String> {
    let mut files = Vec::new();
    let mut bytes = 0u64;
    let mut found = false;
    for (relative, before) in walk(base)? {
        native_file(&relative)?;
        found |= portable(&relative)? == entry;
        bytes = bytes
            .checked_add(before.len())
            .ok_or("DLL 패키지 크기를 계산할 수 없습니다.")?;
        files.push(SourceFile {
            source: base.join(&relative),
            relative,
            before,
            reviewed_hash: None,
        });
    }
    if !found {
        return Err(format!("선택한 DLL이 패키지 안에 없습니다. {entry}"));
    }
    Ok((files, bytes))
}

fn verify_folders(root: &Path, index: &Index) -> Result<(), String> {
    let dir = root.join(DIR);
    match fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && index.entries.is_empty() => {
            return Ok(())
        }
        Err(e) => return Err(format!("{} · {e}", dir.display())),
        Ok(_) => (),
    }
    if !metadata(&dir)?.is_dir() {
        return Err("DLL 패키지 폴더를 확인하세요.".into());
    }
    let expected: HashSet<String> = index.entries.iter().map(Entry::folder).collect();
    let mut found = HashSet::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !metadata(&entry.path())?.is_dir() {
            return Err("등록되지 않은 DLL 패키지 경로가 있습니다.".into());
        }
        found.insert(entry.file_name().to_string_lossy().into_owned());
    }
    if found != expected {
        return Err("DLL 패키지 목록과 실제 폴더가 다릅니다.".into());
    }
    Ok(())
}

pub(crate) fn native_bytes(root: &Path) -> Result<u64, String> {
    let registry = index(root)?;
    verify_folders(root, &registry)?;
    let mut total = 0u64;
    for item in registry.entries {
        let (_, bytes) = inventory(&item.path(root), &item.entry)?;
        total = total
            .checked_add(bytes)
            .ok_or("DLL 패키지 전체 크기를 계산할 수 없습니다.")?;
    }
    Ok(total)
}

pub(super) fn validate_contents(root: &Path) -> Result<(), String> {
    native_bytes(root).map(|_| ())
}

fn stage_native_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(DIR);
    match fs::create_dir(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && metadata(&dir)?.is_dir() => (),
        result => result.map_err(|e| format!("{} · {e}", dir.display()))?,
    }
    Ok(dir)
}

fn check_stage_config(stage: &Path, previous: &Index) -> Result<(), String> {
    let profile = stage.join(PROFILE_FILE);
    match fs::symlink_metadata(&profile) {
        Ok(_) => check_manifest(&profile, &config(previous)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn store_index(stage: &Path, registry: &Index) -> Result<(), String> {
    // The caller owns an unpublished staging directory. The switcher commits
    // this directory only once both the index and generated config are ready.
    for (name, bytes) in [
        (
            INDEX,
            serde_json::to_vec(registry).map_err(|e| e.to_string())?,
        ),
        (PROFILE_FILE, config(registry)?.into_bytes()),
    ] {
        let mut temp = tempfile::NamedTempFile::new_in(stage).map_err(|e| e.to_string())?;
        temp.write_all(&bytes)
            .and_then(|_| temp.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        temp.persist(stage.join(name))
            .map_err(|e| e.error.to_string())?;
    }
    Ok(())
}

pub(crate) fn inherit_natives(source: &Path, stage: &Path) -> Result<(), String> {
    let inherited = index(source)?;
    verify_folders(source, &inherited)?;
    check_manifest(&source.join(PROFILE_FILE), &config(&inherited)?)?;
    if inherited.entries.is_empty() {
        return Ok(());
    }
    let initial = index(stage)?;
    verify_folders(stage, &initial)?;
    if !initial.entries.is_empty() {
        return Err("DLL 상속에 사용할 임시 폴더가 비어 있어야 합니다.".into());
    }
    check_stage_config(stage, &initial)?;
    stage_native_dir(stage)?;
    for item in &inherited.entries {
        let (files, _) = inventory(&item.path(source), &item.entry)?;
        let target = item.path(stage);
        fs::create_dir(&target).map_err(|e| e.to_string())?;
        copy_files(&files, &target)?;
    }
    store_index(stage, &inherited)
}

pub(crate) struct NativeOverlay {
    files: Vec<SourceFile>,
    entry: String,
    bytes: u64,
}

impl NativeOverlay {
    pub(crate) fn read(
        folder: &Path,
        entry: &Path,
        game: &Path,
        vault: &Path,
    ) -> Result<Self, String> {
        if !metadata(folder)?.is_dir() || !is_dll(entry) || !metadata(entry)?.is_file() {
            return Err("사용할 DLL 패키지 폴더와 그 안의 주 DLL 파일을 선택하세요.".into());
        }
        let absolute = |p: &Path| fs::canonicalize(p).map_err(|e| format!("{} · {e}", p.display()));
        let source = absolute(folder)?;
        for reserved in [game, vault] {
            let path = absolute(reserved)?;
            if source.starts_with(&path) || path.starts_with(&source) {
                return Err("게임과 보관함 밖에 압축을 푼 전용 DLL 모드 폴더를 선택하세요.".into());
            }
        }
        let chosen = absolute(entry)?;
        let relative = chosen
            .strip_prefix(&source)
            .map_err(|_| "선택한 주 DLL이 지정한 패키지 폴더 밖에 있습니다.")?;
        let entry = portable(relative)?;
        let (files, bytes) = inventory(&source, &entry)?;
        Ok(Self {
            files,
            entry,
            bytes,
        })
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn apply(&self, stage: &Path) -> Result<(), String> {
        let mut registry = index(stage)?;
        verify_folders(stage, &registry)?;
        check_stage_config(stage, &registry)?;
        stage_native_dir(stage)?;
        let id = registry
            .entries
            .iter()
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("DLL 패키지 번호를 사용할 수 없습니다.")?;
        let new = Entry {
            id,
            entry: self.entry.clone(),
        };
        let dest = new.path(stage);
        fs::create_dir(&dest).map_err(|e| e.to_string())?;
        copy_files(&self.files, &dest)?;
        let file_key = |entry: &str| {
            Path::new(entry)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_lowercase()
        };
        let chosen = file_key(&self.entry);
        let same_name = |e: &Entry| file_key(&e.entry) == chosen;
        if let Some(position) = registry.entries.iter().position(same_name) {
            let old = std::mem::replace(&mut registry.entries[position], new);
            // Remove only the validated, private inherited copy in staging.
            // The source profile still owns its previous plugin and settings.
            let prior = old.path(stage);
            inventory(&prior, &old.entry)?;
            fs::remove_dir_all(&prior).map_err(|e| format!("{} · {e}", prior.display()))?;
        } else {
            registry.entries.push(new);
        }
        store_index(stage, &registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modpack::{check_profile, write_profile};
    use tempfile::tempdir;

    fn put(root: &Path, name: &str, bytes: &[u8]) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn explicit_entry_and_its_private_dependencies_survive_clone_and_replacement() {
        let tmp = tempdir().unwrap();
        let base = tmp.path();
        let game = base.join("Game");
        let vault = base.join("vault");
        let stage = vault.join("new-profile");
        let clone = vault.join("clone");
        fs::create_dir(&game).unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::create_dir(&clone).unwrap();
        write_profile(&stage).unwrap();
        put(&game, "Data0.bdt", &[0; 65536]);
        let source = base.join("my-dll-mod");
        put(&source, "plugins/한글.dll", b"chosen");
        put(&source, "plugins/helper.dll", b"inert");
        put(&source, "plugins/mod.ini", b"config");
        put(&source, "localization/kor.json", b"{}");
        let plugin =
            NativeOverlay::read(&source, &source.join("plugins/한글.dll"), &game, &vault).unwrap();
        assert_eq!(plugin.bytes(), 19);
        plugin.apply(&stage).unwrap();
        write_profile(&stage).unwrap();
        check_profile(&stage).unwrap();
        let manifest = fs::read_to_string(stage.join(PROFILE_FILE)).unwrap();
        assert!(manifest.contains("native/pkg-1/plugins/한글.dll"));
        assert_eq!(manifest.matches("[[natives]]").count(), 1);
        assert!(!manifest.contains("helper.dll"));
        assert_eq!(native_bytes(&stage).unwrap(), 19);
        inherit_natives(&stage, &clone).unwrap();
        write_profile(&clone).unwrap();
        assert_eq!(
            fs::read(clone.join("native/pkg-1/plugins/helper.dll")).unwrap(),
            b"inert"
        );
        let updated = base.join("updated");
        put(&updated, "한글.dll", b"v2");
        let update =
            NativeOverlay::read(&updated, &updated.join("한글.dll"), &game, &vault).unwrap();
        update.apply(&clone).unwrap();
        check_profile(&clone).unwrap();
        assert_eq!(native_bytes(&clone).unwrap(), 2);
        assert_eq!(
            fs::read(clone.join("native/pkg-2/한글.dll")).unwrap(),
            b"v2"
        );
        assert!(!clone.join("native/pkg-1").exists());
        assert_eq!(
            fs::read(stage.join("native/pkg-1/plugins/한글.dll")).unwrap(),
            b"chosen"
        );
        assert!(game.join("Data0.bdt").is_file());
    }

    #[test]
    fn native_rejects_misdirected_entry_launchers_and_malformed_registry() {
        let tmp = tempdir().unwrap();
        let base = tmp.path();
        let game = base.join("Game");
        let vault = base.join("vault");
        let source = base.join("dlc");
        let stage = vault.join("stage");
        fs::create_dir_all(&stage).unwrap();
        write_profile(&stage).unwrap();
        put(&game, "example.dll", b"x");
        put(&source, "mod.dll", b"chosen");
        assert!(NativeOverlay::read(&game, &game.join("example.dll"), &game, &vault).is_err());
        assert!(NativeOverlay::read(&source, &game.join("example.dll"), &game, &vault).is_err());
        put(&source, "install.cmd", b"");
        assert!(NativeOverlay::read(&source, &source.join("mod.dll"), &game, &vault).is_err());
        fs::write(
            stage.join(INDEX),
            br#"{"entries":[{"id":1,"entry":"../../Game/example.dll"}]}"#,
        )
        .unwrap();
        assert!(check_profile(&stage).is_err());
        assert!(native_bytes(&stage).is_err());
    }
}
