//! User-owned session tags stored outside Harness data.
use crate::{Desktop, UiError};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const MAX_SESSIONS: usize = 500;
const MAX_TAGS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SessionTags {
    pub profile: String,
    pub session_id: String,
    pub tags: Vec<String>,
}

fn file(root: &Path) -> std::path::PathBuf {
    root.join("session-tags.json")
}

pub fn validate(items: &[SessionTags]) -> Result<(), String> {
    if items.len() > MAX_SESSIONS {
        return Err("标签会话数量超过限制".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for item in items {
        if item.profile.is_empty()
            || item.profile.len() > 4096
            || item.profile.contains('\0')
            || item.session_id.is_empty()
            || item.session_id.len() > 256
            || item.session_id.contains('\0')
            || item.tags.len() > MAX_TAGS
            || !keys.insert((item.profile.as_str(), item.session_id.as_str()))
        {
            return Err("标签记录无效或重复".into());
        }
        let mut tags = std::collections::BTreeSet::new();
        for tag in &item.tags {
            if tag.trim() != tag
                || tag.is_empty()
                || tag.chars().count() > 32
                || tag.contains('\0')
                || !tags.insert(tag.to_lowercase())
            {
                return Err("标签为空、重复或超过 32 个字符".into());
            }
        }
    }
    Ok(())
}

fn read(root: &Path) -> Result<Vec<SessionTags>, UiError> {
    let path = file(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let meta = fs::symlink_metadata(&path)
        .map_err(|_| UiError::new("session_tags", "会话标签无法读取。"))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 2 * 1024 * 1024 {
        return Err(UiError::new(
            "session_tags",
            "会话标签文件类型或大小不允许。",
        ));
    }
    let items: Vec<SessionTags> = serde_json::from_slice(
        &fs::read(path).map_err(|_| UiError::new("session_tags", "会话标签无法读取。"))?,
    )
    .map_err(|_| UiError::new("session_tags", "会话标签文件格式无效。"))?;
    validate(&items).map_err(|_| UiError::new("session_tags", "会话标签文件包含无效数据。"))?;
    Ok(items)
}

fn for_profile(items: Vec<SessionTags>, profile: &str) -> Vec<SessionTags> {
    items
        .into_iter()
        .filter(|item| item.profile == profile)
        .collect()
}

fn write(root: &Path, items: &[SessionTags]) -> Result<(), UiError> {
    validate(items).map_err(|_| UiError::new("session_tags", "标签字段无效或超过限制。"))?;
    let temp = root.join(format!(
        "session-tags-{}-{}.tmp",
        stamp(),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(items)
        .map_err(|_| UiError::new("session_tags", "会话标签无法编码。"))?;
    fs::write(&temp, bytes).map_err(|_| UiError::new("session_tags", "会话标签无法持久化。"))?;
    if let Err(error) = fs::rename(&temp, file(root)) {
        let _ = fs::remove_file(temp);
        return Err(UiError::new(
            "session_tags",
            &format!("会话标签无法提交：{error}"),
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
pub fn session_tags_list(
    profile: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Vec<SessionTags>, UiError> {
    if profile.is_empty() || profile.len() > 4096 || profile.contains('\0') {
        return Err(UiError::new("session_tags", "profile 无效。"));
    }
    let _lock = state.session_tags_lock.lock().unwrap();
    Ok(for_profile(read(&state.root)?, &profile))
}

#[tauri::command]
pub fn session_tags_save(
    item: SessionTags,
    state: tauri::State<'_, Desktop>,
) -> Result<(), UiError> {
    validate(std::slice::from_ref(&item))
        .map_err(|_| UiError::new("session_tags", "标签为空、重复或超过限制。"))?;
    let _lock = state.session_tags_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    items.retain(|old| old.profile != item.profile || old.session_id != item.session_id);
    if !item.tags.is_empty() {
        if items.len() >= MAX_SESSIONS {
            return Err(UiError::new(
                "session_tags_limit",
                "已达到 500 个会话标签上限。",
            ));
        }
        items.push(item);
    }
    write(&state.root, &items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounded_unique_tags() {
        let item = SessionTags {
            profile: "p".into(),
            session_id: "s".into(),
            tags: vec!["项目A".into(), "待跟进".into()],
        };
        assert!(validate(std::slice::from_ref(&item)).is_ok());
        let duplicate = SessionTags {
            tags: vec!["Build".into(), "build".into()],
            ..item.clone()
        };
        assert!(validate(&[duplicate]).is_err());
        let oversized = SessionTags {
            tags: vec!["x".repeat(33)],
            ..item
        };
        assert!(validate(&[oversized]).is_err());
    }

    #[test]
    fn session_tags_round_trip_and_clear() {
        let root =
            std::env::temp_dir().join(format!("hsp-tags-{}-{}", std::process::id(), stamp()));
        fs::create_dir(&root).unwrap();
        let tags = SessionTags {
            profile: "p".into(),
            session_id: "id".into(),
            tags: vec!["bug".into()],
        };
        write(&root, std::slice::from_ref(&tags)).unwrap();
        assert_eq!(read(&root).unwrap(), vec![tags.clone()]);
        write(&root, &[]).unwrap();
        assert!(read(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_filter_never_exposes_another_profiles_tags() {
        let item = SessionTags {
            profile: "profile-a".into(),
            session_id: "same-id".into(),
            tags: vec!["private".into()],
        };
        assert!(for_profile(vec![item], "profile-b").is_empty());
    }
}
