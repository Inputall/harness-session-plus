#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hsp_core::{
    cdp::Error,
    discovery,
    management::{self, Action, Options},
    recycle,
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
    settings: Mutex<Settings>,
    gate: tokio::sync::Mutex<()>,
    report: Mutex<Option<Value>>,
    injection_owner: Mutex<Option<Options>>,
    recycle_cancel: Mutex<Option<CancellationToken>>,
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
    if state.injection_owner.lock().unwrap().is_some() {
        return Err(UiError::new(
            "injection_active",
            "请先卸载测试注入，再更改配置，避免遗留页面增强。",
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
        std::thread::spawn(move || { let _ = child.wait(); });
        Ok(())
    }
    #[cfg(not(windows))]
    Err(UiError::new("open_website", "当前仅支持 Windows 默认浏览器。"))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let root = app.path().app_config_dir()?;
            std::fs::create_dir_all(&root)?;
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
                settings: Mutex::new(settings),
                gate: tokio::sync::Mutex::new(()),
                report: Mutex::new(None),
                injection_owner: Mutex::new(None),
                recycle_cancel: Mutex::new(None),
                root,
            });
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
            install_recycle,
            refresh_trash,
            restore_sessions,
            check_permanent_delete,
            launch_harness,
            export_report,
            quit_app,
            exit_without_cleanup,
            open_recommendation
        ])
        .run(tauri::generate_context!())
        .expect("Harness Session Plus desktop failed to start");
}
