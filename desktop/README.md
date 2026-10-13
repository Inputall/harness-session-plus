# Harness Session Plus 桌面版

当前版本 **0.2.2**，Windows Tauri 2 桌面程序。Rust 宿主与静态 HTML/CSS/JavaScript 前端，已完成 release 构建，当前提供独立 EXE，没有安装包或代码签名。

完整功能、兼容性、调试参数和故障排查见[项目 README](../README.md)。

## 构建

需要 Windows Rust MSVC、C++ 构建工具与 Windows SDK、WebView2 Runtime。首次构建需要下载 Cargo 依赖。在项目根目录执行：

```powershell
cargo build --manifest-path desktop/src-tauri/Cargo.toml --release --locked
& .\desktop\src-tauri\target\release\hsp-desktop.exe
```

前端嵌入程序，不需要 npm 或 Node 后端。桌面包拥有独立 Cargo.lock，根 workspace 构建不会生成桌面 EXE。更新时先从托盘退出旧管理工具。

## 使用

1. 在设置中配置 Harness 程序路径和端口。
2. 若 Harness 未开启调试端口，先正常退出，再在“连接与增强”点击“启动 Harness”。
3. 检查连接，在“会话回收站”点击“安装回收站增强”。
4. 重新打开 Harness 会话菜单，使用“移入回收站”。管理窗口支持搜索、单项及批量恢复。

保持工具运行。关闭窗口隐藏到托盘，退出时尝试卸载增强，不结束 Harness。页面刷新或 Harness 重启后需重新安装。

## 功能和边界

- 概览、连接诊断、增强卸载、主动启动 Harness。
- 按当前 profile 隔离的 SQLite 回收站与操作日志。
- 真实 ID 核验，运行或未知状态拒绝操作；结果不明时不自动重放。
- 可选每 15 秒检查连接，连续 3 次失败暂停。
- 明暗主题、明确页面选择、报告导出与系统托盘。
- 概览页 CoderPlant · 码源推荐卡片，支持明暗主题和窄窗口布局；点击后使用默认浏览器打开固定官网地址。
- 永久删除保持禁用，删除能力检查仅只读显示原因。

原生回收站只适配已验证安装包的 Harness 0.2.0-rc.2，见[验证记录](../docs/recycle-validation.md)和[删除审计](../docs/permanent-delete-audit.md)。

## 数据和权限

Tauri `app_config_dir` 保存 settings.json、recycle.sqlite3 和用户主动导出的 capability-report.json。回收站记录包含 ID、标题、工作区和操作状态，不保存聊天正文。

固定 Rust commands 执行业务。管理前端不直接修改 Harness 文件，没有 shell/filesystem 插件；CSP 禁止远程脚本。页面 JS 用于界面和版本受约束的接口桥接，不负责文件清理或持久化。

推荐卡片不会自动请求第三方网站；打开官网无需 Harness 连接，也不会向链接添加会话、profile 或工作区信息。第三方模型、价格、套餐和服务承诺以官网当前说明为准。

## 更新和排查

1. 从托盘退出旧版 Harness Session Plus，保留 Harness 正常运行。
2. 构建并启动新版 `desktop/src-tauri/target/release/hsp-desktop.exe`。
3. 检查连接，重新安装回收站增强，并重新打开 Harness 会话菜单。

如果 Harness 没有开启调试端口，仍需先正常退出 Harness，再带参数重启。普通运行的实例不能仅靠安装增强开启调试端口。

侧栏显示的是桌面版本；注入脚本有独立版本号。桌面 0.2.2 使用回收站脚本 0.2.0 属于正常情况。其他问题见[主 README 常见问题](../README.md#常见问题)。

## 检查

```powershell
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --locked --all-targets -- -D warnings
node --test fixtures/desktop-ui.test.cjs fixtures/injection.test.cjs fixtures/recycle-menu.test.cjs
```

桌面构建和 Clippy 已通过，前端测试是 DOM/IPC 模拟测试。真实 Harness 专用同名会话归档与恢复闭环已通过，不代表所有窗口、托盘、断线和升级场景均已完成真实验收。直接在浏览器打开 HTML 不能使用桌面功能。
