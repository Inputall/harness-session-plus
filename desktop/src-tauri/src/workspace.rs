//! Read-only project workspace diagnostics. No Git mutation commands are exposed.
use crate::{recycle_error, UiError};
use hsp_core::{recycle, sessions};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::Path, process::Stdio};
use tokio::process::Command;

const MAX_OUTPUT: usize = 128 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitFile {
    status: String,
    path: String,
}

fn valid_root(cwd: &str) -> Result<&Path, UiError> {
    if cwd.is_empty() || cwd.len() > 4096 || cwd.contains('\0') {
        return Err(UiError::new("workspace", "工作区路径无效。"));
    }
    let path = Path::new(cwd);
    if !path.is_absolute() || !path.is_dir() {
        return Err(UiError::new(
            "workspace",
            "工作区目录不存在或不是绝对路径。",
        ));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| UiError::new("workspace", "工作区目录无法读取。"))?;
    if metadata.file_type().is_symlink() {
        return Err(UiError::new("workspace", "拒绝通过符号链接读取工作区。"));
    }
    Ok(path)
}

async fn git(cwd: &Path, args: &[&str]) -> Result<String, UiError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .arg("--no-optional-locks")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .args(args)
        .stdin(Stdio::null())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .await
        .map_err(|_| UiError::new("workspace", "未找到可用的 Git 命令。"))?;
    if output.stdout.len() > MAX_OUTPUT || output.stderr.len() > MAX_OUTPUT {
        return Err(UiError::new("workspace", "Git 输出超过安全限制。"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        if args.first() == Some(&"log") && detail.contains("does not have any commits yet") {
            return Ok(String::new());
        }
        let message = if detail.contains("not a git repository") {
            "当前工作区不是 Git 仓库。"
        } else {
            "Git 只读检查失败，请确认目录和仓库状态。"
        };
        return Err(UiError::new("workspace", message));
    }
    Ok(stdout)
}

fn parse_status(raw: &str) -> (String, Vec<GitFile>) {
    let mut branch = "未识别".to_string();
    let mut files = Vec::new();
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("## ") {
            branch = value.trim().to_string();
            continue;
        }
        if line.len() < 4 {
            continue;
        }
        let xy = &line[..2];
        let status = match (xy.as_bytes()[0] as char, xy.as_bytes()[1] as char) {
            ('?', '?') => "未跟踪".to_string(),
            (' ', 'M') => "工作区修改".to_string(),
            ('M', ' ') => "已暂存修改".to_string(),
            ('A', ' ') => "已暂存新增".to_string(),
            ('D', ' ') => "已暂存删除".to_string(),
            (' ', 'D') => "工作区删除".to_string(),
            ('R', _) => "已暂存重命名".to_string(),
            (' ', 'R') => "工作区重命名".to_string(),
            ('M', 'M') => "已暂存且继续修改".to_string(),
            ('A', 'M') => "已暂存新增且继续修改".to_string(),
            _ => xy.trim().to_string(),
        };
        let path = line[3..].trim().to_string();
        if !path.is_empty() {
            files.push(GitFile { status, path });
        }
    }
    (branch, files)
}

#[tauri::command]
pub async fn workspace_git_status(
    session_id: String,
    expected_profile: String,
    state: tauri::State<'_, crate::Desktop>,
) -> Result<Value, UiError> {
    if session_id.is_empty() || session_id.len() > 256 || expected_profile.is_empty() {
        return Err(UiError::new("workspace", "会话身份无效，请刷新会话列表。"));
    }
    let settings = state.settings.lock().unwrap().clone();
    let identity = recycle::identify(settings.port, settings.executable.clone())
        .await
        .map_err(recycle_error)?;
    if identity.profile != expected_profile {
        return Err(UiError::new(
            "profile",
            "Harness profile 已改变，请刷新会话列表。",
        ));
    }
    let snapshot = sessions::query(&settings.options(), &sessions::Request::List)
        .await
        .map_err(recycle_error)?;
    let cwd = snapshot["sessions"]
        .as_array()
        .and_then(|list| list.iter().find(|item| item["sessionId"] == session_id))
        .and_then(|item| item["cwd"].as_str())
        .filter(|cwd| !cwd.is_empty())
        .ok_or_else(|| UiError::new("workspace", "该会话没有可核对的工作区目录。"))?
        .to_string();
    let root = valid_root(&cwd)?.to_path_buf();
    tokio::task::spawn(async move {
        let status = git(&root, &["status", "--porcelain=v1", "--branch", "--untracked-files=normal"]).await?;
        let log = git(&root, &["log", "-5", "--date=short", "--pretty=format:%h%x09%ad%x09%s"]).await?;
        let (branch, files) = parse_status(&status);
        let commits = log
            .lines()
            .filter(|line| !line.is_empty())
            .take(5)
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        Ok(json!({"cwd":root.to_string_lossy(),"branch":branch,"files":files,"commits":commits,"clean":files.is_empty()}))
    })
    .await
    .map_err(|_| UiError::new("workspace", "Git 检查任务中断。"))?
}

#[cfg(test)]
mod tests {
    use super::{git, parse_status};
    #[test]
    fn parses_branch_and_worktree_states_without_mutation() {
        let (branch, files) = parse_status(
            "## main...origin/main [ahead 1]\nM  staged.rs\n M working.rs\n?? new.rs\n",
        );
        assert_eq!(branch, "main...origin/main [ahead 1]");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].status, "已暂存修改");
        assert_eq!(files[2].status, "未跟踪");
    }

    #[tokio::test]
    async fn reads_git_state_without_changing_the_index() {
        use std::{fs, process::Command};
        let root = std::env::temp_dir().join(format!("hsp-git-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap()
        };
        assert!(Command::new("git")
            .args(["init", root.to_str().unwrap()])
            .status()
            .unwrap()
            .success());
        fs::write(root.join("staged.rs"), "staged").unwrap();
        assert!(run(&["add", "staged.rs"]).status.success());
        fs::write(root.join("working.rs"), "untracked").unwrap();
        let before = fs::read(root.join(".git/index")).unwrap();
        let status = git(
            &root,
            &[
                "status",
                "--porcelain=v1",
                "--branch",
                "--untracked-files=normal",
            ],
        )
        .await
        .unwrap();
        let (_, files) = parse_status(&status);
        assert!(files
            .iter()
            .any(|f| f.path == "staged.rs" && f.status == "已暂存新增"));
        assert!(files
            .iter()
            .any(|f| f.path == "working.rs" && f.status == "未跟踪"));
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), before);
        assert!(git(
            &root,
            &[
                "log",
                "-5",
                "--date=short",
                "--pretty=format:%h%x09%ad%x09%s"
            ]
        )
        .await
        .unwrap()
        .is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
