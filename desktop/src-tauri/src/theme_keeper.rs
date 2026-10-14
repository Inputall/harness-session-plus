//! Independent appearance-only keeper. No session actions or app package changes.
use hsp_core::{management::Options, skin};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs::OpenOptions, path::Path};

#[derive(Clone, Serialize, Deserialize)]
pub struct SavedTheme {
    pub options: Options,
    pub skin: Value,
    pub revision: String,
}

impl SavedTheme {
    pub fn validate(&self) -> Result<(), String> {
        if self.options.port == 0 || self.revision.is_empty() || self.revision.len() > 100 {
            return Err("持久主题配置无效".into());
        }
        hsp_core::theme_import::validate(&self.skin)?;
        if self.skin["persistenceRevision"] != self.revision {
            return Err("持久主题版本不一致".into());
        }
        if self.skin.get("wallpaperClarity").is_some()
            && self.skin["wallpaperClarity"]
                .as_u64()
                .is_none_or(|v| v > 100)
        {
            return Err("背景清晰度无效".into());
        }
        Ok(())
    }
}

pub fn spawn(root: &Path) -> std::io::Result<()> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.arg("--theme-keeper").arg(root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub async fn run(root: &Path) -> std::io::Result<()> {
    // OS locks are released even after a crash. Only one keeper per tool profile.
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("theme-keeper.lock"))?;
    if lock.try_lock().is_err() {
        return Ok(());
    }
    let path = root.join("active-theme.json");
    loop {
        let operation_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("theme-operation.lock"))?;
        if operation_file.try_lock().is_err() {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            continue;
        }
        let data = match std::fs::read(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => {
                drop(operation_file);
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                continue;
            }
        };
        let saved: SavedTheme = match serde_json::from_slice(&data) {
            Ok(saved) => saved,
            Err(_) => break,
        };
        if saved.validate().is_err() {
            break;
        }
        // Selection IDs change on restart: rebind only when exactly one Harness page exists.
        let options = Options {
            port: saved.options.port,
            target_id: None,
        };
        let revision = saved.revision.clone();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let status = skin::execute(&options, "status", None).await?;
            if status["installed"] != true || status["revision"] != revision {
                skin::execute_with_skin(&options, "apply", Some(saved.skin.clone())).await?;
            }
            Ok::<(), hsp_core::cdp::Error>(())
        })
        .await;
        drop(operation_file);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_theme_rejects_invalid_options_and_clarity() {
        let mut saved = SavedTheme {
            options: Options {
                port: 9223,
                target_id: None,
            },
            skin: {
                let mut value = skin::select("midnight").unwrap();
                value["persistenceRevision"] = serde_json::json!("test-1");
                value
            },
            revision: "test-1".into(),
        };
        assert!(saved.validate().is_ok());
        saved.skin["wallpaperClarity"] = serde_json::json!(101);
        assert!(saved.validate().is_err());
        saved.skin["wallpaperClarity"] = serde_json::json!(70);
        saved.options.port = 0;
        assert!(saved.validate().is_err());
    }
}
