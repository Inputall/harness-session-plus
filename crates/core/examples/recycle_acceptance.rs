//! Explicit local acceptance: creates two dedicated SAME-TITLE sessions, changes only those IDs.
use hsp_core::{
    management::{evaluate_harness, Options},
    recycle,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exe = std::env::var("HSP_TEST_EXE")?;
    let options = Options {
        port: 9223,
        target_id: None,
    };
    let identity = recycle::identify(options.port, exe).await?;
    let client = recycle::connect(&options).await?;
    let before = recycle::snapshot(&client).await?;
    let workspace = before["workspaces"]["items"][0]["workspaceId"]
        .as_str()
        .ok_or("No workspace")?;
    let adapter = include_str!("../../../injected/native-adapter.js");
    let script=format!("(async()=>{{const a={adapter};const ids=[];for(let i=0;i<2;i++){{const s=await a.rpc('session/create',{{request:{{workspaceId:{}}}}});await a.rpc('session/rename',{{request:{{sessionId:s.sessionId,title:'HSP 回收站验收专用'}}}});ids.push(s.sessionId);}}return ids;}})()",json!(workspace));
    let ids = evaluate_harness(&client, &script, &CancellationToken::new()).await?;
    let first = ids[0].as_str().ok_or("Missing test ID")?;
    let second = ids[1].as_str().ok_or("Missing test ID")?;
    let db = std::env::current_dir()?.join("target/recycle-acceptance.sqlite3");
    let snapshot = recycle::snapshot(&client).await?;
    assert!(snapshot["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["sessionId"] == first || s["sessionId"] == second)
        .all(|s| s["title"] == "HSP 回收站验收专用"));
    recycle::change(&client, &db, &identity, first, false).await?;
    let after = recycle::snapshot(&client).await?;
    assert!(after["workspaces"]["archivedSessionIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == first));
    assert!(!after["workspaces"]["archivedSessionIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == second));
    let records = recycle::reconcile(&client, &db, &identity.profile).await?;
    assert_eq!(records.iter().filter(|r| r.state == "archived").count(), 1);
    recycle::change(&client, &db, &identity, first, true).await?;
    let restored = recycle::snapshot(&client).await?;
    assert!(!restored["workspaces"]["archivedSessionIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == first));
    assert!(restored["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["sessionId"] == first));
    println!("PASS: two dedicated same-title sessions created; one archived and restored by ID; other unchanged; SQLite journal confirmed. No permanent deletion.");
    Ok(())
}
