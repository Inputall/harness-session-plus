#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod backup;
mod checkpoints;
mod session_center;
mod theme_keeper;
mod workspace;

use hsp_core::{
    cdp::Error,
    discovery,
    management::{self, Action, Options},
    recycle, skin,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, sync::Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    port: u16,
    target_id: Option<String>,
    executable: String,
    theme: String,
}
impl Default for Settings {
    fn default() -> Self {
        let executable = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| {
                path.join("Programs/DeepSeek Harness/DeepSeek Harness.exe")
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        Self {
            port: 9223,
            target_id: None,
            executable,
            theme: "dark".into(),
        }
    }
}
impl Settings {
    fn options(&self) -> Options {
        Options {
            port: self.port,
            target_id: self.target_id.clone(),
        }
    }
    fn validate(&self) -> Result<(), UiError> {
        if self.port == 0
            || self
                .target_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 256)
            || self.executable.len() > 4096
            || !matches!(self.theme.as_str(), "dark" | "light")
        {
            return Err(UiError::new(
                "invalid_settings",
                "配置无效，请检查端口、页面选择和主题。",
            ));
        }
        Ok(())
    }
}
struct Desktop {
    _instance_lock: std::fs::File,
    settings: Mutex<Settings>,
    gate: tokio::sync::Mutex<()>,
    report: Mutex<Option<Value>>,
    injection_owner: Mutex<Option<Options>>,
    skin_owner: Mutex<Option<Options>>,
    recycle_cancel: Mutex<Option<CancellationToken>>,
    session_cancel: Mutex<Option<CancellationToken>>,
    checkpoint_lock: Mutex<()>,
    #[cfg(windows)]
    session_toasts: Mutex<Vec<windows::UI::Notifications::ToastNotification>>,
    root: PathBuf,
}
#[derive(Debug, Clone, Serialize)]
struct UiError {
    code: String,
    message: String,
    target_ids: Vec<String>,
}
impl UiError {
    fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            target_ids: vec![],
        }
    }
}
impl From<Error> for UiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Ambiguous(ids) => Self {
                code: "select_target".into(),
                message: "发现多个 Harness 页面，请在连接设置中明确选择。".into(),
                target_ids: ids,
            },
            Error::Discovery => Self::new(
                "discovery",
                "无法连接调试服务。请确认 Harness 已使用本机调试参数启动，且端口正确。",
            ),
            Error::TargetNotFound => Self::new(
                "target_not_found",
                "未找到匹配的 Harness 页面，请刷新页面列表并重新选择。",
            ),
            Error::Disconnected => Self::new(
                "disconnected",
                "连接已断开。请重新检查连接；操作未自动重放。",
            ),
            Error::Timeout => Self::new("timeout", "操作超时，结果尚未确认。请检查连接。"),
            Error::Cancelled => Self::new("cancelled", "操作已取消。"),
            Error::InvalidEndpoint => {
                Self::new("endpoint", "调试地址不符合配置的本机访问范围，已拒绝连接。")
            }
            Error::InvalidResult => Self::new(
                "invalid_result",
                "页面上下文已变化或脚本执行失败，请重新检查。",
            ),
            Error::Protocol(_) => Self::new("protocol", "当前页面的调试协议请求失败。"),
        }
    }
}
#[derive(Serialize)]
struct TargetChoice {
    id: String,
}

#[tauri::command]
fn load_settings(state: tauri::State<'_, Desktop>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
async fn save_settings(
    settings: Settings,
    state: tauri::State<'_, Desktop>,
) -> Result<Settings, UiError> {
    settings.validate()?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    if state.injection_owner.lock().unwrap().is_some()
        || state.skin_owner.lock().unwrap().is_some()
        || state.root.join("active-theme.json").exists()
    {
        return Err(UiError::new(
            "injection_active",
            "请先卸载页面增强并恢复 Harness 默认皮肤，再更改配置。",
        ));
    }
    let data = serde_json::to_vec_pretty(&settings)
        .map_err(|_| UiError::new("settings", "配置序列化失败。"))?;
    std::fs::write(state.root.join("settings.tmp"), data)
        .and_then(|_| {
            std::fs::rename(
                state.root.join("settings.tmp"),
                state.root.join("settings.json"),
            )
        })
        .map_err(|_| UiError::new("settings", "配置保存失败，请检查应用数据目录权限。"))?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn discover_targets(state: tauri::State<'_, Desktop>) -> Result<Vec<TargetChoice>, UiError> {
    let port = state.settings.lock().unwrap().port;
    let (_, targets) = discovery::discover(port).await?;
    Ok(targets
        .into_iter()
        .filter(|t| t.kind == "page" && t.url == "dsh-app://app/")
        .map(|t| TargetChoice { id: t.id })
        .collect())
}

async fn perform(app: &tauri::AppHandle, action: Action) -> Result<Value, UiError> {
    let state = app.state::<Desktop>();
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let mut options = if matches!(action, Action::RemoveInjection) {
        state
            .injection_owner
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| state.settings.lock().unwrap().options())
    } else {
        state.settings.lock().unwrap().options()
    };
    if matches!(action, Action::InjectTest) && state.recycle_cancel.lock().unwrap().is_some() {
        return Err(UiError::new(
            "recycle_active",
            "回收站增强已安装。请先卸载增强，再安装测试按钮。",
        ));
    }
    if matches!(action, Action::InjectTest) {
        let (_, targets) = discovery::discover(options.port).await?;
        let target = discovery::select(&targets, options.target_id.as_deref())?;
        options.target_id = Some(target.id);
        // Track the explicit page before evaluation: a timeout may follow a successful install.
        *state.injection_owner.lock().unwrap() = Some(options.clone());
    }
    let result = management::execute(&options, action, &CancellationToken::new())
        .await
        .map_err(UiError::from);
    if let Ok(value) = &result {
        if matches!(action, Action::InjectTest) {
            *state.injection_owner.lock().unwrap() = Some(options);
        }
        if matches!(action, Action::RemoveInjection) {
            if let Some(cancel) = state.recycle_cancel.lock().unwrap().take() {
                cancel.cancel();
            }
            let _ = std::fs::remove_file(state.root.join("recycle-preference.json"));
            *state.injection_owner.lock().unwrap() = None;
        }
        if matches!(action, Action::ProbeCapabilities) {
            *state.report.lock().unwrap() = Some(value.clone());
        }
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(if result.is_ok() {
            "Harness Session Plus · 最近检查成功"
        } else {
            "Harness Session Plus · 连接需要检查"
        }));
    }
    result
}

fn recycle_error(message: String) -> UiError {
    UiError::new("recycle", &message)
}

#[tauri::command]
fn skin_catalog() -> Value {
    skin::catalog()
}

fn all_skins(state: &Desktop) -> Result<Value, UiError> {
    let mut catalog = skin::catalog();
    let recovered = state.root.join("saved-appearance.json");
    if recovered.exists() {
        let saved: theme_keeper::SavedTheme = serde_json::from_slice(
            &std::fs::read(recovered).map_err(|_| UiError::new("theme", "恢复皮肤不可读取"))?,
        )
        .map_err(|_| UiError::new("theme", "恢复皮肤格式无效"))?;
        saved.validate().map_err(|e| UiError::new("theme", &e))?;
        let mut skin = saved.skin;
        skin["id"] = serde_json::json!("backup-appearance");
        skin["category"] = serde_json::json!("imported");
        skin["name"] = serde_json::json!("备份恢复的皮肤");
        catalog.as_array_mut().unwrap().push(skin);
    }
    let path = state.root.join("imported-themes.json");
    if path.exists() {
        let bytes = std::fs::read(path)
            .map_err(|_| UiError::new("theme", "无法读取本地主题库，请检查文件权限。"))?;
        let imported: Vec<Value> = serde_json::from_slice(&bytes)
            .map_err(|_| UiError::new("theme", "本地主题库格式损坏，请备份后检查文件。"))?;
        if imported.len() > 20 {
            return Err(UiError::new("theme", "本地主题库超过 20 个主题。"));
        }
        for s in imported {
            hsp_core::theme_import::validate(&s).map_err(|e| UiError::new("theme", &e))?;
            if !s["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("imported-"))
            {
                return Err(UiError::new("theme", "本地主题 ID 无效。"));
            }
            catalog.as_array_mut().unwrap().push(s);
        }
    }
    let path = state.root.join("community-themes.json");
    if path.exists() {
        let bytes =
            std::fs::read(path).map_err(|_| UiError::new("theme", "社区主题记录读取失败。"))?;
        let installed: Vec<Value> = serde_json::from_slice(&bytes)
            .map_err(|_| UiError::new("theme", "社区主题记录损坏。"))?;
        for s in installed {
            hsp_core::theme_import::validate(&s).map_err(|e| UiError::new("theme", &e))?;
            if !s["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("community-"))
            {
                return Err(UiError::new("theme", "社区主题 ID 无效。"));
            }
            catalog.as_array_mut().unwrap().push(s);
        }
    }
    Ok(catalog)
}
#[tauri::command]
fn theme_library(state: tauri::State<'_, Desktop>) -> Result<Value, UiError> {
    all_skins(&state)
}
#[tauri::command]
async fn community_page(offset: usize) -> Result<Value, UiError> {
    hsp_core::theme_community::page(offset)
        .await
        .map_err(|e| UiError::new("theme", &e))
}
#[tauri::command]
async fn community_install(
    version_id: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let theme = hsp_core::theme_community::install(&version_id)
        .await
        .map_err(|e| UiError::new("theme", &e))?;
    let id = theme["id"].as_str().unwrap();
    let mut summaries: Vec<Value> = all_skins(&state)?
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("community-"))
        })
        .cloned()
        .collect();
    let directory = state.root.join("community-themes");
    std::fs::create_dir_all(&directory).map_err(|_| UiError::new("theme", "主题目录创建失败。"))?;
    save_theme_json(&directory.join(format!("{id}.json")), &theme)?;
    let mut summary = theme.clone();
    summary.as_object_mut().unwrap().remove("wallpaper");
    summary.as_object_mut().unwrap().remove("upstreamManifest");
    summary["category"] = serde_json::json!("community");
    summaries.retain(|s| s["id"] != theme["id"]);
    summaries.push(summary.clone());
    save_theme_json(
        &state.root.join("community-themes.json"),
        &serde_json::json!(summaries),
    )?;
    Ok(summary)
}
fn save_theme_json(path: &std::path::Path, value: &Value) -> Result<(), UiError> {
    let temp = path.with_extension("tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec(value).map_err(|_| UiError::new("theme", "主题无法序列化。"))?,
    )
    .and_then(|_| std::fs::rename(temp, path))
    .map_err(|_| UiError::new("theme", "主题保存失败。"))
}
#[tauri::command]
async fn import_theme(
    config: String,
    image_base64: Option<String>,
    author: String,
    license: String,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    use base64::Engine;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    if image_base64
        .as_ref()
        .is_some_and(|s| s.len() > hsp_core::theme_import::IMAGE_LIMIT * 4 / 3 + 8)
    {
        return Err(UiError::new("theme", "图片超过 5 MiB。"));
    }
    let image = image_base64
        .map(|s| base64::engine::general_purpose::STANDARD.decode(s))
        .transpose()
        .map_err(|_| UiError::new("theme", "图片编码无效。"))?;
    let theme = hsp_core::theme_import::convert(&config, image.as_deref(), &author, &license)
        .map_err(|e| UiError::new("theme", &e))?;
    let mut library: Vec<Value> = all_skins(&state)?
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            v["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("imported-"))
        })
        .cloned()
        .collect();
    library.retain(|s| s["id"] != theme["id"]);
    if library.len() >= 20 {
        return Err(UiError::new("theme", "最多保存 20 个导入主题。"));
    }
    library.push(theme.clone());
    let path = state.root.join("imported-themes.json");
    let temp = state.root.join("imported-themes.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec(&library).map_err(|_| UiError::new("theme", "主题无法序列化。"))?,
    )
    .and_then(|_| std::fs::rename(temp, path))
    .map_err(|_| UiError::new("theme", "主题保存失败。"))?;
    Ok(theme)
}

#[tauri::command]
async fn skin_action(
    action: String,
    skin_id: Option<String>,
    wallpaper_clarity: Option<u8>,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    // Serialize reset/apply with the independent keeper to prevent stale reapplication.
    let operation_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.root.join("theme-operation.lock"))
        .map_err(|_| UiError::new("theme", "无法锁定主题配置。"))?;
    operation_file
        .try_lock()
        .map_err(|_| UiError::new("busy", "主题正在恢复，请稍后重试。"))?;
    let previous = state.skin_owner.lock().unwrap().clone();
    let mut options = previous
        .clone()
        .unwrap_or_else(|| state.settings.lock().unwrap().options());
    let selected = if action == "apply" {
        let id = skin_id
            .as_deref()
            .ok_or_else(|| UiError::new("skin", "请选择皮肤。"))?;
        let summary = all_skins(&state)?
            .as_array()
            .and_then(|v| v.iter().find(|s| s["id"].as_str() == Some(id)))
            .cloned()
            .ok_or_else(|| UiError::new("skin", "请选择有效的皮肤。"))?;
        let mut full = if id.starts_with("community-") {
            let bytes = std::fs::read(
                state
                    .root
                    .join("community-themes")
                    .join(format!("{id}.json")),
            )
            .map_err(|_| UiError::new("theme", "社区主题文件缺失，请重新安装。"))?;
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| UiError::new("theme", "社区主题文件损坏。"))?;
            if value["id"] != id {
                return Err(UiError::new("theme", "社区主题身份不一致。"));
            }
            value
        } else {
            skin::select(id).unwrap_or(summary)
        };
        if let Some(clarity) = wallpaper_clarity {
            if clarity > 100 {
                return Err(UiError::new("skin", "背景清晰度须在 0–100 之间。"));
            }
            full["wallpaperClarity"] = serde_json::json!(clarity);
        }
        full["persistenceRevision"] = serde_json::json!(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        Some(full)
    } else {
        None
    };
    if action == "apply" {
        let (_, targets) = discovery::discover(options.port).await?;
        let target = match discovery::select(&targets, options.target_id.as_deref()) {
            Err(Error::TargetNotFound) if previous.is_some() => discovery::select(&targets, None)?,
            other => other?,
        };
        options.target_id = Some(target.id);
        // Track the explicit target before evaluation, including uncertain outcomes.
        *state.skin_owner.lock().unwrap() = Some(options.clone());
    }
    if action == "reset" {
        match std::fs::remove_file(state.root.join("active-theme.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UiError::new("theme", "无法停止主题自动恢复。")),
        }
        // Preserve an explicit live target; only rebind a stale one if unique.
        let (_, targets) = discovery::discover(options.port).await?;
        if matches!(
            discovery::select(&targets, options.target_id.as_deref()),
            Err(Error::TargetNotFound)
        ) {
            options.target_id = Some(discovery::select(&targets, None)?.id);
        }
    }
    let mut result = skin::execute_with_skin(&options, &action, selected.clone()).await?;
    if action == "apply" {
        let full = selected.unwrap();
        let saved = theme_keeper::SavedTheme {
            options: options.clone(),
            revision: full["persistenceRevision"].as_str().unwrap().into(),
            skin: full,
        };
        saved.validate().map_err(|e| UiError::new("theme", &e))?;
        let path = state.root.join("active-theme.json");
        let temp = state.root.join("active-theme.tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec(&saved).map_err(|_| UiError::new("theme", "主题保存失败。"))?,
        )
        .and_then(|_| std::fs::rename(temp, path))
        .map_err(|_| UiError::new("theme", "主题已应用，但自动恢复配置保存失败。"))?;
        theme_keeper::spawn(&state.root)
            .map_err(|_| UiError::new("theme", "主题已保存，但后台主题守护启动失败。"))?;
        result["persistent"] = serde_json::json!(true);
    }
    if action == "reset" {
        *state.skin_owner.lock().unwrap() = None;
    }
    Ok(result)
}

#[tauri::command]
async fn install_recycle(app: tauri::AppHandle) -> Result<Value, UiError> {
    let state = app.state::<Desktop>();
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    let mut options = settings.options();
    let (_, targets) = discovery::discover(options.port).await?;
    options.target_id = Some(discovery::select(&targets, options.target_id.as_deref())?.id);
    let client = recycle::connect(&options).await.map_err(recycle_error)?;
    let records = recycle::reconcile(
        &client,
        &state.root.join("recycle.sqlite3"),
        &identity.profile,
    )
    .await
    .map_err(recycle_error)?;
    if let Some(cancel) = state.recycle_cancel.lock().unwrap().take() {
        cancel.cancel();
    }
    *state.injection_owner.lock().unwrap() = Some(options.clone());
    let status =
        management::evaluate_harness(&client, recycle::INJECTION, &CancellationToken::new())
            .await?;
    // Remember the user's explicit choice so a later connection can restore only this
    // page enhancement. The marker never contains session data or an operation request.
    std::fs::write(
        state.root.join("recycle-preference.json"),
        br#"{"enabled":true}"#,
    )
    .map_err(|_| UiError::new("recycle", "增强已安装，但保持配置保存失败。"))?;
    let cancel = CancellationToken::new();
    *state.recycle_cancel.lock().unwrap() = Some(cancel.clone());
    let root = state.root.clone();
    let worker_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {_ = cancel.cancelled()=>break,_=interval.tick()=>{}}
            let requests=match management::evaluate_harness(&client,"window.__HARNESS_SESSION_PLUS__?.version === '0.2.0' ? window.__HARNESS_SESSION_PLUS__.drain() : null",&cancel).await{Ok(Value::Array(v))=>v,_=>break};
            for request in requests {
                if cancel.is_cancelled() {
                    break;
                }
                let request_id = match request["requestId"].as_str() {
                    Some(v) if v.len() <= 64 => v,
                    _ => continue,
                };
                let result = if request["action"] == "open" {
                    show(&worker_app);
                    let _ = worker_app.emit("open-trash", ());
                    Ok(())
                } else if request["action"] == "archive" {
                    let state = worker_app.state::<Desktop>();
                    let _guard = state.gate.lock().await;
                    if cancel.is_cancelled() {
                        break;
                    }
                    match recycle::identify(settings.port, settings.executable.clone()).await {
                        Ok(current) if current == identity => match request["sessionId"].as_str() {
                            Some(id) => {
                                recycle::change(
                                    &client,
                                    &root.join("recycle.sqlite3"),
                                    &identity,
                                    id,
                                    false,
                                )
                                .await
                            }
                            None => Err("会话 ID 缺失。".into()),
                        },
                        _ => Err("Harness 实例已改变，请重新安装增强。".into()),
                    }
                } else {
                    Err("未支持的动作。".into())
                };
                let error = result.err();
                let script = format!(
                    "window.__HARNESS_SESSION_PLUS__?.complete({}, {})",
                    serde_json::json!(request_id),
                    serde_json::json!(error)
                );
                if management::evaluate_harness(&client, &script, &cancel)
                    .await
                    .is_err()
                {
                    break;
                }
                let _ = worker_app.emit("trash-changed", ());
            }
        }
        client.close();
        let _ = worker_app.emit("recycle-disconnected", ());
    });
    Ok(
        serde_json::json!({"injection":status,"records":records,"adapter":"0.2.0-rc.2","identityVerified":true}),
    )
}

#[tauri::command]
fn recycle_restore_preference(state: tauri::State<'_, Desktop>) -> bool {
    let path = state.root.join("recycle-preference.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value["enabled"].as_bool())
        .unwrap_or(false)
}

#[tauri::command]
async fn refresh_trash(state: tauri::State<'_, Desktop>) -> Result<Vec<recycle::Record>, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    let client = recycle::connect(&settings.options())
        .await
        .map_err(recycle_error)?;
    recycle::reconcile(
        &client,
        &state.root.join("recycle.sqlite3"),
        &identity.profile,
    )
    .await
    .map_err(recycle_error)
}

#[tauri::command]
async fn restore_sessions(
    session_ids: Vec<String>,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    if session_ids.is_empty() || session_ids.len() > 100 {
        return Err(UiError::new("selection", "请选择 1 到 100 项。"));
    }
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    let client = recycle::connect(&settings.options())
        .await
        .map_err(recycle_error)?;
    let mut outcomes = vec![];
    for id in session_ids {
        let current = recycle::identify(settings.port, settings.executable.clone())
            .await
            .map_err(recycle_error)?;
        if current != identity {
            return Err(UiError::new(
                "identity",
                "Harness 实例已改变，批量恢复已停止。",
            ));
        }
        let result = recycle::change(
            &client,
            &state.root.join("recycle.sqlite3"),
            &identity,
            &id,
            true,
        )
        .await;
        let uncertain = result.as_ref().err().is_some_and(|e| e.contains("未确认"));
        outcomes.push(serde_json::json!({"sessionId":id,"ok":result.is_ok(),"error":result.err()}));
        if uncertain {
            break;
        }
    }
    Ok(serde_json::json!({"outcomes":outcomes}))
}

#[tauri::command]
async fn check_permanent_delete(
    session_ids: Vec<String>,
    state: tauri::State<'_, Desktop>,
) -> Result<recycle::DeletePreflight, UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    let client = recycle::connect(&settings.options())
        .await
        .map_err(recycle_error)?;
    recycle::delete_preflight(
        &client,
        &state.root.join("recycle.sqlite3"),
        &identity,
        session_ids,
    )
    .await
    .map_err(recycle_error)
}

#[tauri::command]
async fn desktop_action(app: tauri::AppHandle, action: Action) -> Result<Value, UiError> {
    perform(&app, action).await
}

#[tauri::command]
fn export_report(state: tauri::State<'_, Desktop>) -> Result<String, UiError> {
    let report = state.report.lock().unwrap();
    let value = report
        .as_ref()
        .ok_or_else(|| UiError::new("no_report", "请先完成一次能力探测。"))?;
    let path = state.root.join("capability-report.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(value).map_err(|_| UiError::new("export", "报告序列化失败。"))?,
    )
    .map_err(|_| UiError::new("export", "报告保存失败。"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn launch_harness(state: tauri::State<'_, Desktop>) -> Result<(), UiError> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let path = PathBuf::from(&settings.executable);
    if !path.is_absolute()
        || !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err(UiError::new(
            "executable",
            "请选择实际存在的 Harness .exe 绝对路径。",
        ));
    }
    let mut child = std::process::Command::new(path)
        .arg(format!("--remote-debugging-port={}", settings.port))
        .arg("--remote-debugging-address=127.0.0.1")
        .spawn()
        .map_err(|_| UiError::new("launch", "Harness 启动失败，请检查可执行文件路径。"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn show(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
fn quit(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Desktop>();
        let _guard = state.gate.lock().await;
        if let Some(cancel) = state.recycle_cancel.lock().unwrap().take() {
            cancel.cancel();
        }
        let owner = state.injection_owner.lock().unwrap().clone();
        if let Some(options) = owner {
            if management::execute(&options, Action::RemoveInjection, &CancellationToken::new())
                .await
                .is_err()
            {
                show(&app);
                let _ = app.emit("cleanup-failed", "无法确认注入已卸载。刷新或重启 Harness 页面可清除注入；点击“直接退出”可保留 Harness 并退出工具。");
                return;
            }
        }
        app.exit(0);
    });
}
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    quit(app);
}
#[tauri::command]
fn exit_without_cleanup(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn open_recommendation() -> Result<(), UiError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let system_root = std::env::var_os("SystemRoot")
            .ok_or_else(|| UiError::new("open_website", "无法定位系统浏览器启动程序。"))?;
        let mut child = std::process::Command::new(PathBuf::from(system_root).join("explorer.exe"))
            .arg("https://coderplant.com/")
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|error| UiError::new("open_website", &format!("无法打开官网：{error}")))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
    #[cfg(not(windows))]
    Err(UiError::new(
        "open_website",
        "当前仅支持 Windows 默认浏览器。",
    ))
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--theme-keeper") {
        if let Some(root) = args.get(2) {
            if let Ok(runtime) = tokio::runtime::Runtime::new() {
                let _ = runtime.block_on(theme_keeper::run(std::path::Path::new(root)));
            }
        }
        return;
    }
    tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_config_dir()?;
            std::fs::create_dir_all(&root)?;
            let instance_lock = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(root.join("manager.lock"))?;
            instance_lock.try_lock().map_err(|_| std::io::Error::other("管理工具已在运行，请先从托盘退出旧实例"))?;
            let recovery = match backup::recover(&root) {
                Ok(message) => message,
                Err(error) if root.join("restore-journal.json").exists() => return Err(std::io::Error::other(format!("恢复回滚未完成，已停止启动以保护配置：{error}")).into()),
                Err(error) => format!("恢复未执行：{error}。可在设置中取消待恢复任务。"),
            };
            std::fs::write(root.join("restore-status.json"), serde_json::to_vec(&recovery)?)?;
            let settings_path = root.join("settings.json");
            let settings = if settings_path.exists() {
                let settings: Settings = serde_json::from_slice(&std::fs::read(settings_path)?)?;
                settings
                    .validate()
                    .map_err(|e| std::io::Error::other(e.message))?;
                settings
            } else {
                Settings::default()
            };
            app.manage(Desktop {
                _instance_lock: instance_lock,
                settings: Mutex::new(settings),
                gate: tokio::sync::Mutex::new(()),
                report: Mutex::new(None),
                injection_owner: Mutex::new(None),
                skin_owner: Mutex::new(None),
                recycle_cancel: Mutex::new(None),
                session_cancel: Mutex::new(None),
                checkpoint_lock: Mutex::new(()),
                #[cfg(windows)]
                session_toasts: Mutex::new(Vec::new()),
                root,
            });
            let state = app.state::<Desktop>();
            if state.root.join("active-theme.json").exists() {
                theme_keeper::spawn(&state.root)?;
            }
            let open = MenuItem::with_id(app, "open", "打开管理窗口", true, None::<&str>)?;
            let diagnose = MenuItem::with_id(app, "diagnose", "检查连接", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause", "卸载测试增强", true, None::<&str>)?;
            let exit =
                MenuItem::with_id(app, "quit", "退出工具（保留 Harness）", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &diagnose, &pause, &exit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| std::io::Error::other("application icon missing"))?;
            TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .tooltip("Harness Session Plus · 尚未检查连接")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app),
                    "quit" => quit(app.clone()),
                    "diagnose" | "pause" => {
                        let app = app.clone();
                        let action = if event.id.as_ref() == "pause" {
                            Action::RemoveInjection
                        } else {
                            Action::Diagnose
                        };
                        tauri::async_runtime::spawn(async move {
                            let result = perform(&app, action).await;
                            let _ = app.emit("management-result", serde_json::json!({"action":action,"result":result.as_ref().ok(),"error":result.as_ref().err()}));
                        });
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            discover_targets,
            desktop_action,
            session_center::session_query,
            session_center::session_watch,
            session_center::session_export,
            session_center::session_notification_test,
            checkpoints::checkpoint_list,
            checkpoints::checkpoint_create,
            checkpoints::checkpoint_restore,
            checkpoints::checkpoint_delete,
            workspace::workspace_git_status,
            install_recycle,
            recycle_restore_preference,
            refresh_trash,
            restore_sessions,
            check_permanent_delete,
            launch_harness,
            export_report,
            backup::backup_create,
            backup::backup_preview,
            backup::backup_stage,
            backup::backup_cancel,
            backup::backup_diagnostics,
            backup::backup_status,
            backup::backup_open_folder,
            quit_app,
            exit_without_cleanup,
            open_recommendation,
            skin_catalog,
            theme_library,
            community_page,
            community_install,
            import_theme,
            skin_action
        ])
        .run(tauri::generate_context!())
        .expect("Harness Session Plus desktop failed to start");
}
