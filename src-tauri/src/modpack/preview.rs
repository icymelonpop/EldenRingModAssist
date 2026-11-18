use super::{metadata, Overlay, SourceFile};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModPreview {
    pub token: String,
    pub base_name: String,
    pub added: Vec<String>,
    pub identical: Vec<String>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Conflict {
    pub path: String,
    existing_bytes: u64,
    incoming_bytes: u64,
}

fn path_name(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_hash(file: &mut SourceFile) -> Result<[u8; 32], String> {
    let path = &file.source;
    if !metadata(path)?.is_file() {
        return Err(format!("비교할 파일을 확인하세요. {}", path.display()));
    }
    let hash = (|| -> io::Result<[u8; 32]> {
        let mut reader = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)?;
        let mut digest = Sha256::new();
        let size = io::copy(&mut reader, &mut digest)?;
        if size != file.before.len() {
            return Err(io::Error::other(
                "폴더를 읽는 동안 크기가 바뀌었습니다. 다시 비교하세요.",
            ));
        }
        Ok(digest.finalize().into())
    })()
    .map_err(|e| format!("{} · {e}", path.display()))?;
    file.reviewed_hash = Some(hash);
    Ok(hash)
}

// Variable-length fields are length-prefixed so path and profile boundaries
// are unambiguous. File contents are verified again as the new copy is written.
fn field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
}

impl ModPreview {
    pub(crate) fn compare(
        incoming: &mut Overlay,
        inherited: Option<&mut Overlay>,
        active_id: &str,
        base_name: String,
        build: Option<&str>,
    ) -> Result<Self, String> {
        let mut preview = Self {
            token: String::new(),
            base_name,
            added: vec![],
            identical: vec![],
            conflicts: vec![],
        };
        let mut digest = Sha256::new();
        field(&mut digest, active_id.as_bytes());
        field(&mut digest, build.unwrap_or_default().as_bytes());
        let mut originals = BTreeMap::new();
        if let Some(base) = inherited {
            for file in &mut base.files {
                originals.insert(path_name(&file.relative).to_lowercase(), file);
            }
        }
        for file in &mut incoming.files {
            let path = path_name(&file.relative);
            let key = path.to_lowercase();
            let children = format!("{key}/");
            if originals
                .range(children.clone()..)
                .next()
                .is_some_and(|(name, _)| name.starts_with(&children))
                || file
                    .relative
                    .ancestors()
                    .skip(1)
                    .any(|p| originals.contains_key(&path_name(p).to_lowercase()))
            {
                return Err(format!("파일과 폴더의 경로가 겹쳐 함께 담을 수 없습니다. {path} · 모드의 폴더 구성을 확인하세요."));
            }
            field(&mut digest, path.as_bytes());
            let new_hash = file_hash(file)?;
            digest.update(new_hash);
            if let Some(old) = originals.get_mut(&key) {
                let old_hash = file_hash(old)?;
                digest.update([1]);
                digest.update(old_hash);
                if old_hash == new_hash {
                    preview.identical.push(path);
                } else {
                    preview.conflicts.push(Conflict {
                        path,
                        existing_bytes: old.before.len(),
                        incoming_bytes: file.before.len(),
                    });
                }
            } else {
                digest.update([0]);
                preview.added.push(path);
            }
        }
        preview.token = format!("{:x}", digest.finalize());
        Ok(preview)
    }

    pub(crate) fn select(
        &self,
        token: &str,
        keep: &[String],
        incoming: &mut Overlay,
        inherited: Option<&mut Overlay>,
    ) -> Result<(), String> {
        if self.token != token {
            return Err("미리보기 이후 프로필 또는 모드 파일이 바뀌었습니다. 모드 폴더를 다시 선택해 비교하세요.".into());
        }
        let choices: HashSet<_> = keep.iter().map(String::as_str).collect();
        let conflicts: HashSet<_> = self.conflicts.iter().map(|c| c.path.as_str()).collect();
        if choices.len() != keep.len() || !choices.is_subset(&conflicts) {
            return Err("미리보기의 충돌 파일 중에서 유지할 파일을 선택하세요.".into());
        }
        let same: HashSet<_> = self.identical.iter().map(String::as_str).collect();
        incoming.files.retain(|file| {
            let name = path_name(&file.relative);
            !choices.contains(name.as_str()) && !same.contains(name.as_str())
        });
        incoming.bytes = incoming.files.iter().map(|f| f.before.len()).sum();
        if let Some(base) = inherited {
            let replaced: HashSet<_> = conflicts
                .difference(&choices)
                .map(|p| p.to_lowercase())
                .collect();
            base.files
                .retain(|f| !replaced.contains(&path_name(&f.relative).to_lowercase()));
            base.bytes = base.files.iter().map(|f| f.before.len()).sum();
        }
        Ok(())
    }
}
