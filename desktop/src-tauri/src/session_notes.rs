//! User-authored notes keyed by Harness profile and real session ID.
use crate::{Desktop, UiError};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use tauri::State;

const MAX_NOTES: usize = 500;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SessionNote {
    pub profile: String,
    pub session_id: String,
    pub content: String,
    pub updated_at: u64,
}

fn file(root: &Path) -> std::path::PathBuf {
    root.join("session-notes.json")
}

pub fn validate(items: &[SessionNote]) -> Result<(), String> {
    if items.len() > MAX_NOTES {
        return Err("会话备注数量超过限制".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for item in items {
        if item.profile.is_empty()
            || item.profile.len() > 4096
            || item.profile.contains('\0')
            || item.session_id.is_empty()
            || item.session_id.len() > 256
            || item.session_id.contains('\0')
            || item.content.trim().is_empty()
            || item.content.chars().count() > 4000
            || item.content.contains('\0')
            || !keys.insert((item.profile.as_str(), item.session_id.as_str()))
        {
            return Err("会话备注字段无效或超过限制".into());
        }
    }
    Ok(())
}

fn read(root: &Path) -> Result<Vec<SessionNote>, UiError> {
    let path = file(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| UiError::new("session_notes", "会话备注无法读取。"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FILE_BYTES {
        return Err(UiError::new(
            "session_notes",
            "会话备注文件类型或大小不允许。",
        ));
    }
    let items: Vec<SessionNote> = serde_json::from_slice(
        &fs::read(path).map_err(|_| UiError::new("session_notes", "会话备注无法读取。"))?,
    )
    .map_err(|_| UiError::new("session_notes", "会话备注文件格式无效。"))?;
    validate(&items).map_err(|_| UiError::new("session_notes", "会话备注文件包含无效数据。"))?;
    Ok(items)
}

fn write(root: &Path, items: &[SessionNote]) -> Result<(), UiError> {
    validate(items).map_err(|_| UiError::new("session_notes", "会话备注字段无效。"))?;
    let bytes = serde_json::to_vec_pretty(items)
        .map_err(|_| UiError::new("session_notes", "会话备注无法编码。"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new("session_notes", "会话备注文件超过大小限制。"));
    }
    let temp = root.join(format!(
        "session-notes-{}-{}.tmp",
        stamp(),
        std::process::id()
    ));
    fs::write(&temp, bytes).map_err(|_| UiError::new("session_notes", "会话备注无法暂存。"))?;
    if let Err(error) = fs::rename(&temp, file(root)) {
        let _ = fs::remove_file(temp);
        return Err(UiError::new(
            "session_notes",
            &format!("会话备注无法保存：{error}"),
        ));
    }
    Ok(())
}

fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[tauri::command]
pub fn session_notes_list(
    profile: String,
    state: State<'_, Desktop>,
) -> Result<Vec<SessionNote>, UiError> {
    if profile.is_empty() || profile.len() > 4096 || profile.contains('\0') {
        return Err(UiError::new("session_notes", "profile 无效。"));
    }
    let _lock = state.session_notes_lock.lock().unwrap();
    Ok(read(&state.root)?
        .into_iter()
        .filter(|note| note.profile == profile)
        .collect())
}

#[tauri::command]
pub fn session_notes_save(mut note: SessionNote, state: State<'_, Desktop>) -> Result<(), UiError> {
    if note.content.is_empty() {
        if note.profile.is_empty()
            || note.profile.len() > 4096
            || note.profile.contains('\0')
            || note.session_id.is_empty()
            || note.session_id.len() > 256
            || note.session_id.contains('\0')
        {
            return Err(UiError::new("session_notes", "会话备注身份无效。"));
        }
    } else {
        validate(std::slice::from_ref(&note))
            .map_err(|_| UiError::new("session_notes", "备注为空或超过 4000 个字符。"))?;
        note.updated_at = stamp();
    }
    let _lock = state.session_notes_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    items.retain(|old| old.profile != note.profile || old.session_id != note.session_id);
    if !note.content.is_empty() {
        if items.len() >= MAX_NOTES {
            return Err(UiError::new(
                "session_notes_limit",
                "已达到 500 条会话备注上限。",
            ));
        }
        items.push(note);
    }
    write(&state.root, &items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(profile: &str, session_id: &str, content: &str) -> SessionNote {
        SessionNote {
            profile: profile.into(),
            session_id: session_id.into(),
            content: content.into(),
            updated_at: 1,
        }
    }

    #[test]
    fn validates_profile_identity_and_bounded_content() {
        assert!(validate(&[note("profile-a", "same-id", "Use the local test database.")]).is_ok());
        assert!(validate(&[note("profile-a", "same-id", "x".repeat(4001).as_str())]).is_err());
        assert!(validate(&[note("p", "s", " ")]).is_err());
        assert!(validate(&[
            note("profile-a", "same-id", "one"),
            note("profile-a", "same-id", "two")
        ])
        .is_err());
        assert!(validate(&[
            note("profile-a", "same-id", "one"),
            note("profile-b", "same-id", "two")
        ])
        .is_ok());
    }

    #[test]
    fn notes_round_trip_without_cross_profile_sharing() {
        let root =
            std::env::temp_dir().join(format!("hsp-notes-{}-{}", std::process::id(), stamp()));
        fs::create_dir(&root).unwrap();
        let items = vec![note(
            "profile-a",
            "same-id",
            "Remember the accepted constraints.",
        )];
        write(&root, &items).unwrap();
        assert_eq!(read(&root).unwrap(), items);
        let visible = read(&root)
            .unwrap()
            .into_iter()
            .filter(|item| item.profile == "profile-b")
            .collect::<Vec<_>>();
        assert!(visible.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
