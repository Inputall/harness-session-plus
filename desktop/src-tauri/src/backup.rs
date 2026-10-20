//! Tool-owned files only. Never opens or mutates Harness storage.
use crate::{theme_keeper::SavedTheme, Desktop, Settings, UiError};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

const LIMIT: usize = 192 * 1024 * 1024;
const ZIP_LIMIT: usize = 64 * 1024 * 1024;
type Files = BTreeMap<String, Vec<u8>>;
type Result<T> = std::result::Result<T, String>;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    size: usize,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    tool_version: String,
    created_at: u64,
    includes_records: bool,
    files: Vec<Entry>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn allowed(name: &str) -> bool {
    matches!(
        name,
        "settings.json"
            | "imported-themes.json"
            | "community-themes.json"
            | "saved-appearance.json"
            | "session-checkpoints.json"
            | "session-favorites.json"
            | "recycle.sqlite3"
    ) || name
        .strip_prefix("community-themes/")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(|id| {
            id.starts_with("community-")
                && id.len() <= 100
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}
fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "文件不可读取")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("不接受重解析文件".into());
        }
    }
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > limit as u64 {
        return Err("文件类型或大小不允许".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "文件不可读取")?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "文件读取失败")?;
    if bytes.len() > limit {
        return Err("文件超过限制".into());
    }
    Ok(bytes)
}
fn directory(path: &Path) -> Result<()> {
    if path.exists() {
        let m = fs::symlink_metadata(path).map_err(|_| "目录不可读取")?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("不接受重定向目录".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes() & 0x400 != 0 {
                return Err("不接受重解析目录".into());
            }
        }
    } else {
        fs::create_dir(path).map_err(|_| "目录创建失败")?;
    }
    Ok(())
}
fn validate_db(path: &Path) -> Result<()> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "回收站快照无效")?;
    db.pragma_update(None, "trusted_schema", false)
        .map_err(|_| "无法检查快照")?;
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|_| "快照校验失败")?;
    if integrity != "ok" {
        return Err("回收站快照损坏".into());
    }
    let unexpected: i64 = db.query_row("SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND NOT (type='table' AND name IN ('trash','operations'))", [], |r|r.get(0)).map_err(|_| "快照结构无效")?;
    if unexpected != 0 {
        return Err("快照含未知表、视图或触发器".into());
    }
    for (table, expected) in [
        ("trash", "CREATE TABLE trash(profile TEXT NOT NULL,session_id TEXT NOT NULL,title TEXT NOT NULL,workspace_id TEXT NOT NULL,workspace_title TEXT NOT NULL,moved_at INTEGER NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',PRIMARY KEY(profile,session_id))"),
        ("operations", "CREATE TABLE operations(id INTEGER PRIMARY KEY,profile TEXT NOT NULL,session_id TEXT NOT NULL,action TEXT NOT NULL,created_at INTEGER NOT NULL,state TEXT NOT NULL)")
    ] {
        let sql:String=db.query_row("SELECT sql FROM sqlite_schema WHERE name=?1",[table],|r|r.get(0)).map_err(|_|"快照缺少必要表")?;
        let normalize=|s:&str|s.chars().filter(|c|!c.is_ascii_whitespace()).collect::<String>().to_ascii_lowercase();
        if normalize(&sql)!=normalize(expected){return Err("数据库表定义不兼容".into());}
    }
    for (table, expected) in [
        (
            "trash",
            "profile,session_id,title,workspace_id,workspace_title,moved_at,state,error",
        ),
        (
            "operations",
            "id,profile,session_id,action,created_at,state",
        ),
    ] {
        let mut statement = db
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|_| "快照结构无效")?;
        let names = statement
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|_| "快照结构无效")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| "快照结构无效")?;
        if names.join(",") != expected {
            return Err("快照结构不兼容".into());
        }
    }
    Ok(())
}
fn validate(files: &Files, root: &Path) -> Result<()> {
    let mut total = 0usize;
    for (name, bytes) in files {
        total = total.checked_add(bytes.len()).ok_or("备份过大")?;
        if !allowed(name) || total > LIMIT {
            return Err("备份文件或总大小不允许".into());
        }
        if name == "recycle.sqlite3" {
            let scratch = root.join(format!("backup-check-{}.sqlite3", stamp()));
            write_new(&scratch, bytes)?;
            let result = validate_db(&scratch);
            let _ = fs::remove_file(&scratch);
            result?;
            continue;
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| "配置 JSON 无效")?;
        match name.as_str() {
            "settings.json" => {
                let settings: Settings =
                    serde_json::from_value(value).map_err(|_| "连接配置无效")?;
                settings.validate().map_err(|_| "连接配置无效")?;
            }
            "saved-appearance.json" => {
                let s: SavedTheme = serde_json::from_value(value).map_err(|_| "保存的皮肤无效")?;
                s.validate()?;
            }
            "session-checkpoints.json" => {
                let entries: Vec<crate::checkpoints::Checkpoint> =
                    serde_json::from_value(value).map_err(|_| "会话检查点文件无效")?;
                crate::checkpoints::validate(&entries)?;
            }
            "session-favorites.json" => {
                let entries: Vec<crate::favorites::Favorite> =
                    serde_json::from_value(value).map_err(|_| "会话收藏文件无效")?;
                crate::favorites::validate(&entries)?;
            }
            "imported-themes.json" | "community-themes.json" => {
                let array = value.as_array().ok_or("主题库无效")?;
                if array.len()
                    > if name == "imported-themes.json" {
                        20
                    } else {
                        200
                    }
                {
                    return Err("主题数量超过限制".into());
                }
                let prefix = if name == "imported-themes.json" {
                    "imported-"
                } else {
                    "community-"
                };
                let mut ids = std::collections::BTreeSet::new();
                for item in array {
                    hsp_core::theme_import::validate(item)?;
                    let id = item["id"].as_str().ok_or("主题 ID 缺失")?;
                    if !id.starts_with(prefix) || !ids.insert(id) {
                        return Err("主题 ID 无效或重复".into());
                    }
                    if prefix == "community-" {
                        let bytes = files
                            .get(&format!("community-themes/{id}.json"))
                            .ok_or("社区主题文件缺失")?;
                        let full: Value =
                            serde_json::from_slice(bytes).map_err(|_| "社区主题无效")?;
                        if full["id"] != item["id"] {
                            return Err("社区主题身份不一致".into());
                        }
                    }
                }
            }
            _ => {
                hsp_core::theme_import::validate(&value)?;
                if *name
                    != format!(
                        "community-themes/{}.json",
                        value["id"].as_str().unwrap_or_default()
                    )
                {
                    return Err("社区主题文件名不一致".into());
                }
            }
        }
    }
    Ok(())
}
fn collect(root: &Path, records: bool, private_paths: bool) -> Result<Files> {
    let mut files = Files::new();
    for name in [
        "settings.json",
        "imported-themes.json",
        "community-themes.json",
        "saved-appearance.json",
        "session-checkpoints.json",
        "session-favorites.json",
    ] {
        if root.join(name).exists() {
            files.insert(name.into(), read_limited(&root.join(name), LIMIT)?);
        }
    }
    if root.join("active-theme.json").exists() {
        files.insert(
            "saved-appearance.json".into(),
            read_limited(&root.join("active-theme.json"), LIMIT)?,
        );
    }
    if let Some(bytes) = files.get_mut("settings.json") {
        let mut value: Value = serde_json::from_slice(bytes).map_err(|_| "配置损坏")?;
        if !private_paths {
            value["target_id"] = Value::Null;
            value["executable"] = json!("");
        }
        *bytes = serde_json::to_vec(&value).map_err(|_| "配置编码失败")?;
    }
    let dir = root.join("community-themes");
    if dir.exists() {
        directory(&dir)?;
        for item in fs::read_dir(&dir).map_err(|_| "社区主题目录不可读")? {
            let item = item.map_err(|_| "主题不可读")?;
            let name = format!("community-themes/{}", item.file_name().to_string_lossy());
            if allowed(&name) {
                files.insert(name, read_limited(&item.path(), LIMIT)?);
            }
            if files.len() > 206 || files.values().map(Vec::len).sum::<usize>() > LIMIT {
                return Err("主题备份超过限制".into());
            }
        }
    }
    if records && root.join("recycle.sqlite3").exists() {
        let snapshot = root.join(format!("backup-snapshot-{}.sqlite3", stamp()));
        let result: Result<()> = (|| {
            let source = Connection::open_with_flags(
                root.join("recycle.sqlite3"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|_| "无法读取回收站")?;
            let mut dest = Connection::open(&snapshot).map_err(|_| "快照创建失败")?;
            let backup =
                rusqlite::backup::Backup::new(&source, &mut dest).map_err(|_| "快照创建失败")?;
            let page_size: usize = source
                .pragma_query_value(None, "page_size", |r| r.get(0))
                .map_err(|_| "页大小读取失败")?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                if std::time::Instant::now() > deadline {
                    return Err("数据库忙，快照超时，请稍后重试".into());
                }
                if matches!(
                    backup.step(64).map_err(|_| "快照未完成")?,
                    rusqlite::backup::StepResult::Done
                ) {
                    break;
                }
                if backup.progress().pagecount as usize * page_size > LIMIT {
                    return Err("数据库超过备份限制".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            drop(backup);
            drop(dest);
            files.insert("recycle.sqlite3".into(), read_limited(&snapshot, LIMIT)?);
            Ok(())
        })();
        let _ = fs::remove_file(&snapshot);
        result?;
    }
    validate(&files, root)?;
    Ok(files)
}
fn pack(files: &Files) -> Result<Vec<u8>> {
    let manifest = Manifest {
        format: 1,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        created_at: stamp(),
        includes_records: files.contains_key("recycle.sqlite3"),
        files: files
            .iter()
            .map(|(path, b)| Entry {
                path: path.clone(),
                size: b.len(),
                sha256: digest(b),
            })
            .collect(),
    };
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        zip.start_file(name, options).map_err(|_| "备份打包失败")?;
        zip.write_all(bytes).map_err(|_| "备份写入失败")?;
    }
    zip.start_file("manifest.json", options)
        .map_err(|_| "清单写入失败")?;
    zip.write_all(&serde_json::to_vec(&manifest).map_err(|_| "清单编码失败")?)
        .map_err(|_| "清单写入失败")?;
    let bytes = zip.finish().map_err(|_| "备份打包失败")?.into_inner();
    if bytes.len() > ZIP_LIMIT {
        return Err("压缩备份超过 64 MiB".into());
    }
    Ok(bytes)
}
fn unpack(bytes: &[u8], root: &Path) -> Result<(Manifest, Files)> {
    if bytes.len() > ZIP_LIMIT {
        return Err("备份超过 64 MiB".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "不是有效 ZIP 备份")?;
    if zip.len() > 208 {
        return Err("备份文件数量超限".into());
    }
    let mut entries = Files::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut item = zip.by_index(i).map_err(|_| "ZIP 文件损坏")?;
        let name = item.name().to_owned();
        if (name != "manifest.json" && !allowed(&name))
            || entries.contains_key(&name)
            || item
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("备份含非法路径、链接或重复文件".into());
        }
        let mut b = Vec::new();
        (&mut item)
            .take((LIMIT - total + 1) as u64)
            .read_to_end(&mut b)
            .map_err(|_| "备份解压失败")?;
        total += b.len();
        if total > LIMIT {
            return Err("解压后超过 192 MiB".into());
        }
        entries.insert(name, b);
    }
    let manifest: Manifest =
        serde_json::from_slice(&entries.remove("manifest.json").ok_or("缺少备份清单")?)
            .map_err(|_| "备份清单无效")?;
    if manifest.format != 1
        || manifest.tool_version != env!("CARGO_PKG_VERSION")
        || manifest.files.len() != entries.len()
        || manifest.includes_records != entries.contains_key("recycle.sqlite3")
    {
        return Err("备份版本或清单不兼容".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for item in &manifest.files {
        let b = entries.get(&item.path).ok_or("清单文件缺失")?;
        if !seen.insert(&item.path) || b.len() != item.size || digest(b) != item.sha256 {
            return Err("备份 SHA256 或文件大小不匹配".into());
        }
    }
    validate(&entries, root)?;
    Ok((manifest, entries))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| "目标已存在或不可写")?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
        return Err("写入失败".into());
    }
    Ok(())
}
fn save_export(root: &Path, prefix: &str, bytes: &[u8], extension: &str) -> Result<Value> {
    let dir = root.join("backups");
    directory(&dir)?;
    let path = dir.join(format!("{prefix}-{}.{extension}", stamp()));
    write_new(&path, bytes)?;
    let hash = digest(bytes);
    write_new(
        &path.with_extension(format!("{extension}.sha256")),
        format!("{hash}  {}\n", path.file_name().unwrap().to_string_lossy()).as_bytes(),
    )?;
    Ok(json!({"path":path.to_string_lossy(),"sha256":hash,"size":bytes.len()}))
}
fn ui(error: String) -> UiError {
    UiError::new("backup", &error)
}
#[tauri::command]
pub fn backup_open_folder(state: tauri::State<'_, Desktop>) -> std::result::Result<(), UiError> {
    let path = state.root.join("backups");
    directory(&path).map_err(ui)?;
    std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|_| ui("无法打开备份目录".into()))?;
    Ok(())
}

#[tauri::command]
pub async fn backup_create(
    include_records: bool,
    include_paths: bool,
    state: tauri::State<'_, Desktop>,
) -> std::result::Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成"))?;
    let root = state.root.clone();
    tokio::task::spawn_blocking(move || {
        let files = collect(&root, include_records, include_paths)?;
        let bytes = pack(&files)?;
        save_export(&root, "hsp-backup", &bytes, "zip")
    })
    .await
    .map_err(|_| ui("备份任务失败".into()))?
    .map_err(ui)
}
fn decode(encoded: &str) -> Result<Vec<u8>> {
    if encoded.len() > ZIP_LIMIT * 4 / 3 + 8 {
        return Err("备份超过 64 MiB".into());
    }
    STANDARD.decode(encoded).map_err(|_| "备份编码无效".into())
}
#[tauri::command]
pub async fn backup_preview(
    archive_base64: String,
    state: tauri::State<'_, Desktop>,
) -> std::result::Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成"))?;
    let root = state.root.clone();
    tokio::task::spawn_blocking(move || {let bytes=decode(&archive_base64)?;let (m,files)=unpack(&bytes,&root)?;
        let items: Vec<_> = m.files.iter().map(|f|json!({"path":f.path,"size":f.size,"change":if root.join(&f.path).exists(){"覆盖"}else{"新增"}})).collect();
        let profiles = if files.contains_key("recycle.sqlite3") {"含原 profile 记录，只在原身份下可用，不能恢复 Harness 会话本体"} else {"不含回收站记录，保留当前记录"};
        Ok(json!({"sha256":digest(&bytes),"version":m.tool_version,"createdAt":m.created_at,"files":items,"records":profiles,"warning":"恢复会覆盖清单内的工具文件；连接页面 ID 会清除，皮肤仅加入可选主题。增强偏好与关注清单不恢复。"}))
    }).await.map_err(|_|ui("预览任务失败".into()))?.map_err(ui)
}
#[tauri::command]
pub async fn backup_stage(
    archive_base64: String,
    expected_sha256: String,
    state: tauri::State<'_, Desktop>,
) -> std::result::Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成"))?;
    if state.injection_owner.lock().unwrap().is_some()
        || state.root.join("active-theme.json").exists()
    {
        return Err(ui(
            "请先卸载页面增强并恢复 Harness 默认皮肤，再恢复备份".into()
        ));
    }
    let root = state.root.clone();
    tokio::task::spawn_blocking(move || {let bytes=decode(&archive_base64)?;if digest(&bytes)!=expected_sha256{return Err("备份已变化，请重新预览".into());}unpack(&bytes,&root)?;
        if root.join("pending-restore.zip").exists(){return Err("已有待恢复任务，请先取消".into());}
        let temporary=root.join(format!("pending-{}.tmp",stamp()));write_new(&temporary,&bytes)?;
        fs::rename(&temporary,root.join("pending-restore.zip")).map_err(|_|"无法安排恢复")?;
        Ok(json!({"message":"已安排恢复。请从托盘完全退出工具后重新打开；Harness 保持运行。下次启动前会自动备份现有工具文件。"}))
    }).await.map_err(|_|ui("恢复任务失败".into()))?.map_err(ui)
}
#[tauri::command]
pub async fn backup_cancel(state: tauri::State<'_, Desktop>) -> std::result::Result<(), UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成"))?;
    match fs::remove_file(state.root.join("pending-restore.zip")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ui("取消恢复失败".into())),
    }
}
#[tauri::command]
pub fn backup_status(state: tauri::State<'_, Desktop>) -> Value {
    let message = fs::read(state.root.join("restore-status.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<String>(&b).ok())
        .unwrap_or_default();
    json!({"message":message,"pending":state.root.join("pending-restore.zip").exists()})
}
#[tauri::command]
pub async fn backup_diagnostics(
    state: tauri::State<'_, Desktop>,
) -> std::result::Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成"))?;
    // Deliberately construct a whitelist, never include raw errors, configuration values or report payloads.
    let value = json!({"format":1,"toolVersion":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"createdAt":stamp(),"settingsPresent":state.root.join("settings.json").is_file(),"recycleDatabasePresent":state.root.join("recycle.sqlite3").is_file(),"savedThemePresent":state.root.join("active-theme.json").is_file(),"pendingRestore":state.root.join("pending-restore.zip").is_file(),"recycleWorkerRegistered":state.recycle_cancel.lock().unwrap().is_some(),"sessionWorkerRegistered":state.session_cancel.lock().unwrap().is_some(),"scope":"本工具状态摘要，不代表 Harness 实时连接检查；不含正文、标题、路径、profile、令牌或原始日志"});
    save_export(
        &state.root,
        "hsp-diagnostics",
        &serde_json::to_vec_pretty(&value).map_err(|_| ui("编码失败".into()))?,
        "json",
    )
    .map_err(ui)
}

// Crash recovery uses an on-disk journal written before any replacement. A failed or
// interrupted restore always rolls back, never replays the pending archive.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    names: Vec<String>,
    original: Vec<String>,
}
fn transaction_path(name: &str) -> bool {
    allowed(name) || name == "recycle-preference.json"
}
fn replace(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    if !transaction_path(name) {
        return Err("非法恢复路径".into());
    }
    directory(root)?;
    if name.starts_with("community-themes/") {
        directory(&root.join("community-themes"))?;
    }
    let target = root.join(name);
    if target.exists() {
        read_limited(&target, LIMIT)?;
    }
    let temp = target.with_extension(format!("{}-restore-tmp", stamp()));
    write_new(&temp, bytes)?;
    fs::rename(&temp, target).map_err(|_| "配置替换失败".into())
}
fn rollback(root: &Path, folder: &Path, journal: &Journal) -> Result<()> {
    directory(folder)?;
    if journal.names.len() > 208 || journal.original.iter().any(|n| !journal.names.contains(n)) {
        return Err("回滚清单无效".into());
    }
    for name in &journal.names {
        if !transaction_path(name) {
            return Err("回滚清单无效".into());
        }
        if name.starts_with("community-themes/") {
            directory(&folder.join("community-themes"))?;
        }
        if journal.original.contains(name) {
            replace(root, name, &read_limited(&folder.join(name), LIMIT)?)?;
        } else if root.join(name).exists() {
            fs::remove_file(root.join(name)).map_err(|_| "回滚失败")?;
        }
    }
    Ok(())
}
pub fn recover(root: &Path) -> Result<String> {
    let journal_path = root.join("restore-journal.json");
    let rollback_dir = root.join("restore-rollback");
    if !journal_path.exists() && !root.join("pending-restore.zip").exists() {
        return Ok(String::new());
    }
    let operation_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("theme-operation.lock"))
        .map_err(|_| "无法锁定配置")?;
    operation_lock
        .try_lock()
        .map_err(|_| "主题操作进行中，请稍后重启")?;
    let keeper_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("theme-keeper.lock"))
        .map_err(|_| "无法锁定守护")?;
    keeper_lock
        .try_lock()
        .map_err(|_| "主题守护仍在退出，请稍后重启")?;
    if journal_path.exists() {
        let journal: Journal = serde_json::from_slice(&read_limited(&journal_path, 64 * 1024)?)
            .map_err(|_| "恢复日志损坏，请保留配置目录并人工检查")?;
        rollback(root, &rollback_dir, &journal)?;
        fs::remove_file(&journal_path).map_err(|_| "无法完成回滚")?;
        return Ok("上次恢复中断，已回滚到恢复前的工具配置；原备份仍在 backups 目录。".into());
    }
    let pending = root.join("pending-restore.zip");
    if !pending.exists() {
        return Ok(String::new());
    }
    if root.join("active-theme.json").exists() {
        return Err("存在启用中的主题，请取消待恢复或先恢复默认皮肤".into());
    }
    let bytes = read_limited(&pending, ZIP_LIMIT)?;
    let (_, mut files) = unpack(&bytes, root)?;
    // The previous preference must not replay a page mutation after migration.
    files.insert(
        "recycle-preference.json".into(),
        br#"{"enabled":false}"#.to_vec(),
    );
    if let Some(settings) = files.get_mut("settings.json") {
        let mut s: Settings = serde_json::from_slice(settings).map_err(|_| "连接配置无效")?;
        s.target_id = None;
        if s.executable.is_empty() {
            s.executable = Settings::default().executable;
        }
        *settings = serde_json::to_vec(&s).map_err(|_| "配置编码失败")?;
    }
    // Keep an independent, user-restorable snapshot before replacing any tool data.
    let mut originals = collect(root, true, true)?;
    let safety = pack(&originals)?;
    save_export(root, "before-restore", &safety, "zip")?;
    if root.join("recycle-preference.json").exists() {
        originals.insert(
            "recycle-preference.json".into(),
            read_limited(&root.join("recycle-preference.json"), 65536)?,
        );
    }
    directory(&rollback_dir)?;
    let mut journal = Journal {
        names: files.keys().cloned().collect(),
        original: vec![],
    };
    for name in &journal.names {
        if root.join(name).exists() {
            replace(
                &rollback_dir,
                name,
                originals.get(name).ok_or("恢复前文件缺失")?,
            )?;
            journal.original.push(name.clone());
        }
    }
    // Checkpoint before replacing SQLite, so stale WAL pages cannot shadow restored data.
    if files.contains_key("recycle.sqlite3") && root.join("recycle.sqlite3").exists() {
        let db =
            Connection::open(root.join("recycle.sqlite3")).map_err(|_| "数据库无法准备恢复")?;
        let busy: i64 = db
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .map_err(|_| "数据库正在使用")?;
        if busy != 0 {
            return Err("数据库仍在使用，拒绝恢复".into());
        }
    }
    let consumed = root.join(format!("backups/restore-source-{}.zip", stamp()));
    fs::rename(&pending, consumed).map_err(|_| "无法安排恢复事务")?;
    write_new(
        &journal_path,
        &serde_json::to_vec(&journal).map_err(|_| "日志编码失败")?,
    )?;
    let result: Result<()> = (|| {
        for (name, data) in &files {
            replace(root, name, data)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        rollback(root, &rollback_dir, &journal)?;
        fs::remove_file(&journal_path).map_err(|_| "无法完成回滚")?;
        return Ok(format!("恢复失败，已回滚：{error}"));
    }
    fs::remove_file(&journal_path).map_err(|_| "恢复完成但日志未清除，下次启动将回滚")?;
    Ok("备份恢复完成。请重新核对连接；恢复的皮肤可在皮肤中心选择后手动应用。回收站记录仍按原 profile 隔离。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "hsp-backup-test-{}-{}-{id}",
                std::process::id(),
                stamp()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let parent = std::env::temp_dir().canonicalize().unwrap();
            if let Ok(path) = self.0.canonicalize() {
                assert!(path.starts_with(parent));
                assert!(path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("hsp-backup-test-"));
                let _ = fs::remove_dir_all(path);
            }
        }
    }
    fn settings(port: u16) -> Vec<u8> {
        serde_json::to_vec(&Settings {
            port,
            target_id: Some("stale-page".into()),
            executable: "C:\\private\\Harness.exe".into(),
            theme: "dark".into(),
        })
        .unwrap()
    }
    fn db(root: &Path) -> Connection {
        let db = Connection::open(root.join("recycle.sqlite3")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE trash(profile TEXT NOT NULL,session_id TEXT NOT NULL,title TEXT NOT NULL,workspace_id TEXT NOT NULL,workspace_title TEXT NOT NULL,moved_at INTEGER NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',PRIMARY KEY(profile,session_id)); CREATE TABLE operations(id INTEGER PRIMARY KEY,profile TEXT NOT NULL,session_id TEXT NOT NULL,action TEXT NOT NULL,created_at INTEGER NOT NULL,state TEXT NOT NULL);").unwrap();
        db.execute("INSERT INTO trash VALUES ('original-profile','session-1','test-title','workspace','workspace',1,'archived','')",[]).unwrap();
        db
    }
    #[test]
    fn snapshots_include_committed_wal_and_preserve_identity() {
        let source = Temp::new();
        let target = Temp::new();
        let db = db(&source.0);
        assert!(source.0.join("recycle.sqlite3-wal").exists());
        fs::write(source.0.join("settings.json"), settings(9223)).unwrap();
        let files = collect(&source.0, true, false).unwrap();
        let s: Value = serde_json::from_slice(&files["settings.json"]).unwrap();
        assert!(s["target_id"].is_null());
        assert_eq!(s["executable"], "");
        let archive = pack(&files).unwrap();
        let (manifest, restored) = unpack(&archive, &target.0).unwrap();
        assert!(manifest.includes_records);
        fs::write(
            target.0.join("recycle.sqlite3"),
            &restored["recycle.sqlite3"],
        )
        .unwrap();
        let read = Connection::open(target.0.join("recycle.sqlite3")).unwrap();
        let identity: (String, String) = read
            .query_row("SELECT profile,session_id FROM trash", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(identity, ("original-profile".into(), "session-1".into()));
        drop(db);
    }
    #[test]
    fn default_archive_round_trips_user_saved_checkpoints() {
        let source = Temp::new();
        let target = Temp::new();
        let checkpoint = crate::checkpoints::Checkpoint {
            id: "cp-123-0".into(),
            profile: "profile-a".into(),
            session_id: "session-a".into(),
            title: "Task".into(),
            workspace: "Workspace".into(),
            cwd: "D:/work".into(),
            created_at: 123,
            prompt: "continue task".into(),
        };
        fs::write(
            source.0.join("session-checkpoints.json"),
            serde_json::to_vec(&vec![checkpoint.clone()]).unwrap(),
        )
        .unwrap();
        let files = collect(&source.0, false, false).unwrap();
        assert!(files.contains_key("session-checkpoints.json"));
        let (_, restored) = unpack(&pack(&files).unwrap(), &target.0).unwrap();
        let entries: Vec<crate::checkpoints::Checkpoint> =
            serde_json::from_slice(&restored["session-checkpoints.json"]).unwrap();
        assert_eq!(entries, vec![checkpoint]);
        assert!(!restored.contains_key("session.json"));
        assert!(!restored.contains_key("project-files.zip"));
    }

    #[test]
    fn default_archive_round_trips_profile_scoped_favorites() {
        let source = Temp::new();
        let target = Temp::new();
        let favorite = crate::favorites::Favorite {
            profile: "profile-a".into(),
            session_id: "session-a".into(),
            title: "Task".into(),
            workspace: "Workspace".into(),
            updated_at: 123,
        };
        fs::write(
            source.0.join("session-favorites.json"),
            serde_json::to_vec(&vec![favorite.clone()]).unwrap(),
        )
        .unwrap();
        let files = collect(&source.0, false, false).unwrap();
        assert!(files.contains_key("session-favorites.json"));
        let (_, restored) = unpack(&pack(&files).unwrap(), &target.0).unwrap();
        let entries: Vec<crate::favorites::Favorite> =
            serde_json::from_slice(&restored["session-favorites.json"]).unwrap();
        assert_eq!(entries, vec![favorite]);
    }
    #[test]
    fn restore_keeps_unlisted_records_and_disables_automatic_enhancement() {
        let target = Temp::new();
        let db = db(&target.0);
        drop(db);
        fs::write(target.0.join("settings.json"), settings(9223)).unwrap();
        fs::write(
            target.0.join("recycle-preference.json"),
            br#"{"enabled":true}"#,
        )
        .unwrap();
        let files = Files::from([("settings.json".into(), settings(9555))]);
        fs::write(target.0.join("pending-restore.zip"), pack(&files).unwrap()).unwrap();
        assert!(recover(&target.0).unwrap().contains("恢复完成"));
        let s: Settings =
            serde_json::from_slice(&fs::read(target.0.join("settings.json")).unwrap()).unwrap();
        assert_eq!(s.port, 9555);
        assert!(s.target_id.is_none());
        assert_eq!(
            fs::read(target.0.join("recycle-preference.json")).unwrap(),
            br#"{"enabled":false}"#
        );
        let db = Connection::open(target.0.join("recycle.sqlite3")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM trash", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(!target.0.join("active-theme.json").exists());
        assert!(!target.0.join("pending-restore.zip").exists());
        assert!(fs::read_dir(target.0.join("backups")).unwrap().any(|f| f
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("before-restore-")));
    }
    #[test]
    fn replace_database_checkpoints_old_wal() {
        let source = Temp::new();
        let target = Temp::new();
        let old = db(&target.0);
        drop(old);
        let new = db(&source.0);
        new.execute("UPDATE trash SET session_id='new-id'", [])
            .unwrap();
        let files = collect(&source.0, true, false).unwrap();
        drop(new);
        fs::write(target.0.join("pending-restore.zip"), pack(&files).unwrap()).unwrap();
        recover(&target.0).unwrap();
        let read = Connection::open(target.0.join("recycle.sqlite3")).unwrap();
        assert_eq!(
            read.query_row("SELECT session_id FROM trash", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "new-id"
        );
    }
    #[test]
    fn interrupted_transaction_rolls_back_existing_and_new_files() {
        let target = Temp::new();
        let folder = target.0.join("restore-rollback");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("settings.json"), settings(9223)).unwrap();
        fs::write(target.0.join("settings.json"), settings(9999)).unwrap();
        fs::write(target.0.join("imported-themes.json"), b"[]").unwrap();
        let journal = Journal {
            names: vec!["settings.json".into(), "imported-themes.json".into()],
            original: vec!["settings.json".into()],
        };
        fs::write(
            target.0.join("restore-journal.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        assert!(recover(&target.0).unwrap().contains("已回滚"));
        assert_eq!(
            fs::read(target.0.join("settings.json")).unwrap(),
            settings(9223)
        );
        assert!(!target.0.join("imported-themes.json").exists());
    }
    #[test]
    fn failed_rollback_preserves_journal_for_next_startup() {
        let target = Temp::new();
        fs::create_dir(target.0.join("restore-rollback")).unwrap();
        let journal = Journal {
            names: vec!["settings.json".into()],
            original: vec!["settings.json".into()],
        };
        fs::write(
            target.0.join("restore-journal.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        assert!(recover(&target.0).is_err());
        assert!(target.0.join("restore-journal.json").exists());
    }
    #[test]
    fn invalid_backup_does_not_modify_current_settings() {
        let target = Temp::new();
        fs::write(target.0.join("settings.json"), settings(9223)).unwrap();
        fs::write(target.0.join("pending-restore.zip"), b"broken").unwrap();
        assert!(recover(&target.0).is_err());
        assert_eq!(
            fs::read(target.0.join("settings.json")).unwrap(),
            settings(9223)
        );
        assert!(target.0.join("pending-restore.zip").exists());
    }
    #[test]
    fn rejects_unknown_paths_modified_hashes_and_wrong_versions() {
        let target = Temp::new();
        let files = Files::from([("../settings.json".into(), settings(9223))]);
        assert!(unpack(&pack(&files).unwrap(), &target.0).is_err());
        let build = |version: &str, hash: &str| {
            let bytes = settings(9223);
            let m = Manifest {
                format: 1,
                tool_version: version.into(),
                created_at: 0,
                includes_records: false,
                files: vec![Entry {
                    path: "settings.json".into(),
                    size: bytes.len(),
                    sha256: hash.into(),
                }],
            };
            let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("settings.json", o).unwrap();
            z.write_all(&bytes).unwrap();
            z.start_file("manifest.json", o).unwrap();
            z.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
            z.finish().unwrap().into_inner()
        };
        assert!(unpack(&build(env!("CARGO_PKG_VERSION"), "wrong"), &target.0).is_err());
        assert!(unpack(&build("0.0.0", &digest(&settings(9223))), &target.0).is_err());
    }
    #[test]
    fn rejects_triggers_and_keeps_saved_skin_inactive() {
        let source = Temp::new();
        let db = db(&source.0);
        db.execute_batch(
            "CREATE TRIGGER bad AFTER DELETE ON trash BEGIN DELETE FROM operations; END;",
        )
        .unwrap();
        assert!(collect(&source.0, true, false).is_err());
        drop(db);
        let mut skin = hsp_core::skin::select("midnight").unwrap();
        skin["persistenceRevision"] = json!("backup-test");
        let saved = SavedTheme {
            options: hsp_core::management::Options {
                port: 9223,
                target_id: None,
            },
            skin,
            revision: "backup-test".into(),
        };
        fs::write(
            source.0.join("active-theme.json"),
            serde_json::to_vec(&saved).unwrap(),
        )
        .unwrap();
        let files = collect(&source.0, false, false).unwrap();
        assert!(files.contains_key("saved-appearance.json"));
        assert!(!files.contains_key("active-theme.json"));
        let target = Temp::new();
        fs::write(target.0.join("pending-restore.zip"), pack(&files).unwrap()).unwrap();
        recover(&target.0).unwrap();
        assert!(target.0.join("saved-appearance.json").exists());
        assert!(!target.0.join("active-theme.json").exists());
    }
}
