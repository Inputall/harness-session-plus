use crate::{recycle_error, show, Desktop, UiError};
use hsp_core::{recycle, sessions};
use serde_json::{json, Value};
use std::collections::HashMap;
use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

#[tauri::command]
pub async fn session_query(
    request: sessions::Request,
    expected_profile: Option<String>,
    state: tauri::State<'_, Desktop>,
) -> Result<Value, UiError> {
    request.validate().map_err(recycle_error)?;
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| UiError::new("busy", "请等待当前操作完成。"))?;
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    if expected_profile
        .as_ref()
        .is_some_and(|p| *p != identity.profile)
    {
        return Err(UiError::new(
            "profile",
            "Harness profile 已改变，请刷新会话中心。",
        ));
    }
    let mut result = sessions::query(&settings.options(), &request)
        .await
        .map_err(recycle_error)?;
    result["profile"] = json!(identity.profile);
    if matches!(request, sessions::Request::Open { .. }) {
        foreground(identity.main_pid);
    }
    Ok(result)
}

#[cfg(windows)]
fn foreground(pid: u32) {
    use windows::core::BOOL;
    use windows::Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
            ShowWindow, SW_RESTORE,
        },
    };
    unsafe extern "system" fn find(hwnd: HWND, value: LPARAM) -> BOOL {
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process));
        if process == value.0 as u32 && IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(pid as isize));
    }
}
#[cfg(not(windows))]
fn foreground(_: u32) {}

pub(crate) fn foreground_for_checkpoint(pid: u32) {
    foreground(pid);
}

#[tauri::command]
pub fn session_export(
    content: String,
    state: tauri::State<'_, Desktop>,
) -> Result<String, UiError> {
    if content.len() > 1_048_576 || content.trim().is_empty() {
        return Err(UiError::new("export", "导出内容为空或超过大小限制。"));
    }
    let folder = state.root.join("exports");
    std::fs::create_dir_all(&folder).map_err(|_| UiError::new("export", "无法创建导出目录。"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = folder.join(format!("session-context-{stamp}.md"));
    std::fs::write(&path, content).map_err(|_| UiError::new("export", "无法保存导出文件。"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn session_notification_test(app: tauri::AppHandle) -> Result<(), UiError> {
    native_notice(
        app,
        json!({"kind":"桌面通知测试：点击返回会话中心", "test":true}),
    )
    .map_err(recycle_error)
}

#[tauri::command]
pub async fn session_watch(
    app: tauri::AppHandle,
    session_ids: Vec<String>,
    native: bool,
    events: Vec<String>,
    expected_profile: Option<String>,
) -> Result<(), UiError> {
    let allowed_events = ["approval", "question", "plan-review", "stopped", "error"];
    if session_ids.len() > 20
        || session_ids.iter().any(|id| id.is_empty() || id.len() > 256)
        || events.len() > allowed_events.len()
        || events
            .iter()
            .any(|event| !allowed_events.contains(&event.as_str()))
    {
        return Err(UiError::new("watch", "最多关注 20 个会话。"));
    }
    let state = app.state::<Desktop>();
    if let Some(cancel) = state.session_cancel.lock().unwrap().take() {
        cancel.cancel();
    }
    if session_ids.is_empty() {
        return Ok(());
    }
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    if expected_profile
        .as_ref()
        .is_some_and(|p| *p != identity.profile)
    {
        return Err(UiError::new(
            "profile",
            "Harness profile 已改变，请刷新会话中心。",
        ));
    }
    // Validate the baseline before reporting that monitoring has started.
    let initial = sessions::query(&settings.options(), &sessions::Request::List)
        .await
        .map_err(recycle_error)?;
    let available = initial["sessions"]
        .as_array()
        .ok_or_else(|| UiError::new("watch", "会话列表无效。"))?;
    if session_ids.iter().any(|id| {
        !available
            .iter()
            .any(|s| s["sessionId"] == *id && s["archived"] == false)
    }) {
        return Err(UiError::new(
            "watch",
            "关注的会话不存在或已归档，请刷新列表。",
        ));
    }
    let cancel = CancellationToken::new();
    *state.session_cancel.lock().unwrap() = Some(cancel.clone());
    let events: std::collections::HashSet<String> = events.into_iter().collect();
    tauri::async_runtime::spawn(async move {
        let mut previous = states(&initial);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! { _ = cancel.cancelled() => break, _ = interval.tick() => {} }
            let state = app.state::<Desktop>();
            let current_settings = state.settings.lock().unwrap().clone();
            if current_settings.port != settings.port
                || current_settings.target_id != settings.target_id
                || current_settings.executable != settings.executable
            {
                let _ = app.emit("session-watch-stopped", "连接配置已改变，请重新开启关注。");
                break;
            }
            let attempt = async {
                let current = recycle::identify(settings.port, settings.executable.clone()).await?;
                if current != identity {
                    return Err("Harness 实例已改变，关注已停止；请重新开启。".to_string());
                }
                let _guard = state.gate.lock().await;
                let result = sessions::query(&settings.options(), &sessions::Request::List).await?;
                Ok(result)
            };
            let snapshot = tokio::select! { _ = cancel.cancelled()=>break, result=attempt=>result };
            let mut snapshot = match snapshot {
                Ok(value) => value,
                Err(_) => {
                    let _ = app.emit(
                        "session-watch-stopped",
                        "连接或实例已改变，关注已停止；重连后请重新开启。",
                    );
                    break;
                }
            };
            snapshot["profile"] = json!(identity.profile);
            if cancel.is_cancelled() {
                break;
            }
            let current = states(&snapshot);
            for id in &session_ids {
                if let (Some(old), Some(new)) = (previous.get(id), current.get(id)) {
                    if let Some(mut kind) = transition(old, new) {
                        let mut event = transition_event(kind);
                        if old["running"] == true && new["running"] == false {
                            let read_end = async {
                                let _guard = state.gate.lock().await;
                                sessions::query(
                                    &settings.options(),
                                    &sessions::Request::Detail {
                                        session_id: id.clone(),
                                    },
                                )
                                .await
                            };
                            let detail = tokio::select! { _ = cancel.cancelled()=>return, value=read_end=>value };
                            if let Ok(detail) = detail {
                                if confirmed_error(old, &detail) {
                                    kind = "会话发生错误，请查看详情";
                                    event = "error";
                                }
                            }
                        }
                        if !events.contains(event) {
                            continue;
                        }
                        if event == "error" {
                            kind = "会话发生错误，请查看详情";
                        }
                        let notice = json!({"sessionId":id,"title":new["title"],"kind":kind,"profile":identity.profile});
                        let _ = app.emit("session-notice", &notice);
                        if native {
                            if let Err(reason) = native_notice(app.clone(), notice.clone()) {
                                let _ = app.emit("session-notification-unavailable", reason);
                            }
                        }
                    }
                }
            }
            previous = current;
            let _ = app.emit("session-snapshot", snapshot);
        }
    });
    Ok(())
}

fn transition_event(kind: &str) -> &'static str {
    match kind {
        "等待审批" => "approval",
        "等待回答" => "question",
        "等待计划审阅" => "plan-review",
        _ => "stopped",
    }
}

fn states(snapshot: &Value) -> HashMap<String, Value> {
    snapshot["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            s["sessionId"]
                .as_str()
                .map(|id| (id.to_string(), s.clone()))
        })
        .collect()
}
fn transition(old: &Value, new: &Value) -> Option<&'static str> {
    if new["archived"] == true {
        return None;
    }
    if new["statusObserved"] == true && new["pending"] != old["pending"] {
        match new["pending"].as_str() {
            Some("approval") => return Some("等待审批"),
            Some("question") => return Some("等待回答"),
            Some("plan-review") => return Some("等待计划审阅"),
            _ => {}
        }
    }
    if old["running"] == true && new["running"] == false {
        return Some("运行已停止，请查看结果");
    }
    None
}
fn confirmed_error(old: &Value, detail: &Value) -> bool {
    detail["lastTurn"]["reason"] == "error"
        && old["cursor"]
            .as_i64()
            .zip(detail["lastTurn"]["seq"].as_i64())
            .is_some_and(|(before, after)| after > before)
}

#[cfg(windows)]
fn native_notice(app: tauri::AppHandle, notice: Value) -> Result<(), String> {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    use windows::{
        core::{IInspectable, HSTRING},
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{
            NotificationSetting, ToastFailedEventArgs, ToastNotification, ToastNotificationManager,
        },
    };
    let stage = std::cell::Cell::new("注册通知身份");
    let deliver = || -> windows::core::Result<()> {
        // Tokio worker threads need a WinRT apartment. RPC_E_CHANGED_MODE means it already has an STA.
        struct Apartment(bool);
        impl Drop for Apartment {
            fn drop(&mut self) {
                if self.0 {
                    unsafe {
                        windows::Win32::System::WinRT::RoUninitialize();
                    }
                }
            }
        }
        let _apartment = Apartment(unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok());
        register_notifications()?;
        stage.set("创建通知服务");
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
            "com.harness-session-plus.manager",
        ))?;
        stage.set("读取通知权限");
        match notifier.Setting() {
            Ok(NotificationSetting::Enabled) => {}
            Ok(_) => {
                return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x80070005u32 as i32,
                )))
            }
            // Windows has no per-app setting record before an unpackaged app's first toast.
            // Let Show establish it; all other permission errors remain failures.
            Err(error) if error.code().0 as u32 == 0x80070490 => {}
            Err(error) => return Err(error),
        }
        stage.set("构建通知");
        let xml = XmlDocument::new()?;
        // No conversation text or title in OS notification / notification history.
        xml.LoadXml(&HSTRING::from(format!("<toast><visual><binding template=\"ToastGeneric\"><text>Harness Session Plus</text><text>{}</text></binding></visual></toast>", notice["kind"].as_str().unwrap_or("会话状态变化"))))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        let app_click = app.clone();
        let app_failed = app.clone();
        toast.Failed(
            &TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, _| {
                let _ = app_failed.emit(
                    "session-notification-unavailable",
                    "Windows 拒绝显示通知；请检查系统通知设置，工具内提醒仍有效。",
                );
                Ok(())
            }),
        )?;
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, _| {
                show(&app_click);
                let _ = app_click.emit("session-notice-click", &notice);
                Ok(())
            },
        ))?;
        stage.set("显示通知");
        notifier.Show(&toast)?;
        let state = app.state::<Desktop>();
        let mut handles = state.session_toasts.lock().unwrap();
        handles.push(toast);
        if handles.len() > 32 {
            handles.remove(0);
        }
        Ok(())
    };
    deliver().map_err(|error| {
        format!(
            "Windows 通知在{}失败（错误码 {:08X}）；仍会保留工具内通知。",
            stage.get(),
            error.code().0 as u32
        )
    })
}
#[cfg(windows)]
fn register_notifications() -> windows::core::Result<()> {
    use windows::{
        core::{w, Interface, GUID, HSTRING, PCWSTR},
        Win32::{
            Foundation::PROPERTYKEY,
            System::{
                Com::{
                    CoCreateInstance, IPersistFile,
                    StructuredStorage::{PropVariantChangeType, PROPVARIANT, PVCHF_DEFAULT},
                    CLSCTX_INPROC_SERVER,
                },
                Registry::*,
            },
            UI::Shell::{
                IShellLinkW, PropertiesSystem::IPropertyStore,
                SetCurrentProcessExplicitAppUserModelID, ShellLink,
            },
        },
    };
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Classes\\AppUserModelId\\com.harness-session-plus.manager"),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let text: Vec<u8> = "Harness Session Plus\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let result = RegSetValueExW(key, w!("DisplayName"), None, REG_SZ, Some(&text)).ok();
        let _ = RegCloseKey(key);
        result?;
        SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(
            "com.harness-session-plus.manager",
        ))?;
        // Unpackaged Windows apps need a Start Menu shortcut carrying their AUMID.
        // This is a normal launch shortcut, never a startup entry.
        let denied =
            || windows::core::Error::from_hresult(windows::core::HRESULT(0x80070005u32 as i32));
        let base = std::env::var_os("APPDATA").ok_or_else(denied)?;
        let folder = std::path::PathBuf::from(base).join("Microsoft/Windows/Start Menu/Programs");
        std::fs::create_dir_all(&folder).map_err(|_| denied())?;
        let shortcut = folder.join("Harness Session Plus Notifications.lnk");
        let exe = std::env::current_exe().map_err(|_| denied())?;
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe.to_string_lossy().as_ref()))?;
        link.SetDescription(w!("Harness Session Plus notifications"))?;
        let store: IPropertyStore = link.cast()?;
        let key = PROPERTYKEY {
            fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
            pid: 5,
        };
        let source = PROPVARIANT::from("com.harness-session-plus.manager");
        let mut value = PROPVARIANT::default();
        PropVariantChangeType(
            &mut value,
            &source,
            PVCHF_DEFAULT,
            windows::Win32::System::Variant::VT_LPWSTR,
        )?;
        store.SetValue(&key, &value)?;
        store.Commit()?;
        let persist: IPersistFile = link.cast()?;
        persist.Save(&HSTRING::from(shortcut.to_string_lossy().as_ref()), true)?;
    }
    Ok(())
}
#[cfg(not(windows))]
fn native_notice(_: tauri::AppHandle, _: Value) -> Result<(), String> {
    Err("桌面通知仅支持 Windows。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transition_is_not_a_success_or_failure_inference() {
        let old = json!({"running":true,"pending":null});
        assert_eq!(
            transition(&old, &json!({"running":false})),
            Some("运行已停止，请查看结果")
        );
        assert_eq!(
            transition(
                &old,
                &json!({"running":true,"statusObserved":true,"pending":"approval"})
            ),
            Some("等待审批")
        );
        assert_eq!(
            transition(
                &old,
                &json!({"running":true,"pending":"question","statusObserved":false})
            ),
            None
        );
        assert_eq!(
            transition(&old, &json!({"running":false,"archived":true})),
            None
        );
        assert_eq!(transition(&old, &old), None);
        assert!(confirmed_error(
            &json!({"cursor":10}),
            &json!({"lastTurn":{"seq":12,"reason":"error"}})
        ));
        assert!(!confirmed_error(
            &json!({"cursor":10}),
            &json!({"lastTurn":{"seq":9,"reason":"error"}})
        ));
        assert!(!confirmed_error(
            &json!({}),
            &json!({"lastTurn":{"seq":12,"reason":"error"}})
        ));
        assert_eq!(transition_event("等待审批"), "approval");
        assert_eq!(transition_event("等待回答"), "question");
        assert_eq!(transition_event("等待计划审阅"), "plan-review");
        assert_eq!(transition_event("运行已停止，请查看结果"), "stopped");
    }
}
