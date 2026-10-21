//! Tool-owned session bookmarks. They store identity metadata only.
use crate::{Desktop, UiError};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const MAX_FAVORITES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    pub profile: String,
    pub session_id: String,
    pub title: String,
    pub workspace: String,
    pub updated_at: u64,
}

fn path(root: &Path) -> std::path::PathBuf {
    root.join("session-favorites.json")
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}
pub fn validate(items: &[Favorite]) -> Result<(), String> {
    if items.len() > MAX_FAVORITES {
        return Err("收藏数量超过限制".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for item in items {
        if !valid_text(&item.profile, 4096)
            || !valid_text(&item.session_id, 256)
            || item.title.len() > 2000
            || item.workspace.len() > 2000
            || !keys.insert((item.profile.as_str(), item.session_id.as_str()))
        {
            return Err("收藏字段无效或重复".into());
        }
    }
    Ok(())
}
fn read(root: &Path) -> Result<Vec<Favorite>, UiError> {
    let file = path(root);
    if !file.exists() {
        return Ok(Vec::new());
    }
    let metadata =
        fs::symlink_metadata(&file).map_err(|_| UiError::new("favorites", "收藏无法读取。"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 2 * 1024 * 1024
    {
        return Err(UiError::new("favorites", "收藏文件类型或大小不允许。"));
    }
    let items: Vec<Favorite> = serde_json::from_slice(
        &fs::read(file).map_err(|_| UiError::new("favorites", "收藏无法读取。"))?,
    )
    .map_err(|_| UiError::new("favorites", "收藏文件格式无效。"))?;
    validate(&items).map_err(|_| UiError::new("favorites", "收藏文件包含无效数据。"))?;
    Ok(items)
}
fn write(root: &Path, items: &[Favorite]) -> Result<(), UiError> {
    validate(items).map_err(|_| UiError::new("favorites", "收藏字段无效或超过限制。"))?;
    let temp = root.join(format!(
        "session-favorites-{}-{}.tmp",
        stamp(),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(items)
        .map_err(|_| UiError::new("favorites", "收藏无法编码。"))?;
    fs::write(&temp, bytes).map_err(|_| UiError::new("favorites", "收藏无法持久化。"))?;
    if let Err(error) = fs::rename(&temp, path(root)) {
        let _ = fs::remove_file(temp);
        return Err(UiError::new("favorites", &format!("收藏无法提交：{error}")));
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
pub fn favorite_list(
    profile: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Vec<Favorite>, UiError> {
    if !valid_text(&profile, 4096) {
        return Err(UiError::new("favorites", "profile 无效。"));
    }
    let _lock = state.favorites_lock.lock().unwrap();
    Ok(read(&state.root)?
        .into_iter()
        .filter(|item| item.profile == profile)
        .collect())
}

#[tauri::command]
pub fn favorite_toggle(
    favorite: Favorite,
    state: tauri::State<'_, Desktop>,
) -> Result<bool, UiError> {
    let _lock = state.favorites_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    let key = (&favorite.profile, &favorite.session_id);
    if let Some(index) = items
        .iter()
        .position(|item| (&item.profile, &item.session_id) == key)
    {
        items.remove(index);
        write(&state.root, &items)?;
        return Ok(false);
    }
    if !valid_text(&favorite.profile, 4096)
        || !valid_text(&favorite.session_id, 256)
        || favorite.title.len() > 2000
        || favorite.workspace.len() > 2000
    {
        return Err(UiError::new("favorites", "收藏字段无效或超过限制。"));
    }
    if items.len() >= MAX_FAVORITES {
        return Err(UiError::new(
            "favorites_limit",
            "已达到 200 个收藏上限，请先取消不需要的收藏。",
        ));
    }
    items.push(favorite);
    write(&state.root, &items)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_profile_and_id() {
        let item = Favorite {
            profile: "p".into(),
            session_id: "s".into(),
            title: "T".into(),
            workspace: "W".into(),
            updated_at: 1,
        };
        assert!(validate(&[item.clone(), item]).is_err());
    }

    #[test]
    fn local_favorites_round_trip_without_sharing_profiles() {
        let root =
            std::env::temp_dir().join(format!("hsp-favorites-{}-{}", std::process::id(), stamp()));
        fs::create_dir(&root).unwrap();
        let item = Favorite {
            profile: "profile-a".into(),
            session_id: "real-id".into(),
            title: "Task".into(),
            workspace: "Workspace".into(),
            updated_at: 10,
        };
        write(&root, std::slice::from_ref(&item)).unwrap();
        assert_eq!(read(&root).unwrap(), vec![item.clone()]);
        let loaded = read(&root).unwrap();
        assert!(loaded
            .iter()
            .find(|entry| entry.profile == "profile-b")
            .is_none());
        write(&root, &[]).unwrap();
        assert!(read(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
