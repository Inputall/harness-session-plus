# 原生回收站实现与本机验证

当前适配对象为经过核验的 Windows Harness 安装包：

- 0.2.0-rc.2：`983ca71114e6dfd353fc79af5a1f9481a250ee64c2a3c757673029b811b23bc2`
- 0.2.1-rc.2：`47874618bf5b3011fcaac10cd4c85f69da333e70d4f99b921207c78d0999512a`

以本机安装资源为准核对：`dsh-api-workspace-controller` 的 `workspace/archiveSession`、`workspace/unarchiveSession`、`workspace/follow`；`dsh-api-session-controller` 的 `session/list`；`dsh-api-gateway` 的 `/api` unary RPC 与 `/api/remote.mux` 流协议。请求封装为实际协议的 `client-request` / `payload.args`，校验响应关联 ID。源码提取在 `target/source-evidence/`，只读安装包，未更改 Harness。

会话菜单通过当前按钮的 React fiber 链中唯一 `sessionId`，再与 `data-row-key=session:<id>` 核对。标题不能作身份。多 ID、无 ID、多行匹配、菜单身份变化均拒绝操作。该结构属于版本适配细节；安装包变化必须重新验证，不能泛化兼容。

Rust 核实 9223 监听进程、实际 exe 路径、安装包摘要，以及该进程唯一 backend 子进程的 profile 参数。PowerShell 只负责读取 Windows 进程事实，不负责归档或持久化。记录按规范化 profile 隔离；PID 只用于当前运行代次核验，不作为持久身份。原生 host origin 在读取前后及变更前再次核对。

独立 SQLite `recycle.sqlite3` 位于工具应用配置目录。先事务写入操作意图与 operation journal，然后调用原生接口，再独立读取归档集合。归档请求不携带 `stopActivity`，由原生 registry 在写入前检查所有活动 provider；已知运行或未知运行状态也在工具中被拒绝。恢复只允许工具收录、归档状态确认、原工作区一致的 ID。断线或结果不明不会自动重放，重新连接后只读核对。

菜单与侧栏入口在管理工具运行时有效；工具通过本机 CDP 定时取固定动作队列，页面心跳超过 5 秒拒绝新的动作。重新安装清除旧队列，防止旧请求重放。页面刷新需要重新安装。退出尝试卸载，失败仍保留原有清理提示。

2026-10-12 本机验证：

- 原生只读列表、工作区 baseline 成功。
- 用户保持菜单打开后，证明该菜单只有一个真实会话 ID 且与侧栏行一致；没有修改该原会话。
- `cargo run -p hsp-core --example recycle_acceptance --offline` 创建两个“ HSP 回收站验收专用 ”同名测试会话，仅归档其中指定 ID，检查另一 ID 未归档，再恢复指定会话。SQLite journal 与独立原生读回全部通过。
- 两个测试会话保留在原工作区，便于复查；没有发送消息或执行模型任务，没有永久删除。
- Rust 与 JS 测试是自动测试；实际归档与恢复闭环是以上本机专用会话验证。桌面点击/视觉验收仍需打开新版 exe。

未实现永久删除：界面按钮禁用，没有删除 endpoint、清理会话文件或改写 Harness 数据库。

2026-10-21 对本机 0.2.1-rc.2 安装包进行只读复核：主程序路径与 9223 监听进程一致，backend 唯一子进程携带规范 `profiles\\desktop` 参数；会话与工作区控制器的 `archiveSession` / `unarchiveSession` RPC 契约及 workspace registry 归档集合与此前核验版一致。新增摘要进入严格白名单。此次只读检查没有执行归档、恢复或删除操作。

使用：正常退出旧 Harness Plus，打开 `desktop/src-tauri/target/release/hsp-desktop.exe`，在“会话回收站”点击“安装回收站增强”，重新打开 Harness 会话菜单。回收站支持单项恢复、选择批量恢复、标题/工作区搜索及手动刷新。

最终桌面 0.2.0 已实际 release 构建成功；core/desktop Clippy 零警告。自动测试为 12 项 Rust 与 14 项 JS，另有上述真实专用会话闭环。旧管理工具的进程未强制结束，备份为 hsp-desktop.pre-recycle.exe。
