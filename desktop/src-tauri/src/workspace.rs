//! Read-only project workspace diagnostics. No Git mutation commands are exposed.
use crate::{recycle_error, UiError};
use hsp_core::{recycle, sessions};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

const MAX_OUTPUT: usize = 128 * 1024;
const MAX_DIFF_OUTPUT: usize = 96 * 1024;
const MAX_RULE_FILE: usize = 32 * 1024;
const MAX_RULE_TOTAL: usize = 256 * 1024;
const MAX_RULE_FILES: usize = 32;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitFile {
    status: String,
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleFile {
    path: String,
    bytes: usize,
    truncated: bool,
    content: String,
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

async fn git_diff(cwd: &Path, cached: bool) -> Result<(String, bool), UiError> {
    let read = async {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(cwd)
            .arg("--no-optional-locks")
            .arg("-c")
            .arg("core.fsmonitor=false")
            .arg("diff")
            .arg("--no-ext-diff")
            .arg("--no-textconv")
            .arg("--no-color")
            .arg("--unified=3");
        if cached {
            command.arg("--cached");
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_PAGER", "cat")
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| UiError::new("workspace", "未找到可用的 Git 命令。"))?;
        let mut bytes = Vec::with_capacity(MAX_DIFF_OUTPUT);
        child
            .stdout
            .take()
            .ok_or_else(|| UiError::new("workspace", "无法读取 Git 差异输出。"))?
            .take((MAX_DIFF_OUTPUT + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| UiError::new("workspace", "Git 差异读取失败。"))?;
        let truncated = bytes.len() > MAX_DIFF_OUTPUT;
        if truncated {
            let _ = child.kill().await;
            let _ = child.wait().await;
            bytes.truncate(MAX_DIFF_OUTPUT);
        } else if !child
            .wait()
            .await
            .map_err(|_| UiError::new("workspace", "Git 差异检查中断。"))?
            .success()
        {
            return Err(UiError::new("workspace", "Git 差异读取失败。"));
        }
        Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
    };
    timeout(Duration::from_secs(10), read)
        .await
        .map_err(|_| UiError::new("workspace", "Git 差异检查超时，已停止读取。"))?
}

fn rule_candidates(root: &Path) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    for name in ["AGENTS.md", "CLAUDE.md", ".clinerules", "CONTRIBUTING.md"] {
        candidates.push(root.join(name));
    }
    for directory in [".cursor/rules", ".windsurf/rules", ".clinerules"] {
        let path = root.join(directory);
        if !std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_dir()) {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten().take(MAX_RULE_FILES) {
                let item = entry.path();
                if item.is_file()
                    && item.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("mdc")
                    })
                {
                    candidates.push(item);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn read_rules(root: &Path) -> Result<Vec<RuleFile>, UiError> {
    let mut total = 0usize;
    let mut files = Vec::new();
    for path in rule_candidates(root) {
        if files.len() >= MAX_RULE_FILES || total >= MAX_RULE_TOTAL {
            break;
        }
        match std::fs::symlink_metadata(&path) {
            Ok(value) if value.file_type().is_file() => {}
            _ => continue,
        }
        let allowed = (MAX_RULE_TOTAL - total).min(MAX_RULE_FILE);
        let file = std::fs::File::open(&path)
            .map_err(|_| UiError::new("workspace", "项目规则文件无法读取。"))?;
        let mut bytes = Vec::new();
        let mut limited = std::io::Read::take(file, (allowed + 1) as u64);
        std::io::Read::read_to_end(&mut limited, &mut bytes)
            .map_err(|_| UiError::new("workspace", "项目规则文件无法读取。"))?;
        let truncated = bytes.len() > allowed;
        bytes.truncate(allowed);
        total += bytes.len();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        files.push(RuleFile {
            path: relative,
            bytes: bytes.len(),
            truncated,
            content: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
    Ok(files)
}

async fn session_root(
    settings: &crate::Settings,
    session_id: &str,
    expected_profile: &str,
) -> Result<std::path::PathBuf, UiError> {
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
        .ok_or_else(|| UiError::new("workspace", "该会话没有可核对的工作区目录。"))?;
    Ok(valid_root(cwd)?.to_path_buf())
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
    let root = session_root(&settings, &session_id, &expected_profile).await?;
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

#[tauri::command]
pub async fn workspace_git_diff(
    session_id: String,
    expected_profile: String,
    state: tauri::State<'_, crate::Desktop>,
) -> Result<Value, UiError> {
    if session_id.is_empty() || session_id.len() > 256 || expected_profile.is_empty() {
        return Err(UiError::new("workspace", "会话身份无效，请刷新会话列表。"));
    }
    let settings = state.settings.lock().unwrap().clone();
    let root = session_root(&settings, &session_id, &expected_profile).await?;
    tokio::task::spawn(async move {
        let (staged, staged_truncated) = git_diff(&root, true).await?;
        let (unstaged, unstaged_truncated) = git_diff(&root, false).await?;
        Ok(json!({
            "staged": staged,
            "stagedTruncated": staged_truncated,
            "unstaged": unstaged,
            "unstagedTruncated": unstaged_truncated,
        }))
    })
    .await
    .map_err(|_| UiError::new("workspace", "Git 差异读取任务中断。"))?
}

#[tauri::command]
pub async fn workspace_rules(
    session_id: String,
    expected_profile: String,
    state: tauri::State<'_, crate::Desktop>,
) -> Result<Value, UiError> {
    if session_id.is_empty() || session_id.len() > 256 || expected_profile.is_empty() {
        return Err(UiError::new("workspace", "会话身份无效，请刷新会话列表。"));
    }
    let settings = state.settings.lock().unwrap().clone();
    let root = session_root(&settings, &session_id, &expected_profile).await?;
    tokio::task::spawn_blocking(move || Ok(json!({"files": read_rules(&root)?})))
        .await
        .map_err(|_| UiError::new("workspace", "项目规则读取任务中断。"))?
}

#[cfg(test)]
mod tests {
    use super::{git, git_diff, parse_status, read_rules, MAX_DIFF_OUTPUT};
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

    #[tokio::test]
    async fn reads_staged_and_unstaged_diffs_without_changing_the_index() {
        use std::{fs, process::Command};
        let root = std::env::temp_dir().join(format!(
            "hsp-git-diff-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap()
        };
        assert!(run(&["init"]).status.success());
        assert!(run(&["config", "user.name", "Test User"]).status.success());
        assert!(run(&["config", "user.email", "test@example.invalid"])
            .status
            .success());
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        fs::write(root.join("staged.txt"), "base staged\n").unwrap();
        fs::write(root.join("unstaged.txt"), "base unstaged\n").unwrap();
        assert!(run(&["add", "tracked.txt", "staged.txt", "unstaged.txt"])
            .status
            .success());
        assert!(run(&["commit", "-m", "baseline"]).status.success());
        fs::write(root.join("staged.txt"), "INDEX_ONLY\n").unwrap();
        assert!(run(&["add", "staged.txt"]).status.success());
        fs::write(root.join("unstaged.txt"), "WORKTREE_ONLY\n").unwrap();
        let index_before = fs::read(root.join(".git/index")).unwrap();
        let (staged, staged_truncated) = git_diff(&root, true).await.unwrap();
        let (unstaged, unstaged_truncated) = git_diff(&root, false).await.unwrap();
        assert!(!staged_truncated && !unstaged_truncated);
        assert!(staged.contains("INDEX_ONLY"));
        assert!(!staged.contains("WORKTREE_ONLY"));
        assert!(unstaged.contains("WORKTREE_ONLY"));
        assert!(!unstaged.contains("INDEX_ONLY"));
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn caps_large_diff_output() {
        use std::{fs, process::Command};
        let root = std::env::temp_dir().join(format!(
            "hsp-git-diff-limit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap()
        };
        assert!(run(&["init"]).status.success());
        assert!(run(&["config", "user.name", "Test User"]).status.success());
        assert!(run(&["config", "user.email", "test@example.invalid"])
            .status
            .success());
        fs::write(root.join("large.txt"), "old\n").unwrap();
        assert!(run(&["add", "large.txt"]).status.success());
        assert!(run(&["commit", "-m", "baseline"]).status.success());
        fs::write(
            root.join("large.txt"),
            format!("{}\n", "x".repeat(MAX_DIFF_OUTPUT * 2)),
        )
        .unwrap();
        let (diff, truncated) = git_diff(&root, false).await.unwrap();
        assert!(truncated);
        assert!(diff.len() <= MAX_DIFF_OUTPUT);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_only_bounded_known_rule_files() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("hsp-rules-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".cursor/rules")).unwrap();
        fs::write(root.join("AGENTS.md"), "root rule").unwrap();
        fs::write(root.join(".cursor/rules/project.mdc"), "cursor rule").unwrap();
        fs::write(root.join("secret.txt"), "must not be read").unwrap();
        let files = read_rules(&root).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|item| item.path == "AGENTS.md"));
        assert!(files
            .iter()
            .any(|item| item.path == ".cursor/rules/project.mdc"));
        assert!(files.iter().all(|item| !item.content.contains("must not")));
        fs::remove_dir_all(root).unwrap();
    }
}
