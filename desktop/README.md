# Harness Session Plus 桌面版

当前源码版本 **0.9.0（开发中）**，最近发布桌面版为 0.8.0。Windows Tauri 2 桌面程序。Rust 宿主与静态 HTML/CSS/JavaScript 前端，提供独立 EXE，没有安装包或代码签名。

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

关闭窗口会隐藏到托盘。回收站菜单增强在页面刷新或 Harness 重启后需重新安装。退出管理工具不会清除已应用的 Harness 主题：独立主题守护会按本机保存的配置恢复样式；Harness 调试端口必须可访问，断线恢复通常有数秒延迟。电脑重启、用户注销或手动结束守护后，需重新打开工具启动它，本版不设置开机自启。点击“恢复 Harness 默认”会停止守护并清除样式覆盖。

## 功能和边界

- 设置页“安全备份与迁移”：导出工具配置、主题和会话检查点，可选回收站管理记录及本机连接路径；恢复前校验清单与 SHA256、显示覆盖预览，并在下次启动时自动备份与恢复。失败或中断后回滚，回滚不完整时停止启动。详见[备份说明](../docs/backup-migration.md)。
- 脱敏诊断导出及备份目录入口；工具数据迁移不包含 Harness 会话正文和项目文件。
- 概览、连接诊断、增强卸载、主动启动 Harness。
- 按当前 profile 隔离的 SQLite 回收站与操作日志。
- 真实 ID 核验，运行或未知状态拒绝操作；结果不明时不自动重放。
- 会话检查点：保存本机元数据和继续提示词，恢复前重新核对 profile、真实会话 ID、标题、工作区及归档状态；不会发送消息、恢复聊天正文或修改项目文件。
- 会话收藏：按 profile 保存最多 200 个常用会话的真实 ID、标题和工作区，支持收藏筛选；不保存正文，包含在工具备份中。
- 会话收藏：按 profile 保存最多 200 个常用会话的真实 ID、标题和工作区，支持收藏筛选；不保存正文，包含在工具备份中。
- Git 工作区只读面板：按所选会话目录显示当前分支、暂存/未暂存/未跟踪文件和最近 5 次提交；不提交、暂存、切换或重置。
- 模型、用量、权限与协作模式面板：只读显示 Harness 持久化模型路由、提供方 Token 汇总、上下文压力、当前会话权限预设和计划模式状态。
- 计划模式快捷命令：依据当前会话计划投影复制 `/plan` 或 `/plan off`，仅供用户回到 Harness 手动粘贴发送。
- 会话技能清单：读取当前会话可调用技能的元数据和模型调用标记；不加载正文、不执行或修改技能。
- 后台任务列表：按会话读取 Harness 原生任务名册首个快照并允许手动刷新；不读取/消费输出、不启动或停止任务。
- 多会话比较：在同一 Harness profile 中选择两个会话，按需并排读取状态、最近模型、权限预设、计划模式和持久化 Token 汇总；只读、缺失投影标记为不可用，切换 profile 或刷新列表时清除选择。
- 可选每 15 秒检查连接，连续 3 次失败暂停。
- 明暗主题、明确页面选择、报告导出与系统托盘。
- 概览页 CoderPlant · 码源推荐卡片，支持明暗主题和窄窗口布局；点击后使用默认浏览器打开固定官网地址。
- 永久删除保持禁用，删除能力检查仅只读显示原因。
- Harness 皮肤中心：四套配色、40 款转换主题，支持轻量预览、搜索分类分页、背景清晰度调节（0–100%，默认 70%）、应用及恢复默认。DreamSkin 社区支持联网分页浏览及按需核验安装。手动导入 `schemaVersion: 1` 的主题 JSON（64 KiB）和可选 PNG/JPEG/WebP（5 MiB），最多保存 20 个，社区主题不占此名额。应用后会在 `active-theme.json` 保存主题、壁纸、清晰度和恢复版本；独立后台守护每三秒检查样式，在 Harness 页面断线重连或覆盖丢失时尝试恢复。仅在唯一页面匹配时应用，不修改会话、Harness 安装包或原生主题偏好。完整退出并重启 Harness 的恢复流程尚未实测；电脑重启后需打开工具启动守护，本版不设置开机自启。

原生回收站只适配已验证安装包的 Harness 0.2.0-rc.2，见[验证记录](../docs/recycle-validation.md)和[删除审计](../docs/permanent-delete-audit.md)。

## 数据和权限

Tauri `app_config_dir` 保存 settings.json、active-theme.json、recycle.sqlite3、session-checkpoints.json、session-favorites.json、imported-themes.json、community-themes.json、community-themes/ 和用户主动导出的 capability-report.json。回收站记录包含 ID、标题、工作区和操作状态；收藏只保存会话标识元数据；均不保存聊天正文。主题守护独立于管理窗口，但不会随 Windows 开机自动启动。

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
node --test fixtures/desktop-ui.test.cjs fixtures/skin.test.cjs fixtures/injection.test.cjs fixtures/recycle-menu.test.cjs
```

当前源码通过 desktop tests、Clippy 与前端测试；前端测试是 DOM/IPC 模拟测试。真实 Harness 专用同名会话归档与恢复闭环已通过。主题守护已验证管理工具退出后的主题保持及覆盖丢失恢复，但完整退出并重启 Harness 的流程尚未实测。会话检查点已通过本机持久化、界面 IPC 与恢复身份守卫的自动化测试，尚未在用户 Harness 桌面上完成端到端人工验收。备份恢复使用合成数据测试，尚未对真实用户配置执行恢复。直接在浏览器打开 HTML 不能使用桌面功能。

主题只转换配色与图片，不执行包内 CSS 或脚本；ZIP 需先解压。作者及许可见[素材说明](ui/themes/README.md)，真实配色与壁纸验收见[皮肤验证](../docs/skin-validation.md)。
