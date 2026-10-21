//! Bounded read-only session search and excerpts for the pinned native adapter.
use crate::{cdp::Client, recycle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize, Serialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Request {
    List,
    Search { query: String },
    Detail { session_id: String },
    Open { session_id: String },
    Jobs { session_id: String },
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Search { query }
                if query.trim().is_empty()
                    || query.encode_utf16().count() > 200
                    || query.contains('\0') =>
            {
                Err("搜索词需要 1–200 个字符，不能包含空字符。".into())
            }
            Self::Detail { session_id } | Self::Open { session_id } | Self::Jobs { session_id }
                if session_id.is_empty() || session_id.len() > 256 || session_id.contains('\0') =>
            {
                Err("会话 ID 无效。".into())
            }
            _ => Ok(()),
        }
    }
}
pub async fn execute(client: &Client, request: &Request) -> Result<Value, String> {
    request.validate()?;
    let expression = format!("(async()=>{{try{{return {{ok:true,value:await ({})({}, {})}}}}catch{{return {{ok:false}}}}}})()",
        include_str!("../../../injected/session-center.js"),
        include_str!("../../../injected/native-adapter.js"), json!(request));
    let guarded = format!("(()=>{{if(location.href!=='dsh-app://app/')throw Error('Harness page mismatch');return ({expression});}})()");
    let result = client
        .evaluate_with_timeout(
            &guarded,
            &CancellationToken::new(),
            std::time::Duration::from_secs(20),
        )
        .await
        .map_err(|_| "会话查询连接已改变，请刷新后重试。".to_string())?;
    if result["ok"] != true {
        return Err("当前会话查询或导航不可用。请确认会话存在；归档会话需先恢复。导航需要展开 Harness 工作区。".into());
    }
    Ok(result["value"].clone())
}

/// Caller must identify the current pinned build before connecting.
pub async fn query(
    options: &crate::management::Options,
    request: &Request,
) -> Result<Value, String> {
    request.validate()?;
    let client = recycle::connect(options).await?;
    let result = execute(&client, request).await;
    client.close();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_and_unknown_requests() {
        assert!(Request::Search { query: " ".into() }.validate().is_err());
        assert!(Request::Search {
            query: "😀".repeat(101)
        }
        .validate()
        .is_err());
        assert!(Request::Detail {
            session_id: "x\0y".into()
        }
        .validate()
        .is_err());
        assert!(Request::Jobs {
            session_id: "x".repeat(257)
        }
        .validate()
        .is_err());
        assert!(
            serde_json::from_value::<Request>(json!({"action":"delete","sessionId":"x"})).is_err()
        );
        assert_eq!(
            json!(Request::Detail {
                session_id: "unique-id".into()
            })["sessionId"],
            "unique-id"
        );
        assert_eq!(
            json!(Request::Jobs {
                session_id: "session-a".into()
            }),
            json!({"action":"jobs","sessionId":"session-a"})
        );
    }
}
