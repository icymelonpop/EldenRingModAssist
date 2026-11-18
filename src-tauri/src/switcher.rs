//! Asset packages share the vanilla installation. Only legacy full installations use directory swaps.
//! The journal is committed before the first rename. Recovery never replaces
//! an existing directory and refuses ambiguous layouts (e.g. Steam recreated Game).
mod files;
mod packages;
pub use packages::{
    convert_profile, create_profile, import_profile, preview_mods, profile_from_native,
    profile_from_review,
};

use files::{atomic_json, check_game, directory, io, meta, move_dir, scan, Tree};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};

type R<T> = Result<T, String>;
const VANILLA: &str = "vanilla";
const VAULT: &str = ".eldenring-mod-assist";
const OWNER: &[u8] = b"Elden Ring Mod Assist storage v1\n";

#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Legacy,
    Package,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub build: Option<String>,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default, rename = "archivedFiles")]
    pub archived_files: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub phase: &'static str,
    pub active_id: Option<String>,
    pub profiles: Vec<Profile>,
    pub baseline_build: Option<String>,
    pub steam_build: Option<String>,
    pub steam_issue: Option<String>,
    pub stale: bool,
    pub game_path: String,
    pub vault_path: String,
    pub last_released_path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub matches: bool,
    pub checked: usize,
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    pub stale: bool,
    pub baseline_build: Option<String>,
    pub steam_build: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Baseline {
    build: Option<String>,
    files: Tree,
}

#[derive(Clone, Serialize, Deserialize)]
struct Released {
    id: String,
    serial: u64,
}

impl Released {
    fn path(&self, game: &Path) -> PathBuf {
        game.with_file_name(format!("EldenRing-Profile-{}-{}", self.id, self.serial))
    }
}

#[derive(Clone, Serialize, Deserialize)]
enum Job {
    Park {
        profile: Profile,
    }, // v1 recovery
    Capture {
        profile: Profile,
        plan: crate::adoption::Plan,
    },
    Uncapture(Released),
    Unpark {
        id: String,
    },
    Copying {
        staging: String,
    },
    Swap {
        from: String,
        to: String,
    },
    Publish {
        profile: Profile,
        staging: String,
    },
    Release(Released),
}

#[derive(Serialize, Deserialize)]
struct State {
    version: u32,
    active: Option<String>,
    profiles: Vec<Profile>,
    baseline: Option<Baseline>,
    job: Option<Job>,
    #[serde(default)]
    highest_id: u32,
    #[serde(default)]
    last_release: Option<Released>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 2,
            active: None,
            profiles: vec![],
            baseline: None,
            job: None,
            highest_id: 0,
            last_release: None,
        }
    }
}

fn idsafe(id: &str) -> bool {
    id.strip_prefix("profile-").is_some_and(|s| {
        s.bytes().all(|c| c.is_ascii_digit()) && s.parse::<u32>().is_ok_and(|n| n > 0)
    })
}

fn staging_safe(name: &str) -> bool {
    name.strip_prefix("copy-")
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric()))
}

impl State {
    fn validate(&self) -> R<()> {
        let error = || "보관함 상태 파일이 손상되었거나 지원하지 않는 버전입니다.".to_string();
        let mut ids = std::collections::HashSet::new();
        if !matches!(self.version, 1 | 2)
            || self.last_release.as_ref().is_some_and(|r| !idsafe(&r.id))
            || self
                .profiles
                .iter()
                .any(|p| !idsafe(&p.id) || !ids.insert(p.id.as_str()))
        {
            return Err(error());
        }
        let known = |id: &str| (id == VANILLA && self.baseline.is_some()) || ids.contains(id);
        if self.active.as_deref().is_some_and(|id| !known(id))
            || self.active.is_some() != self.baseline.is_some()
        {
            return Err(error());
        }
        match &self.job {
            Some(Job::Release(released))
                if !ids.contains(released.id.as_str())
                    || self.active.is_none()
                    || self.active.as_ref() == Some(&released.id) =>
            {
                Err(error())
            }
            Some(Job::Copying { staging }) if !staging_safe(staging) => Err(error()),
            Some(Job::Unpark { id } | Job::Uncapture(Released { id, .. }))
                if self.baseline.is_some()
                    || self.profiles.len() != 1
                    || !ids.contains(id.as_str()) =>
            {
                Err(error())
            }
            Some(Job::Park { profile } | Job::Capture { profile, .. })
                if !idsafe(&profile.id)
                    || known(&profile.id)
                    || self.active.as_deref().is_some_and(|id| id != VANILLA) =>
            {
                Err(error())
            }
            Some(Job::Swap { from, to })
                if !known(from)
                    || !known(to)
                    || self.physical(from) == self.physical(to)
                    || self.active.as_ref() != Some(from) =>
            {
                Err(error())
            }
            Some(Job::Publish { profile, staging })
                if !idsafe(&profile.id) || known(&profile.id) || !staging_safe(staging) =>
            {
                Err(error())
            }
            _ => Ok(()),
        }
    }
    fn settled(&self) -> R<()> {
        if self.job.is_some() {
            Err("중단된 전환을 먼저 복구하세요.".into())
        } else {
            Ok(())
        }
    }
    fn physical<'a>(&self, id: &'a str) -> &'a str {
        if self
            .profiles
            .iter()
            .any(|p| p.id == id && p.kind == Kind::Package)
        {
            VANILLA
        } else {
            id
        }
    }
    fn validate_name(&self, name: &str, except: Option<&str>) -> R<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err("프로필 이름을 1~64자로 입력하세요.".into());
        }
        if name.eq_ignore_ascii_case(VANILLA)
            || name == "바닐라"
            || self.profiles.iter().any(|p| {
                except != Some(p.id.as_str()) && p.name.to_lowercase() == name.to_lowercase()
            })
        {
            return Err("이미 사용 중인 프로필 이름입니다.".into());
        }
        Ok(())
    }
    fn profile(&mut self, name: &str, build: Option<String>, vault: &Path) -> R<Profile> {
        let name = name.trim();
        self.validate_name(name, None)?;
        let start = self
            .highest_id
            .checked_add(1)
            .ok_or("프로필 식별자를 할당할 수 없습니다.")?;
        for n in start..=u32::MAX {
            let id = format!("profile-{n}");
            if !self.profiles.iter().any(|p| p.id == id) && meta(&vault.join(&id))?.is_none() {
                self.highest_id = n;
                return Ok(Profile {
                    id,
                    name: name.into(),
                    build,
                    kind: Kind::Legacy,
                    bytes: None,
                    archived_files: 0,
                });
            }
        }
        Err("프로필 식별자를 할당할 수 없습니다.".into())
    }
}

struct Layout {
    game: PathBuf,
    vault: PathBuf,
}

impl Layout {
    fn new(game: &Path) -> R<Self> {
        let parent = game
            .parent()
            .filter(|_| {
                game.is_absolute()
                    && game
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("Game"))
            })
            .ok_or("엘든 링 설치 폴더 안의 Game 폴더를 선택하세요.")?;
        if game
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err("게임 폴더의 전체 경로를 입력하세요.".into());
        }
        for ancestor in parent.ancestors() {
            if !directory(ancestor)? {
                return Err(format!("상위 폴더가 없습니다. {}", ancestor.display()));
            }
        }
        directory(game)?;
        Ok(Self {
            game: game.to_owned(),
            vault: parent.join(VAULT),
        })
    }
    fn owned(&self) -> R<bool> {
        if !directory(&self.vault)? {
            return Ok(false);
        }
        let marker = self.vault.join("owner.txt");
        if !meta(&marker)?.is_some_and(|m| m.is_file() && m.len() == OWNER.len() as u64)
            || fs::read(&marker).map_err(|e| io(&marker, e))? != OWNER
        {
            return Err(format!(
                "기존 폴더의 보관함 식별자를 확인할 수 없습니다. {}",
                self.vault.display()
            ));
        }
        Ok(true)
    }
    fn create(&self, steam_registered: bool) -> R<()> {
        if !self.owned()? {
            if !steam_registered {
                check_game(&self.game, false)?;
            } else if !directory(&self.game)? {
                return Err("Steam에서 Game 폴더를 복구한 뒤 등록하세요.".into());
            }
            match fs::create_dir(&self.vault) {
                Ok(()) => {
                    use std::io::Write;
                    let marker = self.vault.join("owner.txt");
                    let mut file = File::create_new(&marker).map_err(|e| io(&marker, e))?;
                    file.write_all(OWNER)
                        .and_then(|_| file.sync_all())
                        .map_err(|e| io(&marker, e))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    self.owned()?;
                }
                Err(e) => return Err(io(&self.vault, e)),
            }
        }
        Ok(())
    }
    fn lock(&self) -> R<File> {
        let path = self.vault.join("operation.lock");
        if meta(&path)?.is_some_and(|m| !m.is_file()) {
            return Err("보관함 잠금 파일을 확인하세요.".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        fs2::FileExt::try_lock_exclusive(&file)
            .map_err(|_| "다른 창에서 게임 파일을 처리 중입니다.".to_string())?;
        Ok(file)
    }
    fn state(&self) -> R<State> {
        let path = self.vault.join("state.json");
        if meta(&path)?.is_none() {
            // Only a freshly initialized vault may start with an empty inventory.
            for entry in fs::read_dir(&self.vault).map_err(|e| io(&self.vault, e))? {
                let item = entry.map_err(|e| io(&self.vault, e))?;
                if item.file_name() != "owner.txt" && item.file_name() != "operation.lock" {
                    return Err("보관함 상태 파일이 없습니다. 보관된 폴더를 확인하세요.".into());
                }
            }
            return Ok(State::default());
        }
        let reader = BufReader::new(File::open(&path).map_err(|e| io(&path, e))?);
        let mut disk: State = serde_json::from_reader(reader)
            .map_err(|e| format!("보관함 상태를 읽을 수 없습니다. {e}"))?;
        disk.validate()?;
        // Old binaries must refuse the new compact slots rather than mounting one as Game.
        disk.version = 2;
        for profile in &disk.profiles {
            disk.highest_id = disk
                .highest_id
                .max(profile.id["profile-".len()..].parse().unwrap());
        }
        Ok(disk)
    }
    fn save(&self, state: &State) -> R<()> {
        state.validate()?;
        atomic_json(&self.vault.join("state.json"), state)
    }
    fn view(&self, state: &State) -> View {
        let (build, issue) = match crate::platform::steam_install_state(&self.game) {
            Ok(b) => (b, None),
            Err(e) => (None, Some(e)),
        };
        let base = state.baseline.as_ref().and_then(|b| b.build.clone());
        let phase = if state.job.is_some() {
            "recovery"
        } else if state.baseline.is_some() {
            "ready"
        } else if !state.profiles.is_empty() {
            "awaitingVanilla"
        } else {
            "unmanaged"
        };
        let mut profiles = state.profiles.clone();
        for p in profiles.iter_mut().filter(|p| p.kind == Kind::Package) {
            let folder = self.vault.join(&p.id);
            // Users can edit an inactive package in Explorer. Show its actual data footprint.
            let measured = || -> R<(u64, usize)> {
                if !directory(&folder)? {
                    return Err("프로필 폴더가 없습니다.".into());
                }
                let assets = files::payload_bytes(&folder.join(crate::modpack::PACKAGE_DIR))?;
                let archive = folder.join("preserved");
                let (other, count) = if directory(&archive)? {
                    files::payload_stats(&archive)?
                } else {
                    (0, 0)
                };
                Ok((
                    assets
                        .saturating_add(other)
                        .saturating_add(crate::modpack::native_bytes(&folder)?),
                    count,
                ))
            };
            if let Ok((bytes, count)) = measured() {
                p.bytes = Some(bytes);
                p.archived_files = count;
            } else {
                p.bytes = None;
            }
        }
        View {
            phase,
            active_id: state.active.clone(),
            profiles,
            stale: state.baseline.is_some() && base != build,
            steam_issue: issue,
            baseline_build: base,
            steam_build: build,
            game_path: self.game.display().to_string(),
            vault_path: self.vault.display().to_string(),
            last_released_path: state
                .last_release
                .as_ref()
                .map(|r| r.path(&self.game).display().to_string()),
        }
    }
    fn destination(&self, id: &str) -> R<Released> {
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let mut released = Released {
            id: id.into(),
            serial,
        };
        while meta(&released.path(&self.game))?.is_some() {
            released.serial = released
                .serial
                .checked_add(1)
                .ok_or("사용할 수 있는 폴더 이름을 찾을 수 없습니다.")?;
        }
        Ok(released)
    }
    fn finish(&self, state: &mut State) -> R<()> {
        let Some(job) = state.job.clone() else {
            return Ok(());
        };
        let conflict = || {
            format!("중단된 작업의 폴더 구성이 예상과 다릅니다. 파일을 덮어쓰지 않도록 멈췄습니다. {} 및 {}를 확인하세요.", self.game.display(), self.vault.display())
        };
        match job {
            Job::Release(released) => {
                let parked = self.vault.join(&released.id);
                let dest = released.path(&self.game);
                match (directory(&parked)?, directory(&dest)?) {
                    (true, false) => move_dir(&parked, &dest)?,
                    (false, true) => (),
                    _ => return Err(conflict()),
                }
                state.profiles.retain(|p| p.id != released.id);
                state.last_release = Some(released);
            }
            Job::Copying { staging } => {
                files::discard_partial(&self.vault.join(&staging), &staging)?
            }
            Job::Uncapture(released) => {
                let source = self.vault.join(&released.id);
                let dest = released.path(&self.game);
                match (directory(&source)?, directory(&dest)?) {
                    (true, false) => {
                        crate::adoption::undo(&self.game, &source)?;
                        move_dir(&source, &dest)?;
                    }
                    (false, true) => (),
                    _ => return Err(conflict()),
                }
                state.profiles.clear();
                state.last_release = Some(released);
            }
            Job::Capture { profile, plan } => {
                let slot = self.vault.join(&profile.id);
                if let Err(reason) = crate::adoption::capture(&self.game, &slot, &plan) {
                    if crate::adoption::unmoved(&self.game, &slot, &plan)? {
                        state.job = None;
                        self.save(state)?;
                    }
                    return Err(reason);
                }
                state.profiles.push(profile);
                state.active = None;
                state.baseline = None;
            }
            Job::Unpark { id } => {
                let src = self.vault.join(&id);
                match (directory(&src)?, directory(&self.game)?) {
                    (true, false) => move_dir(&src, &self.game)?,
                    (false, true) => (),
                    _ => return Err(conflict()),
                }
                state.profiles.clear();
            }
            Job::Park { profile } => {
                let dest = self.vault.join(&profile.id);
                match (directory(&self.game)?, directory(&dest)?) {
                    (true, false) => move_dir(&self.game, &dest)?,
                    (false, true) => (),
                    _ => return Err(conflict()),
                }
                state.profiles.push(profile);
                state.active = None;
                state.baseline = None;
            }
            Job::Swap { from, to } => {
                let outgoing = self.vault.join(state.physical(&from));
                let incoming = self.vault.join(state.physical(&to));
                match (
                    directory(&self.game)?,
                    directory(&outgoing)?,
                    directory(&incoming)?,
                ) {
                    (true, false, true) => {
                        move_dir(&self.game, &outgoing)?;
                        move_dir(&incoming, &self.game)?;
                    }
                    (false, true, true) => move_dir(&incoming, &self.game)?,
                    (true, true, false) => (),
                    _ => return Err(conflict()),
                }
                state.active = Some(to);
            }
            Job::Publish { profile, staging } => {
                let src = self.vault.join(&staging);
                let dst = self.vault.join(&profile.id);
                match (directory(&src)?, directory(&dst)?) {
                    (true, false) => {
                        let marker = src.join(files::PARTIAL);
                        if meta(&marker)?.is_some() {
                            if fs::read(&marker).map_err(|e| io(&marker, e))? != staging.as_bytes()
                            {
                                return Err(conflict());
                            }
                            fs::remove_file(&marker).map_err(|e| io(&marker, e))?;
                        }
                        move_dir(&src, &dst)?;
                    }
                    (false, true) => (),
                    _ => return Err(conflict()),
                }
                state.profiles.push(profile);
            }
        }
        state.job = None;
        self.save(state)
    }
    fn transact(&self, state: &mut State, job: Job) -> R<()> {
        state.job = Some(job);
        self.save(state)?;
        self.finish(state)
    }
}

pub fn inspect(game: &Path) -> R<View> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        let view = layout.view(&State::default());
        if view.steam_build.is_none() {
            check_game(game, false)?;
        }
        return Ok(view);
    }
    let _lock = layout.lock()?;
    Ok(layout.view(&layout.state()?))
}

// All destructive steps also re-check Steam state immediately before committing.
fn mutate(
    game: &Path,
    may_initialize: bool,
    action: impl FnOnce(&Layout, &mut State, Option<String>) -> R<()>,
) -> R<View> {
    let layout = Layout::new(game)?;
    let build = crate::platform::steam_install_state(game)?;
    if may_initialize {
        layout.create(build.is_some())?;
    } else if !layout.owned()? {
        return Err("먼저 현재 설치를 등록하세요.".into());
    }
    let _lock = layout.lock()?;
    let mut state = layout.state()?;
    action(&layout, &mut state, build)?;
    Ok(layout.view(&state))
}

fn check_build(game: &Path, before: &Option<String>) -> R<()> {
    crate::platform::ensure_game_closed()?;
    if &crate::platform::steam_install_state(game)? != before {
        return Err(
            "처리 중 Steam 빌드가 변경되었습니다. Steam 작업이 끝난 뒤 다시 시도하세요.".into(),
        );
    }
    Ok(())
}

pub fn onboard_modded(game: &Path, name: &str) -> R<View> {
    // Without the Steam inventory there is no authoritative way to distinguish
    // an untouched archive from a modified one on first registration.
    crate::platform::steam_build(game)?;
    mutate(game, true, |layout, state, build| {
        state.settled()?;
        if state.active.as_deref() != Some(VANILLA)
            && (!state.profiles.is_empty() || state.baseline.is_some())
        {
            return Err("현재 단계를 마친 뒤 새 원본 복구를 시작하세요.".into());
        }
        let mut profile = state.profile(name, build.clone(), &layout.vault)?;
        let plan = crate::adoption::plan(game)?;
        profile.kind = Kind::Package;
        profile.bytes = Some(plan.bytes());
        check_build(game, &build)?;
        layout.transact(state, Job::Capture { profile, plan })
    })
}

pub fn baseline(game: &Path) -> R<View> {
    mutate(game, true, |layout, state, build| {
        state.settled()?;
        if state.active.as_deref().is_some_and(|id| id != VANILLA) {
            return Err("모드 프로필이 적용되어 있습니다. 바닐라로 전환한 뒤 등록하세요.".into());
        }
        if meta(&layout.vault.join(VANILLA))?.is_some() {
            return Err("보관함에 별도의 바닐라 폴더가 있습니다. 폴더 상태를 확인하세요.".into());
        }
        check_game(game, build.is_none())?;
        let official = if build.is_some() {
            Some(crate::depot::expected_files(game)?)
        } else {
            None
        };
        let files = scan(game, official.as_ref(), None)?;
        check_build(game, &build)?;
        state.baseline = Some(Baseline { build, files });
        state.active = Some(VANILLA.into());
        layout.save(state)
    })
}

pub fn undo_park(game: &Path, id: &str) -> R<View> {
    mutate(game, false, |layout, state, build| {
        state.settled()?;
        if state.baseline.is_some() || state.profiles.len() != 1 || state.profiles[0].id != id {
            return Err(
                "첫 설치를 보관했고 아직 새 바닐라 원본을 등록하지 않았을 때 되돌릴 수 있습니다."
                    .into(),
            );
        }
        if state.profiles[0].kind == Kind::Package {
            let source = layout.vault.join(id);
            crate::adoption::check_undo(game, &source)?;
            let released = layout.destination(id)?;
            check_build(game, &build)?;
            return layout.transact(state, Job::Uncapture(released));
        }
        if directory(game)? || !directory(&layout.vault.join(id))? {
            return Err(
                "Game 위치에 새 설치가 있거나 보관한 프로필을 찾을 수 없습니다. 폴더를 확인하세요."
                    .into(),
            );
        }
        check_build(game, &build)?;
        layout.transact(state, Job::Unpark { id: id.into() })
    })
}

pub fn rename_profile(game: &Path, id: &str, name: &str) -> R<View> {
    mutate(game, false, |layout, state, _| {
        state.settled()?;
        state.validate_name(name, Some(id))?;
        let profile = state
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("프로필을 찾을 수 없습니다.")?;
        profile.name = name.trim().into();
        layout.save(state)
    })
}

/// Move an inactive profile to a normal folder beside Game. The caller can move
/// that folder to another drive, import it again, or remove it in Explorer.
pub fn release_profile(game: &Path, id: &str) -> R<PathBuf> {
    let view = mutate(game, false, |layout, state, build| {
        state.settled()?;
        if !state.profiles.iter().any(|p| p.id == id) {
            if state.last_release.as_ref().is_some_and(|r| r.id == id) {
                return Ok(());
            }
            return Err("보관된 모드 프로필을 찾을 수 없습니다.".into());
        }
        if state.active.is_none() || state.active.as_deref() == Some(id) {
            return Err("바닐라를 등록하고 다른 프로필로 전환한 뒤 꺼내세요.".into());
        }
        if !directory(&layout.vault.join(id))? {
            return Err("보관함에서 프로필 폴더를 찾을 수 없습니다.".into());
        }
        let released = layout.destination(id)?;
        check_build(game, &build)?;
        layout.transact(state, Job::Release(released))
    })?;
    view.last_released_path
        .map(PathBuf::from)
        .ok_or_else(|| "내보낸 프로필의 위치를 확인하세요.".into())
}

/// An IPC caller may open only a profile recorded in this vault.
pub fn package_folder(game: &Path, id: &str) -> R<PathBuf> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        return Err("프로필을 먼저 등록하세요.".into());
    }
    let _lock = layout.lock()?;
    let state = layout.state()?;
    state.settled()?;
    let p = state
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or("프로필을 찾을 수 없습니다.")?;
    let root = if p.kind == Kind::Legacy && state.active.as_deref() == Some(id) {
        layout.game
    } else {
        layout.vault.join(&p.id)
    };
    if !directory(&root)? {
        return Err("프로필 폴더를 찾을 수 없습니다.".into());
    }
    Ok(root)
}

pub fn active_package(game: &Path) -> R<PathBuf> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        return Err("프로필을 먼저 등록하세요.".into());
    }
    let _lock = layout.lock()?;
    let state = layout.state()?;
    state.settled()?;
    let selected = state
        .profiles
        .iter()
        .find(|p| p.kind == Kind::Package && Some(&p.id) == state.active.as_ref())
        .ok_or("먼저 경량 모드 프로필로 전환하세요.")?;
    let root = layout.vault.join(&selected.id);
    if !directory(&root)? || !directory(&root.join(crate::modpack::PACKAGE_DIR))? {
        return Err("경량 프로필의 모드 폴더를 찾을 수 없습니다.".into());
    }
    check_build(game, &selected.build)?;
    Ok(root)
}

pub fn switch(game: &Path, id: &str) -> R<View> {
    mutate(game, false, |layout, state, build| {
        state.settled()?;
        let from = state
            .active
            .clone()
            .ok_or("먼저 바닐라 원본을 등록하세요.")?;
        if id == from {
            return Ok(());
        }
        if id != VANILLA {
            let profile = state
                .profiles
                .iter()
                .find(|p| p.id == id)
                .ok_or("프로필을 찾을 수 없습니다.")?;
            if profile.build != build
                || state.baseline.as_ref().and_then(|b| b.build.as_ref()) != build.as_ref()
            {
                return Err("게임 빌드가 변경되었습니다. 새 Steam 원본을 등록하고 현재 버전에 맞게 프로필을 다시 만드세요. 이전 프로필의 파일은 보관함에서 확인할 수 있습니다.".into());
            }
        }
        let physical_from = state.physical(&from);
        let physical_to = state.physical(id);
        if physical_to == VANILLA
            && id != VANILLA
            && !directory(&layout.vault.join(id).join(crate::modpack::PACKAGE_DIR))?
        {
            return Err("선택한 프로필의 모드 폴더를 찾을 수 없습니다.".into());
        }
        if !directory(game)? {
            return Err("게임 설치 폴더를 확인하세요.".into());
        }
        if physical_from == physical_to {
            // Both selections use the same untouched Game directory: no moves or file copies.
            check_build(game, &build)?;
            state.active = Some(id.to_owned());
            return layout.save(state);
        }
        if !directory(&layout.vault.join(physical_to))?
            || meta(&layout.vault.join(physical_from))?.is_some()
        {
            return Err("전환에 필요한 게임 폴더와 보관 폴더의 위치를 확인하세요.".into());
        }
        check_build(game, &build)?;
        layout.transact(
            state,
            Job::Swap {
                from,
                to: id.to_owned(),
            },
        )
    })
}

pub fn recover(game: &Path) -> R<View> {
    mutate(game, false, |layout, state, build| {
        check_build(game, &build)?;
        if let Some(Job::Swap { from, to }) = &state.job {
            if state
                .profiles
                .iter()
                .any(|p| &p.id == to && p.build != build)
            {
                let outgoing = layout.vault.join(state.physical(from));
                let incoming = layout.vault.join(state.physical(to));
                match (
                    directory(game)?,
                    directory(&outgoing)?,
                    directory(&incoming)?,
                ) {
                    (true, false, true) => (),
                    (false, true, true) => move_dir(&outgoing, game)?,
                    (true, true, false) => {
                        move_dir(game, &incoming)?;
                        move_dir(&outgoing, game)?;
                    }
                    _ => return Err("중단된 전환의 폴더 구성을 확인하세요.".into()),
                }
                state.job = None;
                return layout.save(state);
            }
        }
        layout.finish(state)
    })
}

fn report(layout: &Layout, state: &State) -> R<Report> {
    state.settled()?;
    let base = state.baseline.as_ref().ok_or("바닐라 기준이 없습니다.")?;
    let build = crate::platform::steam_install_state(&layout.game)?;
    let root = if state.active.as_deref().map(|id| state.physical(id)) == Some(VANILLA) {
        layout.game.clone()
    } else {
        layout.vault.join(VANILLA)
    };
    let actual = scan(&root, None, Some(&base.files))?;
    let mut added = vec![];
    let mut changed = vec![];
    for (name, entry) in &actual {
        match base.files.get(name) {
            None => added.push(name.clone()),
            Some(reference) if entry != reference => changed.push(name.clone()),
            _ => (),
        }
    }
    let missing: Vec<_> = base
        .files
        .keys()
        .filter(|k| !actual.contains_key(*k))
        .cloned()
        .collect();
    check_build(&layout.game, &build)?;
    let stale = build != base.build;
    Ok(Report {
        matches: !stale && added.is_empty() && changed.is_empty() && missing.is_empty(),
        checked: actual.len(),
        added,
        changed,
        missing,
        stale,
        baseline_build: base.build.clone(),
        steam_build: build,
    })
}

pub fn verify(game: &Path) -> R<Report> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        return Err("바닐라 기준이 없습니다.".into());
    }
    let _lock = layout.lock()?;
    report(&layout, &layout.state()?)
}

pub fn launch_verified(game: &Path, launch: impl FnOnce() -> R<()>) -> R<()> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        return Err("Steam 원본을 먼저 등록하세요.".into());
    }
    let _lock = layout.lock()?;
    let state = layout.state()?;
    state.settled()?;
    let registered_build = state.baseline.as_ref().and_then(|b| b.build.clone());
    if state.active.as_deref() != Some(VANILLA) || registered_build.is_none() {
        return Err("공식 Steam 원본으로 등록된 바닐라 설치에서 실행할 수 있습니다.".into());
    }
    check_build(game, &registered_build)?;
    // Re-read Steam's current depots (DLC or language can change without a new build).
    // Never use the locally recorded reference as the authority for a Steam launch.
    if !crate::depot::audit_install(game)?.matches {
        return Err("공식 원본과 차이가 있습니다. Steam 복구와 무결성 검사를 진행하세요.".into());
    }
    check_build(game, &registered_build)?;
    launch()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    fn game_files(game: &Path) {
        fs::create_dir_all(game.join("EasyAntiCheat")).unwrap();
        fs::write(game.join("eldenring.exe"), b"game").unwrap();
        fs::write(game.join("start_protected_game.exe"), b"launcher").unwrap();
        fs::write(game.join("regulation.bin"), b"original").unwrap();
    }
    fn fixture() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("Game");
        game_files(&game);
        (tmp, game)
    }

    #[test]
    fn mod_review_classifies_files_and_applies_individual_choices_to_new_profile() {
        let (tmp, game) = fixture();
        baseline(&game).unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        for root in [&first, &second] {
            fs::create_dir_all(root.join("parts")).unwrap();
            fs::write(root.join("parts/same.dcx"), b"shared").unwrap();
        }
        fs::write(first.join("regulation.bin"), b"ABCD").unwrap();
        fs::write(first.join("parts/armor.dcx"), b"old").unwrap();
        let preview = preview_mods(&game, &first).unwrap();
        assert_eq!(preview.added.len(), 3);
        assert!(preview.conflicts.is_empty());
        let initial = profile_from_review(&game, &first, "첫 모드", &preview.token, &[]).unwrap();
        let id = &initial.profiles[0].id;
        switch(&game, id).unwrap();
        fs::write(second.join("REGULATION.BIN"), b"EFGH").unwrap();
        fs::write(second.join("parts/armor.dcx"), b"new").unwrap();
        fs::write(second.join("parts/new.dcx"), b"brand new").unwrap();
        let preview = preview_mods(&game, &second).unwrap();
        assert_eq!(preview.base_name, "첫 모드");
        assert_eq!(preview.added, ["parts/new.dcx"]);
        assert_eq!(preview.identical, ["parts/same.dcx"]);
        assert_eq!(preview.conflicts.len(), 2);
        assert!(preview.conflicts.iter().any(|c| c.path == "REGULATION.BIN"));
        let before = inspect(&game).unwrap();
        assert_eq!(before.profiles.len(), 1);
        let rejected = profile_from_review(
            &game,
            &second,
            "선택 오류",
            &preview.token,
            &["parts/new.dcx".into()],
        );
        assert!(rejected.is_err());
        let updated = profile_from_review(
            &game,
            &second,
            "혼합",
            &preview.token,
            &["REGULATION.BIN".into()],
        )
        .unwrap();
        assert_eq!(updated.active_id.as_deref(), Some(id.as_str()));
        let vault = Path::new(&updated.vault_path);
        let base = vault.join(id).join("mod");
        let mixed = vault.join(&updated.profiles[1].id).join("mod");
        assert_eq!(fs::read(mixed.join("regulation.bin")).unwrap(), b"ABCD");
        assert_eq!(fs::read(mixed.join("parts/armor.dcx")).unwrap(), b"new");
        assert_eq!(fs::read(mixed.join("parts/same.dcx")).unwrap(), b"shared");
        assert_eq!(fs::read(mixed.join("parts/new.dcx")).unwrap(), b"brand new");
        assert_eq!(fs::read(base.join("parts/armor.dcx")).unwrap(), b"old");
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
    }

    #[test]
    fn mod_review_rejects_changed_source_or_active_profile_before_publishing() {
        let (tmp, game) = fixture();
        baseline(&game).unwrap();
        let pack = tmp.path().join("pack");
        fs::create_dir(&pack).unwrap();
        fs::write(pack.join("regulation.bin"), b"one").unwrap();
        let preview = preview_mods(&game, &pack).unwrap();
        fs::write(pack.join("regulation.bin"), b"two").unwrap();
        assert!(profile_from_review(&game, &pack, "변경된 내용", &preview.token, &[]).is_err());
        assert!(inspect(&game).unwrap().profiles.is_empty());
        let preview = preview_mods(&game, &pack).unwrap();
        let other = create_profile(&game, "빈 구성").unwrap();
        switch(&game, &other.profiles[0].id).unwrap();
        assert!(profile_from_review(&game, &pack, "다른 프로필", &preview.token, &[]).is_err());
        assert_eq!(inspect(&game).unwrap().profiles.len(), 1);
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
    }

    #[test]
    #[ignore = "Windows NTFS release check: stream a 70 GB sparse archive"]
    fn seventy_gb_base_is_shared_by_mod_profiles() {
        use std::os::windows::fs::MetadataExt;
        let (tmp, game) = fixture();
        let archive = game.join("Data0.bdt");
        File::create_new(&archive).unwrap();
        let flag = std::process::Command::new("fsutil.exe")
            .args(["sparse", "setflag"])
            .arg(&archive)
            .output()
            .unwrap();
        assert!(flag.status.success());
        assert_ne!(archive.metadata().unwrap().file_attributes() & 0x200, 0);
        OpenOptions::new()
            .write(true)
            .open(&archive)
            .unwrap()
            .set_len(70_000_000_000)
            .unwrap();
        let last_write = archive.metadata().unwrap().last_write_time();
        baseline(&game).unwrap();
        let source = tmp.path().join("armor");
        fs::create_dir_all(source.join("parts")).unwrap();
        fs::write(
            source.join("parts/test.partsbnd.dcx"),
            vec![0x42; 2 * 1024 * 1024],
        )
        .unwrap();
        let review = preview_mods(&game, &source).unwrap();
        let added = profile_from_review(&game, &source, "갑옷", &review.token, &[]).unwrap();
        let p = &added.profiles[0];
        switch(&game, &p.id).unwrap();
        let dir = active_package(&game).unwrap();
        crate::modpack::check_profile(&dir).unwrap();
        let profile_bytes = files::payload_bytes(&dir).unwrap();
        assert_eq!(p.bytes, Some(2 * 1024 * 1024));
        assert!(profile_bytes < 2 * 1024 * 1024 + 4096);
        assert!(!dir.join("mod/Data0.bdt").exists());
        switch(&game, VANILLA).unwrap();
        assert_eq!(archive.metadata().unwrap().len(), 70_000_000_000);
        assert_eq!(archive.metadata().unwrap().last_write_time(), last_write);
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        println!("base=70000000000 bytes; mod=2097152 bytes; profile including manifest={profile_bytes} bytes");
    }

    // Simulate data written by version 1, including omitted package fields.
    fn legacy_park(game: &Path, name: &str) -> View {
        let layout = Layout::new(game).unwrap();
        layout.create(false).unwrap();
        let mut state = State::default();
        let profile = state.profile(name, None, &layout.vault).unwrap();
        layout.transact(&mut state, Job::Park { profile }).unwrap();
        let json = layout.vault.join("state.json");
        let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&json).unwrap()).unwrap();
        v["version"] = 1.into();
        let saved = v["profiles"][0].as_object_mut().unwrap();
        saved.remove("kind");
        saved.remove("bytes");
        files::atomic_json(&json, &v).unwrap();
        inspect(game).unwrap()
    }

    #[test]
    fn fresh_onboard_needs_official_file_list_and_keeps_unrecognized_install() {
        let (_tmp, game) = fixture();
        assert!(onboard_modded(&game, "현재 모드").is_err());
        assert!(game.join("eldenring.exe").is_file());
        assert!(!game.with_file_name(VAULT).exists());
    }

    #[test]
    fn legacy_full_install_roundtrip_including_unmanaged_extras() {
        let (_tmp, game) = fixture();
        fs::write(game.join("my-mod.ini"), b"personal settings").unwrap();
        let view = inspect(&game).unwrap();
        assert_eq!(view.phase, "unmanaged");
        assert!(!Path::new(&view.vault_path).exists());
        let waiting = legacy_park(&game, "현재 모드");
        assert_eq!(waiting.phase, "awaitingVanilla");
        let id = &waiting.profiles[0].id;
        assert!(!game.exists());
        assert_eq!(inspect(&game).unwrap().phase, "awaitingVanilla");
        assert!(switch(&game, id).is_err());
        game_files(&game);
        baseline(&game).unwrap();
        switch(&game, id).unwrap();
        assert_eq!(
            fs::read(game.join("my-mod.ini")).unwrap(),
            b"personal settings"
        );
        assert!(verify(&game).unwrap().matches);
        assert!(baseline(&game).is_err());
        switch(&game, VANILLA).unwrap();
        assert!(!game.join("my-mod.ini").exists());
        assert!(verify(&game).unwrap().matches);
    }

    #[test]
    fn cloned_profiles_are_independent_and_local_baseline_cannot_launch() {
        let (_tmp, game) = fixture();
        baseline(&game).unwrap();
        let view = create_profile(&game, "실험용").unwrap();
        let id = &view.profiles[0].id;
        switch(&game, id).unwrap();
        let root = active_package(&game).unwrap();
        fs::write(root.join("mod/regulation.bin"), b"mod data").unwrap();
        assert_eq!(inspect(&game).unwrap().profiles[0].bytes, Some(8));
        assert!(verify(&game).unwrap().matches);
        switch(&game, VANILLA).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        assert!(
            launch_verified(&game, || panic!("cannot launch an unofficial reference")).is_err()
        );
        fs::write(game.join("regulation.bin"), b"tampered").unwrap();
        fs::write(game.join("unknown.dat"), b"extra").unwrap();
        fs::remove_file(game.join("eldenring.exe")).unwrap();
        let result = verify(&game).unwrap();
        assert!(!result.matches);
        assert_eq!(result.added, vec!["unknown.dat"]);
        assert_eq!(result.changed, vec!["regulation.bin"]);
        assert_eq!(result.missing, vec!["eldenring.exe"]);
    }

    #[test]
    fn undo_first_park_preserves_an_existing_game_and_accepts_new_name() {
        let (tmp, game) = fixture();
        let view = legacy_park(&game, "현재 설치");
        let id = &view.profiles[0].id;
        let renamed = rename_profile(&game, id, " 첫 프로필 ").unwrap();
        assert_eq!(renamed.profiles[0].name, "첫 프로필");
        fs::create_dir(&game).unwrap();
        assert!(undo_park(&game, id).is_err());
        fs::rename(&game, tmp.path().join("another-game")).unwrap();
        let view = undo_park(&game, id).unwrap();
        assert_eq!(view.phase, "unmanaged");
        assert!(view.profiles.is_empty());
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
    }

    #[test]
    fn overlay_modifies_only_its_published_profile() {
        let (tmp, game) = fixture();
        let pack = tmp.path().join("package");
        fs::create_dir_all(pack.join("parts")).unwrap();
        fs::write(pack.join("parts/coat.partsbnd.dcx"), b"test").unwrap();
        fs::write(pack.join("regulation.bin"), b"modded").unwrap();
        let original = game.join("regulation.bin");
        let permissions = fs::metadata(&original).unwrap().permissions();
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&original, read_only).unwrap();
        baseline(&game).unwrap();
        let review = preview_mods(&game, &pack).unwrap();
        let view = profile_from_review(&game, &pack, "패치", &review.token, &[]).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        switch(&game, &view.profiles[0].id).unwrap();
        let root = active_package(&game).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        assert_eq!(
            fs::read(root.join("mod/regulation.bin")).unwrap(),
            b"modded"
        );
        assert_eq!(
            fs::read(root.join("mod/parts/coat.partsbnd.dcx")).unwrap(),
            b"test"
        );
        assert!(root.join("profile.me3").is_file());
        assert!(!game.join("parts/coat.partsbnd.dcx").exists());
        assert!(!root.join("eldenring.exe").exists());
        assert!(!game.join(files::PARTIAL).exists());
        switch(&game, VANILLA).unwrap();
        assert!(fs::metadata(&original).unwrap().permissions().readonly());
        fs::set_permissions(&original, permissions).unwrap();
        assert!(verify(&game).unwrap().matches);
    }

    #[test]
    fn interrupted_copy_discards_only_recorded_app_owned_temporary_tree() {
        let (_tmp, game) = fixture();
        baseline(&game).unwrap();
        let layout = Layout::new(&game).unwrap();
        let mut state = layout.state().unwrap();
        let name = "copy-Ab1Z";
        let tmp = layout.vault.join(name);
        fs::create_dir(&tmp).unwrap();
        fs::write(tmp.join("partial.bin"), b"half").unwrap();
        state.job = Some(Job::Copying {
            staging: name.into(),
        });
        layout.save(&state).unwrap();
        assert!(recover(&game).is_err());
        assert!(tmp.join("partial.bin").exists());
        fs::write(tmp.join(files::PARTIAL), name).unwrap();
        assert_eq!(recover(&game).unwrap().phase, "ready");
        assert!(!tmp.exists());
    }

    #[test]
    fn releasing_inactive_profile_is_recoverable_without_deleting_its_contents() {
        let (tmp, game) = fixture();
        baseline(&game).unwrap();
        let view = create_profile(&game, "내보내기").unwrap();
        let id = &view.profiles[0].id;
        switch(&game, id).unwrap();
        fs::write(
            active_package(&game).unwrap().join("mod/regulation.bin"),
            b"keep",
        )
        .unwrap();
        assert!(release_profile(&game, id).is_err());
        switch(&game, VANILLA).unwrap();
        let layout = Layout::new(&game).unwrap();
        let mut disk = layout.state().unwrap();
        let release = Released {
            id: id.clone(),
            serial: 1,
        };
        let dest = release.path(&game);
        disk.job = Some(Job::Release(release));
        layout.save(&disk).unwrap();
        // A third-party collision is never replaced, even during recovery.
        fs::create_dir(&dest).unwrap();
        fs::write(dest.join("external"), b"keep").unwrap();
        assert!(recover(&game).is_err());
        fs::rename(&dest, tmp.path().join("existing-folder")).unwrap();
        // Simulate a termination after the physical move and before metadata commit.
        fs::rename(layout.vault.join(id), &dest).unwrap();
        assert_eq!(
            recover(&game).unwrap().last_released_path.as_deref(),
            dest.to_str()
        );
        assert_eq!(release_profile(&game, id).unwrap(), dest);
        assert_eq!(fs::read(dest.join("mod/regulation.bin")).unwrap(), b"keep");
        let next = create_profile(&game, "새 프로필").unwrap().profiles[0]
            .id
            .clone();
        assert_ne!(next, *id);
        let fresh = release_profile(&game, &next).unwrap();
        assert_ne!(fresh, dest);
        assert!(fresh.join("profile.me3").is_file());
        assert!(!fresh.join("mod/regulation.bin").exists());
        assert!(verify(&game).unwrap().matches);
    }

    #[test]
    fn interrupted_switch_resumes_and_refuses_an_external_conflict() {
        let (_tmp, game) = fixture();
        let view = legacy_park(&game, "중단 복구");
        game_files(&game);
        baseline(&game).unwrap();
        let layout = Layout::new(&game).unwrap();
        let mut disk = layout.state().unwrap();
        let id = view.profiles[0].id.clone();
        disk.job = Some(Job::Swap {
            from: VANILLA.into(),
            to: id.clone(),
        });
        layout.save(&disk).unwrap();
        fs::rename(&game, layout.vault.join(VANILLA)).unwrap();
        // Steam may have created a third, distinct installation in the meantime.
        fs::create_dir(&game).unwrap();
        fs::write(game.join("third"), b"preserve me").unwrap();
        assert!(recover(&game).is_err());
        assert_eq!(fs::read(game.join("third")).unwrap(), b"preserve me");
        fs::rename(&game, game.with_file_name("unexpected-backup")).unwrap();
        assert_eq!(recover(&game).unwrap().active_id, Some(id));
        assert!(verify(&game).unwrap().matches);
    }

    #[test]
    fn prevents_stale_mod_and_never_replaces_an_existing_vault() {
        let (tmp, game) = fixture();
        let vault = tmp.path().join(VAULT);
        fs::create_dir(&vault).unwrap();
        fs::write(vault.join("belongings"), b"retain").unwrap();
        assert!(baseline(&game).is_err());
        fs::rename(&vault, tmp.path().join("prior")).unwrap();
        baseline(&game).unwrap();
        let view = create_profile(&game, "구버전").unwrap();
        let layout = Layout::new(&game).unwrap();
        let mut state = layout.state().unwrap();
        state.profiles[0].build = Some("obsolete".into());
        layout.save(&state).unwrap();
        assert!(switch(&game, &view.profiles[0].id).is_err());
        assert_eq!(inspect(&game).unwrap().active_id.as_deref(), Some(VANILLA));
    }

    #[test]
    fn switching_mixed_v1_and_compact_profiles_uses_one_vanilla() {
        let (_tmp, game) = fixture();
        fs::write(game.join("regulation.bin"), b"old patch").unwrap();
        let previous = legacy_park(&game, "v1");
        let full = &previous.profiles[0].id;
        game_files(&game);
        baseline(&game).unwrap();
        let report = convert_profile(&game, full, "경량 변환").unwrap();
        let package = &report.profiles[1];
        assert!(package.kind == Kind::Package);
        assert_eq!(package.bytes, Some(9));
        let root = package_folder(&game, &package.id).unwrap();
        assert_eq!(
            fs::read(root.join("mod/regulation.bin")).unwrap(),
            b"old patch"
        );
        assert!(!root.join("mod/eldenring.exe").exists());
        assert!(package_folder(&game, full)
            .unwrap()
            .join("eldenring.exe")
            .exists());
        switch(&game, &package.id).unwrap();
        switch(&game, full).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"old patch");
        assert!(verify(&game).unwrap().matches);
        switch(&game, &package.id).unwrap();
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        assert_eq!(active_package(&game).unwrap(), root);
        assert!(verify(&game).unwrap().matches);
        assert!(launch_verified(&game, || panic!("wrong mode")).is_err());
    }

    #[test]
    fn large_untouched_archive_is_never_stored_and_clones_are_independent() {
        let (tmp, game) = fixture();
        const SIZE: u64 = 8 * 1024 * 1024;
        File::create(game.join("Data0.bdt"))
            .unwrap()
            .set_len(SIZE)
            .unwrap();
        baseline(&game).unwrap();
        let source = tmp.path().join("GameBackup");
        game_files(&source);
        File::create(source.join("Data0.bdt"))
            .unwrap()
            .set_len(SIZE)
            .unwrap();
        fs::write(source.join("regulation.bin"), b"12345").unwrap();
        let view = import_profile(&game, &source, "기존 설치").unwrap();
        let profile = &view.profiles[0];
        let vault = Path::new(&view.vault_path);
        assert_eq!(profile.bytes, Some(5));
        assert!(files::payload_bytes(vault).unwrap() < 8192);
        switch(&game, &profile.id).unwrap();
        let cloned = create_profile(&game, "복제").unwrap();
        let second = &cloned.profiles[1];
        assert_eq!(second.bytes, Some(5));
        let source_asset = vault.join(&profile.id).join("mod/regulation.bin");
        fs::write(&source_asset, b"edited").unwrap();
        switch(&game, &second.id).unwrap();
        assert_eq!(
            fs::read(active_package(&game).unwrap().join("mod/regulation.bin")).unwrap(),
            b"12345"
        );
        assert!(files::payload_bytes(vault).unwrap() < 8192);
        switch(&game, VANILLA).unwrap();
        assert_eq!(fs::metadata(game.join("Data0.bdt")).unwrap().len(), SIZE);
        assert!(verify(&game).unwrap().matches);
        fs::remove_file(source.join("Data0.bdt")).unwrap();
        assert!(import_profile(&game, &source, "누락된 파일").is_err());
        assert_eq!(inspect(&game).unwrap().profiles.len(), 2);
    }

    #[test]
    fn full_import_preserves_non_loader_files_outside_the_package_including_after_clone() {
        let (tmp, game) = fixture();
        baseline(&game).unwrap();
        let src = tmp.path().join("previous-install");
        game_files(&src);
        fs::write(src.join("regulation.bin"), b"patch").unwrap();
        fs::write(src.join("tool.ini"), b"metadata").unwrap();
        let view = import_profile(&game, &src, "혼합").unwrap();
        assert_eq!(view.profiles[0].bytes, Some(13));
        assert_eq!(view.profiles[0].archived_files, 1);
        let id = &view.profiles[0].id;
        switch(&game, id).unwrap();
        let other = create_profile(&game, "사본").unwrap();
        let root = package_folder(&game, &other.profiles[1].id).unwrap();
        assert_eq!(fs::read(root.join("mod/regulation.bin")).unwrap(), b"patch");
        assert_eq!(
            fs::read(root.join("preserved/tool.ini")).unwrap(),
            b"metadata"
        );
        assert!(!root.join("mod/tool.ini").exists());
        assert_eq!(other.profiles[1].archived_files, 1);
        assert!(!game.join("tool.ini").exists());
    }

    #[test]
    fn compact_first_capture_keeps_base_archives_and_undo_is_journaled() {
        use sha2::{Digest, Sha256};
        let (_tmp, game) = fixture();
        fs::write(game.join("extra.ini"), b"other").unwrap();
        File::create(game.join("Data0.bdt"))
            .unwrap()
            .set_len(2 * 1024 * 1024)
            .unwrap();
        let layout = Layout::new(&game).unwrap();
        layout.create(false).unwrap();
        let mut state = State::default();
        let mut profile = state.profile("보관본", None, &layout.vault).unwrap();
        profile.kind = Kind::Package;
        let plan: crate::adoption::Plan = serde_json::from_value(serde_json::json!({"files":[
            {"relative":"regulation.bin","size":8,"sha256":format!("{:x}",Sha256::digest(b"original")),"loose":true},
            {"relative":"extra.ini","size":5,"sha256":format!("{:x}",Sha256::digest(b"other")),"loose":false}
        ]})).unwrap();
        let id = profile.id.clone();
        state.job = Some(Job::Capture { profile, plan });
        layout.save(&state).unwrap();
        let waiting = recover(&game).unwrap();
        assert_eq!(waiting.phase, "awaitingVanilla");
        assert_eq!(waiting.profiles[0].bytes, Some(13));
        assert_eq!(waiting.profiles[0].archived_files, 1);
        assert!(game.join("Data0.bdt").is_file());
        assert!(!game.join("extra.ini").exists());
        // A file recreated by Steam must not be overwritten or strand the journal.
        fs::write(game.join("regulation.bin"), b"new from Steam").unwrap();
        assert!(undo_park(&game, &id).is_err());
        assert_eq!(inspect(&game).unwrap().phase, "awaitingVanilla");
        fs::remove_file(game.join("regulation.bin")).unwrap();
        let undone = undo_park(&game, &id).unwrap();
        assert_eq!(undone.phase, "unmanaged");
        assert!(undone.profiles.is_empty());
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
        assert_eq!(fs::read(game.join("extra.ini")).unwrap(), b"other");
    }

    #[test]
    fn failed_capture_with_every_original_still_present_can_be_replanned() {
        use sha2::{Digest, Sha256};
        let (_tmp, game) = fixture();
        let layout = Layout::new(&game).unwrap();
        layout.create(false).unwrap();
        let mut state = State::default();
        let mut profile = state.profile("실패", None, &layout.vault).unwrap();
        profile.kind = Kind::Package;
        let plan:crate::adoption::Plan=serde_json::from_value(serde_json::json!({"files":[
            {"relative":"regulation.bin","size":7,"sha256":format!("{:x}",Sha256::digest(b"earlier")),"loose":true}
        ]})).unwrap();
        state.job = Some(Job::Capture { profile, plan });
        layout.save(&state).unwrap();
        assert!(recover(&game).is_err());
        assert_eq!(inspect(&game).unwrap().phase, "unmanaged");
        assert_eq!(fs::read(game.join("regulation.bin")).unwrap(), b"original");
    }

    #[test]
    fn explicitly_selected_native_entry_and_companions_survive_cloning_without_game_writes() {
        let (tmp, game) = fixture();
        baseline(&game).unwrap();
        let bundle = tmp.path().join("user-plugin");
        fs::create_dir_all(bundle.join("settings")).unwrap();
        let dll = bundle.join("example.dll");
        fs::write(&dll, b"sample").unwrap();
        fs::write(bundle.join("settings/menu.ini"), b"prefs").unwrap();
        let installed = profile_from_native(&game, &bundle, &dll, "플러그인").unwrap();
        let source = &installed.profiles[0];
        assert_eq!(source.bytes, Some(11));
        switch(&game, &source.id).unwrap();
        let clone = create_profile(&game, "사본").unwrap();
        let second = &clone.profiles[1];
        assert_eq!(second.bytes, Some(11));
        let root = package_folder(&game, &second.id).unwrap();
        let paths = crate::modpack::walk(&root.join("native")).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|(p, _)| p.ends_with("settings/menu.ini")));
        let config = fs::read_to_string(root.join(crate::modpack::PROFILE_FILE)).unwrap();
        assert!(config.contains("example.dll"));
        assert!(!config.contains("menu.ini"));
        crate::modpack::check_profile(&root).unwrap();
        assert!(!game.join("example.dll").exists());
        assert!(verify(&game).unwrap().matches);
    }

    #[test]
    fn official_hashes_block_same_length_edits_and_runtime_exception_is_narrow() {
        use crate::depot::OfficialFile;
        use sha1::{Digest, Sha1};
        use std::collections::BTreeMap;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("Game.Bin"), b"data").unwrap();
        let expected = BTreeMap::from([(
            "game.bin".into(),
            OfficialFile {
                size: 4,
                sha1: format!("{:x}", Sha1::digest(b"data")),
            },
        )]);
        let trusted = scan(root, Some(&expected), None).unwrap();
        fs::create_dir_all(root.join("crashdumps/reports")).unwrap();
        fs::write(root.join("crashdumps/settings.dat"), b"runtime").unwrap();
        assert!(scan(root, Some(&expected), None).is_ok());
        fs::write(root.join("crashdumps/unknown.dll"), b"unexpected").unwrap();
        assert!(scan(root, Some(&expected), None).is_err());
        let local = scan(root, None, Some(&trusted)).unwrap();
        assert!(local.contains_key("crashdumps/unknown.dll"));
        assert!(!local.contains_key("crashdumps/settings.dat"));
        fs::remove_file(root.join("crashdumps/unknown.dll")).unwrap();
        fs::write(root.join("Game.Bin"), b"edit").unwrap();
        assert!(scan(root, Some(&expected), None).is_err());
    }
}
