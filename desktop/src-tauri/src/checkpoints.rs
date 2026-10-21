//! User-created session checkpoints. These are tool-owned metadata snapshots.
//! They never copy or mutate Harness storage or project files.
use crate::{recycle_error, Desktop, UiError};
use hsp_core::{recycle, sessions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const MAX_CHECKPOINTS: usize = 100;
const MAX_TEXT: usize = 512 * 1024;
const MAX_FILE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub id: String,
    pub profile: String,
    pub session_id: String,
    pub title: String,
    pub workspace: String,
    pub cwd: String,
    pub created_at: u64,
    pub prompt: String,
    #[serde(default)]
    pub git: Option<crate::workspace::GitCheckpoint>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    pub profile: String,
    pub session_id: String,
    pub title: String,
    pub workspace: String,
    pub cwd: String,
    pub prompt: String,
}

fn path(root: &Path) -> std::path::PathBuf {
    root.join("session-checkpoints.json")
}
fn read(root: &Path) -> Result<Vec<Checkpoint>, UiError> {
    let file = path(root);
    if !file.exists() {
        return Ok(Vec::new());
    }
    let metadata =
        fs::symlink_metadata(&file).map_err(|_| UiError::new("checkpoint", "检查点无法读取。"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FILE as u64
    {
        return Err(UiError::new("checkpoint", "检查点文件超过大小限制。"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(UiError::new("checkpoint", "拒绝读取重解析文件。"));
        }
    }
    let bytes = fs::read(file).map_err(|_| UiError::new("checkpoint", "检查点无法读取。"))?;
    let entries: Vec<Checkpoint> = serde_json::from_slice(&bytes)
        .map_err(|_| UiError::new("checkpoint", "检查点文件格式无效。"))?;
    validate(&entries).map_err(|_| UiError::new("checkpoint", "检查点文件包含无效数据。"))?;
    Ok(entries)
}
pub fn validate(entries: &[Checkpoint]) -> Result<(), String> {
    if entries.len() > MAX_CHECKPOINTS {
        return Err("检查点数量超过限制".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    if entries.iter().any(|item| {
        item.id.len() > 100
            || !item.id.starts_with("cp-")
            || !item
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !ids.insert(item.id.as_str())
            || !text(&item.profile, 4096)
            || !text(&item.session_id, 256)
            || item.title.len() > 2000
            || item.workspace.len() > 2000
            || item.cwd.len() > 4096
            || !text(&item.prompt, MAX_TEXT)
            || item.git.as_ref().is_some_and(|git| {
                git.branch.len() > 2000
                    || git.changed_files > 1_000_000
                    || git.files.len() > 10_000
                    || git.files.iter().any(|file| {
                        file.status.len() > 100 || file.path.is_empty() || file.path.len() > 4096
                    })
            })
    }) {
        return Err("检查点字段无效或超过限制".into());
    }
    Ok(())
}
fn write(root: &Path, entries: &[Checkpoint]) -> Result<(), UiError> {
    validate(entries).map_err(|_| UiError::new("checkpoint", "检查点字段无效或超过限制。"))?;
    let temp = root.join(format!(
        "session-checkpoints-{}-{}.tmp",
        stamp(),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(entries)
        .map_err(|_| UiError::new("checkpoint", "检查点无法编码。"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|_| UiError::new("checkpoint", "无法创建检查点暂存文件。"))?;
    use std::io::Write;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| UiError::new("checkpoint", "检查点无法持久化。"))?;
    drop(file);
    if let Err(error) = fs::rename(&temp, path(root)) {
        let _ = fs::remove_file(&temp);
        return Err(UiError::new(
            "checkpoint",
            &format!("检查点无法提交：{error}"),
        ));
    }
    Ok(())
}
fn text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}
fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
async fn verify_session(
    state: &Desktop,
    profile: &str,
    session_id: &str,
    title: &str,
    workspace: &str,
    cwd: &str,
) -> Result<recycle::Identity, UiError> {
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    if identity.profile != profile {
        return Err(UiError::new(
            "profile",
            "Harness profile 已改变，请刷新会话后重试。",
        ));
    }
    let result = sessions::query(&settings.options(), &sessions::Request::List)
        .await
        .map_err(recycle_error)?;
    let session = result["sessions"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["sessionId"] == session_id))
        .ok_or_else(|| UiError::new("not_found", "检查点对应的会话已不存在。"))?;
    if session["archived"] == true {
        return Err(UiError::new(
            "archived",
            "检查点对应的会话已归档，请先恢复会话。",
        ));
    }
    if session["title"].as_str().unwrap_or_default() != title
        || session["workspace"].as_str().unwrap_or_default() != workspace
        || session["cwd"].as_str().unwrap_or_default() != cwd
    {
        return Err(UiError::new(
            "identity",
            "会话标题或工作区已改变，拒绝使用旧检查点。",
        ));
    }
    Ok(identity)
}

#[tauri::command]
pub fn checkpoint_list(state: tauri::State<'_, Desktop>) -> Result<Vec<Checkpoint>, UiError> {
    let _lock = state.checkpoint_lock.lock().unwrap();
    read(&state.root)
}

#[tauri::command]
pub async fn checkpoint_create(
    input: CreateInput,
    state: tauri::State<'_, Desktop>,
) -> Result<Checkpoint, UiError> {
    if !text(&input.profile, 4096)
        || !text(&input.session_id, 256)
        || input.title.len() > 2000
        || input.workspace.len() > 2000
        || input.cwd.len() > 4096
        || !text(&input.prompt, MAX_TEXT)
    {
        return Err(UiError::new("checkpoint", "检查点内容为空或超过大小限制。"));
    }
    let _gate = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    verify_session(
        &state,
        &input.profile,
        &input.session_id,
        &input.title,
        &input.workspace,
        &input.cwd,
    )
    .await?;
    let git = crate::workspace::git_checkpoint(Path::new(&input.cwd)).await;
    let _lock = state.checkpoint_lock.lock().unwrap();
    let mut entries = read(&state.root)?;
    if entries.len() >= MAX_CHECKPOINTS {
        return Err(UiError::new(
            "checkpoint_limit",
            "已达到 100 个检查点上限，请先手动删除不再需要的检查点。",
        ));
    }
    let created_at = stamp();
    let mut suffix = 0u16;
    let id = loop {
        let candidate = format!("cp-{created_at}-{suffix}");
        if entries.iter().all(|item| item.id != candidate) {
            break candidate;
        }
        suffix = suffix.saturating_add(1);
    };
    let checkpoint = Checkpoint {
        id,
        profile: input.profile,
        session_id: input.session_id,
        title: input.title,
        workspace: input.workspace,
        cwd: input.cwd,
        created_at,
        prompt: input.prompt,
        git,
    };
    entries.retain(|item| item.id != checkpoint.id);
    entries.insert(0, checkpoint.clone());
    write(&state.root, &entries)?;
    Ok(checkpoint)
}

#[tauri::command]
pub async fn checkpoint_restore(
    id: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    if !text(&id, 100) {
        return Err(UiError::new("checkpoint", "检查点 ID 无效。"));
    }
    let _gate = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let entries = {
        let _lock = state.checkpoint_lock.lock().unwrap();
        read(&state.root)?
    };
    let checkpoint = entries
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| UiError::new("not_found", "检查点不存在。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = verify_session(
        &state,
        &checkpoint.profile,
        &checkpoint.session_id,
        &checkpoint.title,
        &checkpoint.workspace,
        &checkpoint.cwd,
    )
    .await?;
    let current = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    if current != identity {
        return Err(UiError::new(
            "profile",
            "Harness 实例在检查点核验期间发生变化，未打开会话。",
        ));
    }
    sessions::query(
        &settings.options(),
        &sessions::Request::Open {
            session_id: checkpoint.session_id.clone(),
        },
    )
    .await
    .map_err(recycle_error)?;
    super::session_center::foreground_for_checkpoint(identity.main_pid);
    Ok(
        json!({"id":checkpoint.id,"sessionId":checkpoint.session_id,"title":checkpoint.title,"workspace":checkpoint.workspace,"cwd":checkpoint.cwd,"createdAt":checkpoint.created_at,"prompt":checkpoint.prompt,"git":checkpoint.git}),
    )
}

#[tauri::command]
pub fn checkpoint_delete(id: String, state: tauri::State<'_, Desktop>) -> Result<(), UiError> {
    if !text(&id, 100) {
        return Err(UiError::new("checkpoint", "检查点 ID 无效。"));
    }
    let _lock = state.checkpoint_lock.lock().unwrap();
    let mut entries = read(&state.root)?;
    let length = entries.len();
    entries.retain(|item| item.id != id);
    if entries.len() == length {
        return Err(UiError::new("not_found", "检查点不存在。"));
    }
    write(&state.root, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_text_rejects_empty_and_nul() {
        assert!(!text("", 10));
        assert!(!text("a\0b", 10));
        assert!(!text("1234", 3));
        assert!(text("abc", 3));
    }

    #[test]
    fn checkpoint_records_round_trip_and_reject_duplicates_or_oversized_files() {
        let checkpoint = Checkpoint {
            id: "cp-123-0".into(),
            profile: "profile-a".into(),
            session_id: "session-a".into(),
            title: "Task".into(),
            workspace: "Workspace".into(),
            cwd: "D:/work".into(),
            created_at: 123,
            prompt: "continue this task".into(),
            git: None,
        };
        validate(std::slice::from_ref(&checkpoint)).unwrap();
        let duplicate = vec![checkpoint.clone(), checkpoint.clone()];
        assert!(validate(&duplicate).is_err());

        let root = std::env::temp_dir().join(format!("hsp-checkpoint-test-{}", stamp()));
        fs::create_dir(&root).unwrap();
        write(&root, std::slice::from_ref(&checkpoint)).unwrap();
        assert_eq!(read(&root).unwrap(), vec![checkpoint]);
        let mut updated = read(&root).unwrap();
        updated[0].prompt = "updated prompt".into();
        write(&root, &updated).unwrap();
        assert_eq!(read(&root).unwrap()[0].prompt, "updated prompt");
        fs::write(path(&root), vec![b' '; MAX_FILE + 1]).unwrap();
        assert!(read(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
