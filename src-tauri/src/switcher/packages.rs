use super::{
    check_build, check_game, directory, files, io, mutate, Job, Kind, Layout, Profile, State, View,
    R,
};
use crate::modpack::{ModPreview, Overlay, PACKAGE_DIR};
use std::fs;
use std::path::{Path, PathBuf};

pub fn create_profile(game: &Path, name: &str) -> R<View> {
    make_package(game, name, Inherit::Active, Addition::None)
}
pub fn import_profile(game: &Path, source: &Path, name: &str) -> R<View> {
    make_package(game, name, Inherit::External(source), Addition::None)
}
pub fn profile_from_review(
    game: &Path,
    source: &Path,
    name: &str,
    token: &str,
    keep: &[String],
) -> R<View> {
    make_package(
        game,
        name,
        Inherit::Active,
        Addition::Loose {
            source,
            token,
            keep,
        },
    )
}
pub fn profile_from_native(game: &Path, source: &Path, entry: &Path, name: &str) -> R<View> {
    make_package(
        game,
        name,
        Inherit::Active,
        Addition::Native { source, entry },
    )
}
pub fn convert_profile(game: &Path, id: &str, name: &str) -> R<View> {
    make_package(game, name, Inherit::Legacy(id), Addition::None)
}

#[derive(Clone, Copy)]
enum Inherit<'a> {
    Active,
    External(&'a Path),
    Legacy(&'a str),
}

enum Addition<'a> {
    None,
    Loose {
        source: &'a Path,
        token: &'a str,
        keep: &'a [String],
    },
    Native {
        source: &'a Path,
        entry: &'a Path,
    },
}

enum Source {
    Vanilla,
    Package(PathBuf),
    Installation(PathBuf),
}

// Existing packages can be cloned without opening Game. Old full replicas need
// a single hash comparison with the baseline to identify the files to preserve.
fn resolve(
    layout: &Layout,
    state: &State,
    inherit: Inherit<'_>,
    build: &Option<String>,
) -> R<Source> {
    let source_profile = match inherit {
        Inherit::Active => state
            .profiles
            .iter()
            .find(|p| Some(&p.id) == state.active.as_ref()),
        Inherit::Legacy(id) => Some(
            state
                .profiles
                .iter()
                .find(|p| p.id == id && p.kind == Kind::Legacy)
                .ok_or("변환할 이전 형식의 프로필을 선택하세요.")?,
        ),
        Inherit::External(_) => None,
    };
    if source_profile.is_some_and(|p| p.build.as_ref() != build.as_ref()) {
        return Err(
            "현재 게임과 다른 빌드에서 만든 프로필입니다. 현재 버전의 모드 폴더를 선택하세요."
                .into(),
        );
    }
    let vault = fs::canonicalize(&layout.vault).map_err(|e| io(&layout.vault, e))?;
    let source = match (inherit, source_profile) {
        (Inherit::External(path), _) => {
            check_game(path, false)?;
            let real = fs::canonicalize(path).map_err(|e| io(path, e))?;
            if vault.starts_with(&real) || real.starts_with(&vault) {
                return Err("보관함 밖에 있는 게임 폴더를 선택하세요. 기존 프로필에는 경량 변환을 사용하세요.".into());
            }
            if crate::platform::steam_install_state(path)?
                .is_some_and(|other| Some(&other) != build.as_ref())
            {
                return Err("현재 Steam 빌드와 다른 설치본입니다.".into());
            }
            Source::Installation(real)
        }
        (_, Some(p)) if p.kind == Kind::Package => Source::Package(vault.join(&p.id)),
        (_, Some(p)) => Source::Installation(if Some(&p.id) == state.active.as_ref() {
            layout.game.to_owned()
        } else {
            vault.join(&p.id)
        }),
        (_, None) => Source::Vanilla,
    };
    Ok(source)
}

struct InheritedFiles {
    assets: Option<Overlay>,
    archive: Option<files::Archive>,
    inherited_folder: Option<PathBuf>,
    diff: Option<files::Difference>,
}

fn read_base(
    layout: &Layout,
    state: &State,
    build: &Option<String>,
    inherit: Inherit<'_>,
) -> R<InheritedFiles> {
    state.settled()?;
    let base = state
        .baseline
        .as_ref()
        .ok_or("먼저 바닐라 원본을 등록하세요.")?;
    if &base.build != build {
        return Err("Steam 빌드가 변경되었습니다. 바닐라를 복구하고 기준을 갱신하세요.".into());
    }
    let source = resolve(layout, state, inherit, build)?;
    let (whole_install, inherited_folder) = match source {
        Source::Installation(path) => (Some(path), None),
        Source::Package(path) => (None, Some(path)),
        Source::Vanilla => (None, None),
    };
    let diff = whole_install
        .map(|path| {
            check_game(&path, false)?;
            files::Difference::compare(&path, &base.files)
        })
        .transpose()?;
    let (assets, archive) = if let Some(delta) = &diff {
        let (loose, preserved) = delta
            .paths
            .iter()
            .cloned()
            .partition(|p: &PathBuf| crate::modpack::is_loose_asset(p));
        (
            Some(Overlay::selected(&delta.source, loose)?),
            files::Archive::select(&delta.source, preserved)?,
        )
    } else if let Some(ref source) = inherited_folder {
        if !directory(source)? {
            return Err("프로필 보관 폴더가 없습니다.".into());
        }
        let asset_root = source.join(PACKAGE_DIR);
        let list = crate::modpack::walk(&asset_root)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let archive_root = source.join("preserved");
        let archive = if directory(&archive_root)? {
            let paths = crate::modpack::walk(&archive_root)?
                .into_iter()
                .map(|(p, _)| p)
                .collect();
            files::Archive::select(&archive_root, paths)?
        } else {
            None
        };
        (Some(Overlay::selected(&asset_root, list)?), archive)
    } else {
        (None, None)
    };
    Ok(InheritedFiles {
        assets,
        archive,
        inherited_folder,
        diff,
    })
}

fn review(
    state: &State,
    build: &Option<String>,
    assets: Option<&mut Overlay>,
    incoming: &mut Overlay,
) -> R<ModPreview> {
    let id = state.active.as_deref().ok_or("현재 프로필을 확인하세요.")?;
    let name = state
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.as_str())
        .unwrap_or("바닐라");
    ModPreview::compare(incoming, assets, id, name.to_owned(), build.as_deref())
}

pub fn preview_mods(game: &Path, source: &Path) -> R<ModPreview> {
    let layout = Layout::new(game)?;
    if !layout.owned()? {
        return Err("먼저 바닐라 원본을 등록하세요.".into());
    }
    let _lock = layout.lock()?;
    let state = layout.state()?;
    let build = crate::platform::steam_install_state(game)?;
    let mut inherited = read_base(&layout, &state, &build, Inherit::Active)?;
    let mut incoming = Overlay::read(source, game, &layout.vault)?;
    review(&state, &build, inherited.assets.as_mut(), &mut incoming)
}

fn make_package(game: &Path, name: &str, inherit: Inherit<'_>, add: Addition<'_>) -> R<View> {
    mutate(game, false, |layout, state, build| {
        state.validate_name(name, None)?;
        let InheritedFiles {
            mut assets,
            archive,
            inherited_folder,
            diff,
        } = read_base(layout, state, &build, inherit)?;
        let (overlay, native) = match add {
            Addition::None => (None, None),
            Addition::Loose {
                source,
                token,
                keep,
            } => {
                let mut addition = Overlay::read(source, game, &layout.vault)?;
                let preview = review(state, &build, assets.as_mut(), &mut addition)?;
                preview.select(token, keep, &mut addition, assets.as_mut())?;
                (Some(addition), None)
            }
            Addition::Native { source, entry } => (
                None,
                Some(crate::modpack::NativeOverlay::read(
                    source,
                    entry,
                    game,
                    &layout.vault,
                )?),
            ),
        };
        let inherited_native_bytes = inherited_folder
            .as_deref()
            .map(crate::modpack::native_bytes)
            .transpose()?
            .unwrap_or(0);
        let mut profile = state.profile(name, build.clone(), &layout.vault)?;
        profile.kind = Kind::Package;
        stage_profile(layout, state, profile, |stage| {
            let payload = stage.join(PACKAGE_DIR);
            fs::create_dir(&payload).map_err(|e| io(&payload, e))?;
            let archived_bytes = archive.as_ref().map_or(0, |a| a.bytes);
            let bytes = assets
                .as_ref()
                .map_or(0, Overlay::bytes)
                .saturating_add(overlay.as_ref().map_or(0, Overlay::bytes))
                .saturating_add(archived_bytes)
                .saturating_add(inherited_native_bytes)
                .saturating_add(
                    native
                        .as_ref()
                        .map_or(0, crate::modpack::NativeOverlay::bytes),
                );
            files::check_space(&payload, bytes)?;
            if let Some(delta) = assets {
                delta.apply(&payload)?;
            }
            if let Some(archive) = archive {
                archive.copy_into(&stage.join("preserved"))?;
            }
            if let Some(ref original) = inherited_folder {
                crate::modpack::inherit_natives(original, stage)?;
            }
            if let Some(addition) = overlay {
                addition.apply(&payload)?;
            }
            if let Some(native) = native {
                native.apply(stage)?;
            }
            if let Some(delta) = diff {
                delta.unchanged()?;
            }
            crate::modpack::write_profile(stage)?;
            check_build(game, &build)?;
            Ok(files::payload_bytes(&payload)?
                .saturating_add(archived_bytes)
                .saturating_add(crate::modpack::native_bytes(stage)?))
        })
    })
}

// A newly assembled package is published with a single same-volume rename.
fn stage_profile(
    layout: &Layout,
    state: &mut State,
    mut profile: Profile,
    populate: impl FnOnce(&Path) -> R<u64>,
) -> R<()> {
    let temp = tempfile::Builder::new()
        .prefix("copy-")
        .tempdir_in(&layout.vault)
        .map_err(|e| io(&layout.vault, e))?;
    let staging = temp
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let marker = temp.path().join(files::PARTIAL);
    fs::write(&marker, staging.as_bytes()).map_err(|e| io(&marker, e))?;
    state.job = Some(Job::Copying {
        staging: staging.clone(),
    });
    layout.save(state)?;
    let temp = temp.keep();
    match populate(&temp) {
        Ok(bytes) => profile.bytes = Some(bytes),
        Err(reason) => {
            layout
                .finish(state)
                .map_err(|e| format!("{reason} · 부분 복사 복구가 필요합니다. {e}"))?;
            return Err(reason);
        }
    }
    layout.transact(state, Job::Publish { profile, staging })
}
