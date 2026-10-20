//! User-authored prompt snippets stored in the tool's local data directory.
use crate::{Desktop, UiError};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use tauri::State;

const MAX_TEMPLATES: usize = 100;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub category: String,
    pub content: String,
    pub updated_at: u64,
}

fn file(root: &Path) -> std::path::PathBuf {
    root.join("prompt-templates.json")
}

pub fn validate(items: &[PromptTemplate]) -> Result<(), String> {
    if items.len() > MAX_TEMPLATES {
        return Err("提示词模板数量超过限制".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for item in items {
        if item.id.is_empty()
            || item.id.len() > 128
            || !item
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
            || !ids.insert(item.id.as_str())
            || item.name.trim() != item.name
            || item.name.is_empty()
            || item.name.chars().count() > 80
            || item.name.contains('\0')
            || item.category.trim() != item.category
            || item.category.chars().count() > 40
            || item.category.contains('\0')
            || item.content.trim().is_empty()
            || item.content.chars().count() > 12_000
            || item.content.contains('\0')
        {
            return Err("提示词模板字段无效或超过限制".into());
        }
    }
    Ok(())
}

fn read(root: &Path) -> Result<Vec<PromptTemplate>, UiError> {
    let path = file(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| UiError::new("prompt_templates", "提示词模板无法读取。"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FILE_BYTES {
        return Err(UiError::new(
            "prompt_templates",
            "提示词模板文件类型或大小不允许。",
        ));
    }
    let items: Vec<PromptTemplate> = serde_json::from_slice(
        &fs::read(path).map_err(|_| UiError::new("prompt_templates", "提示词模板无法读取。"))?,
    )
    .map_err(|_| UiError::new("prompt_templates", "提示词模板文件格式无效。"))?;
    validate(&items)
        .map_err(|_| UiError::new("prompt_templates", "提示词模板文件包含无效数据。"))?;
    Ok(items)
}

fn write(root: &Path, items: &[PromptTemplate]) -> Result<(), UiError> {
    validate(items).map_err(|_| UiError::new("prompt_templates", "提示词模板字段无效。"))?;
    let bytes = serde_json::to_vec_pretty(items)
        .map_err(|_| UiError::new("prompt_templates", "提示词模板无法编码。"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(UiError::new(
            "prompt_templates",
            "提示词模板文件超过大小限制。",
        ));
    }
    let temp = root.join(format!(
        "prompt-templates-{}-{}.tmp",
        stamp(),
        std::process::id()
    ));
    fs::write(&temp, bytes)
        .map_err(|_| UiError::new("prompt_templates", "提示词模板无法暂存。"))?;
    if let Err(error) = fs::rename(&temp, file(root)) {
        let _ = fs::remove_file(temp);
        return Err(UiError::new(
            "prompt_templates",
            &format!("提示词模板无法保存：{error}"),
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

fn new_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "template-{}-{}-{}",
        stamp(),
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[tauri::command]
pub fn prompt_template_list(state: State<'_, Desktop>) -> Result<Vec<PromptTemplate>, UiError> {
    let _lock = state.prompt_templates_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(items)
}

#[tauri::command]
pub fn prompt_template_save(
    mut template: PromptTemplate,
    state: State<'_, Desktop>,
) -> Result<PromptTemplate, UiError> {
    template.updated_at = stamp();
    if template.id.is_empty() {
        template.id = new_id();
    }
    validate(std::slice::from_ref(&template))
        .map_err(|_| UiError::new("prompt_templates", "名称、分类或内容无效或超过限制。"))?;
    let _lock = state.prompt_templates_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    if let Some(existing) = items.iter_mut().find(|item| item.id == template.id) {
        *existing = template.clone();
    } else {
        if items.len() >= MAX_TEMPLATES {
            return Err(UiError::new(
                "prompt_templates_limit",
                "提示词模板已达到 100 条上限。",
            ));
        }
        items.push(template.clone());
    }
    write(&state.root, &items)?;
    Ok(template)
}

#[tauri::command]
pub fn prompt_template_delete(id: String, state: State<'_, Desktop>) -> Result<(), UiError> {
    if id.is_empty() || id.len() > 128 {
        return Err(UiError::new("prompt_templates", "提示词模板 ID 无效。"));
    }
    let _lock = state.prompt_templates_lock.lock().unwrap();
    let mut items = read(&state.root)?;
    let before = items.len();
    items.retain(|item| item.id != id);
    if items.len() == before {
        return Err(UiError::new(
            "prompt_templates",
            "提示词模板已不存在，请刷新列表。",
        ));
    }
    write(&state.root, &items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(id: &str) -> PromptTemplate {
        PromptTemplate {
            id: id.into(),
            name: "Review changes".into(),
            category: "Review".into(),
            content: "Inspect the diff and run focused tests.".into(),
            updated_at: 1,
        }
    }

    #[test]
    fn validates_template_fields_and_limits() {
        assert!(validate(&[template("template-1")]).is_ok());
        assert!(validate(&[template("bad id")]).is_err());
        let mut empty = template("template-2");
        empty.content = "  ".into();
        assert!(validate(&[empty]).is_err());
        assert!(validate(&[template("same"), template("same")]).is_err());
        assert!(validate(&vec![template("x"); MAX_TEMPLATES + 1]).is_err());
    }

    #[test]
    fn templates_round_trip_through_local_file() {
        let root =
            std::env::temp_dir().join(format!("hsp-prompts-{}-{}", std::process::id(), stamp()));
        fs::create_dir(&root).unwrap();
        let items = vec![template("template-1")];
        write(&root, &items).unwrap();
        assert_eq!(read(&root).unwrap(), items);
        fs::remove_dir_all(root).unwrap();
    }
}
