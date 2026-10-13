//! Reversible native archival only. Every mutation has a durable intent and independent readback.
use crate::{
    cdp::Client,
    discovery,
    management::{evaluate_harness, Options},
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

const ADAPTER: &str = include_str!("../../../injected/native-adapter.js");
pub const INJECTION: &str = include_str!("../../../injected/recycle-menu.js");
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub profile: String,
    pub host_pid: u32,
    pub main_pid: u32,
    pub adapter: String,
}

pub async fn identify(port: u16, executable: String) -> Result<Identity> {
    #[cfg(windows)]
    {
        use base64::Engine;
        use std::os::windows::process::CommandExt;
        tokio::task::spawn_blocking(move || {
            let bytes: Vec<u8> = include_str!("profile.ps1")
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            let script = base64::engine::general_purpose::STANDARD.encode(bytes);
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &script])
                .env("HSP_PORT", port.to_string())
                .env("HSP_EXE", executable)
                .creation_flags(0x08000000)
                .output()
                .map_err(|_| "无法核实 Harness 实例。".to_string())?;
            if !output.status.success() {
                return Err("当前 Harness 版本或 profile 无法核实，已禁用归档与恢复。".into());
            }
            serde_json::from_slice(&output.stdout).map_err(|_| "实例身份返回无效。".into())
        })
        .await
        .map_err(|_| "实例核验失败。".to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (port, executable);
        Err("此适配器仅支持 Windows。".into())
    }
}

pub async fn connect(options: &Options) -> Result<Client> {
    let (_, targets) = discovery::discover(options.port)
        .await
        .map_err(|_| "无法连接 Harness。")?;
    let target = discovery::select(&targets, options.target_id.as_deref())
        .map_err(|_| "请选择唯一 Harness 页面。")?;
    let client = Client::connect(
        target.websocket.as_deref().ok_or("调试地址缺失。")?,
        options.port,
    )
    .await
    .map_err(|_| "连接失败。")?;
    client
        .enable(&CancellationToken::new())
        .await
        .map_err(|_| "页面上下文不可用。")?;
    Ok(client)
}

async fn native(client: &Client, expression: String) -> Result<Value> {
    // Adapter errors return only stable codes, never request payload, title, or authentication data.
    let script=format!("(async()=>{{try{{const a={ADAPTER};return {{ok:true,value:await ({expression})}};}}catch(e){{return {{ok:false,code:String(e.message).slice(0,100)}};}}}})()");
    let value = evaluate_harness(client, &script, &CancellationToken::new())
        .await
        .map_err(|_| "连接或上下文已改变；结果未确认。".to_string())?;
    if value["ok"] != true {
        return Err(format!(
            "原生接口拒绝操作：{}",
            value["code"].as_str().unwrap_or("unknown")
        ));
    }
    Ok(value["value"].clone())
}
pub async fn snapshot(client: &Client) -> Result<Value> {
    native(client, "a.snapshot()".into()).await
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub session_id: String,
    pub title: String,
    pub workspace_id: String,
    pub workspace_title: String,
    pub moved_at: i64,
    pub state: String,
    pub error: String,
}

/// A truthful preflight: the pinned build has archive/restore but no durable session deletion API.
/// Never infer deletion from an index-only removal or an absent list item.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePreflight {
    pub supported: bool,
    pub dry_run: bool,
    pub adapter: String,
    pub reason: String,
    pub targets: Vec<DeleteTarget>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTarget {
    pub session_id: String,
    pub title: String,
    pub eligible: bool,
    pub reason: String,
}
pub const DELETE_BLOCKED_REASON: &str = "当前 Harness 0.2.0-rc.2 未提供已验证的会话永久删除接口。搜索索引清理及迁移临时文件清理不能用于删除会话；离线日志、工作区引用、缓存与附件的完整清理尚未验证。";

pub async fn delete_preflight(
    client: &Client,
    path: &Path,
    identity: &Identity,
    ids: Vec<String>,
) -> Result<DeletePreflight> {
    if ids.is_empty() || ids.len() > 100 {
        return Err("请选择 1 到 100 个回收站会话。".into());
    }
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    if unique.len() != ids.len() || ids.iter().any(|id| id.is_empty() || id.len() > 256) {
        return Err("会话 ID 重复或无效。".into());
    }
    let records = list(path, &identity.profile).await?;
    let current = snapshot(client).await?;
    let mut targets = Vec::new();
    for id in ids {
        let owned = records.iter().find(|r| r.session_id == id);
        let issue = match owned {
            None => "不属于当前 profile 的工具回收站记录。".to_string(),
            Some(record) if record.state != "archived" => {
                "回收站记录尚未确认归档，不能永久删除。".to_string()
            }
            Some(record) => match metadata(&current, &id) {
                Err(reason) => reason,
                Ok(meta) if meta.workspace_id != record.workspace_id => {
                    "原工作区身份已改变。".to_string()
                }
                Ok(_) if !archived(&current, &id)? => {
                    "原生会话已取消归档，请刷新回收站。".to_string()
                }
                Ok(_) => DELETE_BLOCKED_REASON.to_string(),
            },
        };
        targets.push(DeleteTarget {
            session_id: id,
            title: owned.map(|r| r.title.clone()).unwrap_or_default(),
            eligible: false,
            reason: issue,
        });
    }
    Ok(DeletePreflight {
        supported: false,
        dry_run: true,
        adapter: identity.adapter.clone(),
        reason: DELETE_BLOCKED_REASON.into(),
        targets,
    })
}

async fn database<T: Send + 'static>(
    path: PathBuf,
    work: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move||{
        let mut db=Connection::open(path).map_err(|_|"回收站数据库无法打开。")?;
        db.busy_timeout(std::time::Duration::from_secs(3)).map_err(|_|"数据库忙。")?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS trash(profile TEXT NOT NULL,session_id TEXT NOT NULL,title TEXT NOT NULL,workspace_id TEXT NOT NULL,workspace_title TEXT NOT NULL,moved_at INTEGER NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',PRIMARY KEY(profile,session_id)); CREATE TABLE IF NOT EXISTS operations(id INTEGER PRIMARY KEY,profile TEXT NOT NULL,session_id TEXT NOT NULL,action TEXT NOT NULL,created_at INTEGER NOT NULL,state TEXT NOT NULL);").map_err(|_|"回收站数据库初始化失败。")?;
        work(&mut db)
    }).await.map_err(|_|"数据库工作线程失败。".to_string())?
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn metadata(s: &Value, id: &str) -> Result<Record> {
    let sessions = s["sessions"].as_array().ok_or("会话列表无效。")?;
    let candidates: Vec<_> = sessions.iter().filter(|x| x["sessionId"] == id).collect();
    if candidates.len() != 1 {
        return Err("真实会话 ID 不存在或不唯一。".into());
    }
    let session = candidates[0];
    if session["running"] != false {
        return Err("会话正在运行或状态未知，不能移入回收站。".into());
    }
    let ws = s["workspaces"]["items"]
        .as_array()
        .ok_or("工作区列表无效。")?;
    let owners: Vec<_> = ws
        .iter()
        .filter(|w| {
            w["sessionIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|x| x == id))
        })
        .collect();
    if owners.len() != 1 {
        return Err("原工作区身份无法唯一确认。".into());
    }
    Ok(Record {
        session_id: id.into(),
        title: session["title"].as_str().unwrap_or("未命名会话").into(),
        workspace_id: owners[0]["workspaceId"]
            .as_str()
            .ok_or("工作区 ID 缺失。")?
            .into(),
        workspace_title: owners[0]["title"].as_str().unwrap_or("工作区").into(),
        moved_at: now(),
        state: "pending_archive".into(),
        error: String::new(),
    })
}
fn archived(s: &Value, id: &str) -> Result<bool> {
    Ok(s["workspaces"]["archivedSessionIds"]
        .as_array()
        .ok_or("归档状态不可用。")?
        .iter()
        .any(|x| x == id))
}
pub async fn list(path: &Path, profile: &str) -> Result<Vec<Record>> {
    let profile = profile.to_string();
    database(path.into(),move|db|{
        let mut stmt=db.prepare("SELECT session_id,title,workspace_id,workspace_title,moved_at,state,error FROM trash WHERE profile=? ORDER BY moved_at DESC").map_err(|_|"读取回收站失败。")?;
        let rows=stmt.query_map([profile],|r|Ok(Record{session_id:r.get(0)?,title:r.get(1)?,workspace_id:r.get(2)?,workspace_title:r.get(3)?,moved_at:r.get(4)?,state:r.get(5)?,error:r.get(6)?})).map_err(|_|"读取回收站失败。")?;
        rows.collect::<std::result::Result<Vec<_>,_>>().map_err(|_|"回收站记录无效。".into())
    }).await
}
async fn settle(path: &Path, profile: &str, id: &str, state: &str, error: &str) -> Result<()> {
    let (profile, id, state, error) = (
        profile.to_string(),
        id.to_string(),
        state.to_string(),
        error.to_string(),
    );
    database(path.into(), move |db| {
        let tx = db.transaction().map_err(|_| "记录事务失败。")?;
        tx.execute(
            "UPDATE trash SET state=?,error=? WHERE profile=? AND session_id=?",
            params![state, error, profile, id],
        )
        .map_err(|_| "结果记录失败。")?;
        tx.execute(
            "UPDATE operations SET state=? WHERE profile=? AND session_id=? AND state='pending'",
            params![state, profile, id],
        )
        .map_err(|_| "操作记录失败。")?;
        tx.commit().map_err(|_| "结果写入失败。".into())
    })
    .await
}
pub async fn reconcile(client: &Client, path: &Path, profile: &str) -> Result<Vec<Record>> {
    let s = snapshot(client).await?;
    for record in list(path, profile).await? {
        if matches!(
            record.state.as_str(),
            "pending_archive" | "pending_restore" | "unknown" | "archived"
        ) {
            let exists = s["sessions"]
                .as_array()
                .is_some_and(|xs| xs.iter().any(|x| x["sessionId"] == record.session_id));
            let state = if !exists {
                "missing"
            } else if archived(&s, &record.session_id)? {
                "archived"
            } else {
                "restored"
            };
            settle(path, profile, &record.session_id, state, "").await?;
        }
    }
    list(path, profile).await
}

pub async fn change(
    client: &Client,
    path: &Path,
    identity: &Identity,
    id: &str,
    restore: bool,
) -> Result<()> {
    if id.is_empty() || id.len() > 256 {
        return Err("无效会话 ID。".into());
    }
    let before = snapshot(client).await?;
    if restore {
        let record = list(path, &identity.profile)
            .await?
            .into_iter()
            .find(|r| r.session_id == id)
            .ok_or("工具未收录此会话，拒绝恢复。")?;
        if !archived(&before, id)? {
            return Err("原生会话已取消归档，请刷新回收站。".into());
        }
        let current = metadata(&before, id)?;
        if current.workspace_id != record.workspace_id {
            return Err("原工作区已改变，拒绝恢复。".into());
        }
    } else if archived(&before, id)? {
        return Err("会话已被原生归档，工具不会接管。".into());
    }
    let record = metadata(&before, id)?;
    let expected_workspace = record.workspace_id.clone();
    let (profile, id_owned) = (identity.profile.clone(), id.to_string());
    database(path.into(),move|db|{
        let tx=db.transaction().map_err(|_|"记录事务失败。")?;
        if !restore{
            tx.execute("INSERT INTO trash(profile,session_id,title,workspace_id,workspace_title,moved_at,state) VALUES(?,?,?,?,?,?,'pending_archive') ON CONFLICT(profile,session_id) DO UPDATE SET title=excluded.title,workspace_id=excluded.workspace_id,workspace_title=excluded.workspace_title,moved_at=excluded.moved_at,state='pending_archive',error='' WHERE trash.state IN ('restored','failed')",params![profile,id_owned,record.title,record.workspace_id,record.workspace_title,record.moved_at]).map_err(|_|"写入归档意图失败。")?;
            if tx.changes()!=1{return Err("已有未完成操作，请先刷新核对。".into());}
        }else{tx.execute("UPDATE trash SET state='pending_restore',error='' WHERE profile=? AND session_id=? AND state='archived'",params![profile,id_owned]).map_err(|_|"写入恢复意图失败。")?;if tx.changes()!=1{return Err("记录状态不允许恢复。".into());}}
        tx.execute("INSERT INTO operations(profile,session_id,action,created_at,state) VALUES(?,?,?,?,'pending')",params![profile,id_owned,if restore{"restore"}else{"archive"},now()]).map_err(|_|"操作日志写入失败。")?;
        tx.commit().map_err(|_|"操作意图提交失败。".into())
    }).await?;
    let args = json!({"request":{"sessionId":id}}); // stopActivity is deliberately absent: native guard checks ALL activity.
    let endpoint = if restore {
        "workspace/unarchiveSession"
    } else {
        "workspace/archiveSession"
    };
    let result = native(
        client,
        format!(
            "a.rpc({}, {}, {})",
            json!(endpoint),
            args,
            before["hostOrigin"]
        ),
    )
    .await;
    let after = snapshot(client).await;
    match after {
        Ok(value) => {
            let exists = value["sessions"]
                .as_array()
                .is_some_and(|xs| xs.iter().any(|s| s["sessionId"] == id));
            let accounted = value["workspaces"]["items"].as_array().is_some_and(|ws| {
                ws.iter().any(|w| {
                    w["workspaceId"] == expected_workspace
                        && w["sessionIds"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|s| s == id))
                })
            });
            if !exists || !accounted || value["hostOrigin"] != before["hostOrigin"] {
                settle(
                    path,
                    &identity.profile,
                    id,
                    "unknown",
                    "原会话或工作区读回不一致，请刷新核对。未自动重试。",
                )
                .await?;
                return Err("操作结果未确认：会话或工作区读回不一致。".into());
            }
            let matches = archived(&value, id)? != restore;
            let status = if matches {
                if restore {
                    "restored"
                } else {
                    "archived"
                }
            } else if restore {
                "archived"
            } else {
                "failed"
            };
            settle(
                path,
                &identity.profile,
                id,
                status,
                if matches {
                    ""
                } else {
                    "原生操作未生效；未自动重试。"
                },
            )
            .await?;
            if matches {
                Ok(())
            } else {
                Err(result.err().unwrap_or("原生归档状态未改变。".into()))
            }
        }
        Err(_) => {
            settle(
                path,
                &identity.profile,
                id,
                "unknown",
                "结果未确认，请重连后刷新；不会自动重放。",
            )
            .await?;
            Err("操作结果未确认，请刷新回收站核对。".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_unknown_running_and_duplicate_ownership() {
        let mut s = json!({"sessions":[{"sessionId":"x","title":"same","running":false}],"workspaces":{"items":[{"workspaceId":"w","title":"ws","sessionIds":["x"]}],"archivedSessionIds":[]}});
        assert!(metadata(&s, "x").is_ok());
        assert!(metadata(&s, "same").is_err());
        s["sessions"][0]["running"] = Value::Null;
        assert!(metadata(&s, "x").is_err());
        s["sessions"][0]["running"] = json!(false);
        s["workspaces"]["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({"workspaceId":"other","sessionIds":["x"]}));
        assert!(metadata(&s, "x").is_err());
    }
    #[tokio::test]
    async fn durable_records_are_isolated_by_profile_and_journal_settles_atomically() {
        let root =
            std::env::temp_dir().join(format!("hsp-journal-test-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("own.sqlite3");
        database(path.clone(),|db|{
            for profile in ["profile-a","profile-b"]{
                db.execute("INSERT INTO trash VALUES(?,'same-id','same-title','w','workspace',1,'pending_archive','')",[profile]).unwrap();
                db.execute("INSERT INTO operations(profile,session_id,action,created_at,state) VALUES(?,'same-id','archive',1,'pending')",[profile]).unwrap();
            }
            Ok(())
        }).await.unwrap();
        settle(&path, "profile-a", "same-id", "archived", "")
            .await
            .unwrap();
        assert_eq!(list(&path, "profile-a").await.unwrap()[0].state, "archived");
        assert_eq!(
            list(&path, "profile-b").await.unwrap()[0].state,
            "pending_archive"
        );
        let statuses = database(path, |db| {
            let mut stmt = db
                .prepare("SELECT state FROM operations ORDER BY profile")
                .unwrap();
            Ok(stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap())
        })
        .await
        .unwrap();
        assert_eq!(statuses, ["archived", "pending"]);
    }
}
