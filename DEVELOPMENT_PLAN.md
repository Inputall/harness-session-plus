# Harness Session Plus — Rust 开发方案

本文是交给 Codex 执行的项目任务书。将文件放到 `harness-session-plus` 项目根目录，然后要求 Codex 按本文分阶段开发。

## 1. 项目目标

开发一个 Windows 外部增强工具，通过 Chrome DevTools Protocol（CDP）连接 DeepSeek Harness，在不修改其安装包和源码的前提下提供会话回收站。

核心业务、CDP、持久化、CLI、桌面宿主全部使用 Rust。Harness 页面注入使用必要的 JavaScript；桌面管理界面优先使用简单 HTML/CSS/JavaScript。JavaScript 只负责界面，不实现文件清理或持久化业务。不使用 Node.js 服务或 Electron。

功能目标：

- 会话菜单增加“移入回收站”。
- 侧栏增加回收站入口，打开工具自己的管理窗口。
- 展示标题、原工作区、移入时间和操作状态。
- 单项及批量恢复。
- 经验证后提供永久删除。
- 托盘、连接状态、断线重连、重新注入和卸载注入。
- 不兼容时禁用数据操作并显示原因。

第一版不自动清空回收站，不删除项目目录或 Git 工作树，不自动终止运行任务。

## 2. 已验证事实与未知项

### 已验证

用户在自己的 Windows 环境完成了以下验证：

```text
Harness 安装路径：
C:\Users\Administrator\AppData\Local\Programs\DeepSeek Harness\DeepSeek Harness.exe

启动参数：
--remote-debugging-port=9223
--remote-debugging-address=127.0.0.1

CDP HTTP：
http://127.0.0.1:9223/json/version
http://127.0.0.1:9223/json/list

页面特征：
type = page
url = dsh-app://app/
```

- `/json/version` 返回过 `Chrome/152.0.7977.54`；这是观测值，不是兼容性要求。
- 页面目标的 WebSocket 可以通过 `/json/list` 获取；ID 和地址每次可能变化，不能硬编码。
- `Runtime.evaluate` 可以读取页面文字。
- 使用 `MutationObserver` 定位包含“归档会话”的 `[role="menu"]`，成功添加了“删除会话（测试）”。
- 测试按钮只显示提示，没有执行数据操作。

### 必须探测

- 真实会话 ID、工作区 ID，以及菜单与 ID 的对应关系。
- 多实例和不同 profile 的持久化身份。
- 运行状态的可靠来源。
- 会话列表、归档、恢复及永久删除能力。
- 本地数据库、日志、缓存、附件引用和同步机制。

不得编造 Harness API、表名、文件路径或 IPC 方法。标题和 DOM 序号不能用于标识删除对象。不能把模拟测试描述成真实应用验证。

## 3. 技术架构

```text
Tauri 管理窗口 / Windows 托盘
                  │ 类型明确的 commands/events
                  ▼
Rust core ── SQLite 操作记录与回收站
   │
   ├── CDP 连接与目标发现 ── Harness 注入脚本
   │                            │ 固定动作桥接
   └── Harness 版本适配器 ◀─────┘
                │
                └── 已验证的归档 / 恢复 / 删除能力
```

建议 Cargo workspace：

| 组件 | 技术与职责 |
| --- | --- |
| core | Rust library，业务与适配器，不依赖 Tauri |
| CLI | Rust + clap，诊断、注入与卸载 |
| 异步运行时 | tokio |
| HTTP 发现 | reqwest，仅访问明确配置的本机地址 |
| WebSocket | tokio-tungstenite + futures-util |
| 数据结构 | serde + serde_json |
| 数据库 | rusqlite，放在独立数据库工作线程，避免阻塞异步运行时 |
| 日志与错误 | tracing、thiserror；CLI 边界可使用 anyhow |
| 桌面 | Tauri 2，Windows WebView2 |
| 管理界面 | 原生 HTML/CSS/JavaScript，可使用轻量构建工具，但不引入 Node 后端 |
| 测试 | cargo test、临时数据库和本机 mock HTTP/WebSocket 服务 |

开发时核对官方文档、工具链及依赖兼容性，提交 Cargo.lock。不要将本文视为对任何最新 crate API 的保证。

## 4. 目录结构

```text
harness-session-plus/
├── DEVELOPMENT_PLAN.md
├── Cargo.toml
├── Cargo.lock
├── README.md
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── cdp/
│   │   │   ├── adapter/
│   │   │   ├── trash/
│   │   │   ├── db/
│   │   │   ├── diagnostics/
│   │   │   └── lib.rs
│   │   └── tests/
│   └── cli/
│       ├── Cargo.toml
│       └── src/main.rs
├── desktop/
│   ├── src-tauri/
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   ├── capabilities/
│   │   └── src/
│   └── ui/
├── injected/
│   ├── bootstrap.js
│   ├── menu.js
│   └── sidebar.js
├── migrations/
├── fixtures/
└── docs/
    ├── discovery.md
    ├── architecture.md
    ├── compatibility.md
    └── manual-validation.md
```

注入脚本作为构建资源嵌入，不从网络动态下载代码。可通过 `include_str!` 加载经过验证的 bundle。

## 5. 开发阶段与验收

### A：Rust CDP 诊断与测试注入

先实现命令行，不依赖桌面窗口：

```powershell
cargo run -p hsp-cli -- diagnose --port 9223
cargo run -p hsp-cli -- inject-test --port 9223
cargo run -p hsp-cli -- remove-injection --port 9223
```

要求：

1. 获取 version、targets，选择 `dsh-app://app/` 页面。多个匹配页面时要求明确选择，不默认选首个。
2. 每个请求使用唯一 ID，按 ID 关联响应；接收任务分发响应和事件。
3. 支持超时、连接关闭、取消、待处理请求失败回传。
4. 处理页面导航和 execution context 重建；避免复用失效 context。
5. 输出端口、浏览器版本、页面识别结果和注入状态，不默认输出正文或完整调试地址。
6. 注入测试按钮，点击只提示；重复注入不重复安装。
7. 卸载所有按钮、样式、observer、事件监听和计时器。
8. 重连使用有上限的退避；不能无限高速重试。

验收：用 CLI 完成用户此前的 PowerShell 验证，并验证卸载和再次注入。

### B：会话身份与原生能力探测

先检查公开源码、已加载前端资源和页面结构；必要时通过 CDP 观察网络、WebSocket 或经过核实的桥接。不得凭空选择 HTTP 作为应用唯一通信机制。

创建专用测试会话，使用原生归档、恢复操作观察行为，不操作现有重要会话。

交付 `docs/discovery.md`：每种能力的证据、调用结构、成功判断、失败判断、版本信息，以及未知项。所有样例脱敏，不记录聊天正文或认证信息。

验收：两个同名会话可准确区分；能够确认菜单操作对应的真实 ID；运行状态未知时禁用变更。没有恢复证据时不得进入真实回收站操作。

### C：单会话回收站闭环

优先采用“原生归档 + 工具回收站元数据”，但仅限原生归档已验证可恢复的版本。

移入：核对对象与空闲状态 → 保存操作意图 → 执行归档 → 查询确认 → 更新记录 → 刷新界面。

恢复：核对实例、记录和目标工作区 → 保存操作意图 → 恢复 → 查询确认 → 更新记录。

- 工具只收录自己移入的会话，不把用户所有归档会话视为回收站。
- 原工作区消失时让用户选择已验证可用的目标；不静默创建工作区。
- 当前会话移入后通过已验证的导航能力离开该会话，不硬编码未证实路由。
- 第一版拒绝运行中及状态未知的会话。
- 原生归档无法恢复时停用该策略，交付诊断能力；不能以隐藏 DOM 冒充数据回收站。

验收：专用测试会话可移入、重启后可见、恢复后回到正确工作区；同名会话不受影响。

### D：永久删除与批量处理

只有发现可靠删除能力后才启用永久删除。缺少能力时保持按钮禁用，明确原因。

- 仅删除回收站条目，确认弹窗展示目标和不可恢复提示。
- 执行前重新确认实例身份、ID、状态及回收站归属。
- 同一实例和会话串行处理，批量操作逐项显示结果。
- 部分失败保留记录，不报告整批成功。
- 处理原生索引、日志和缓存，以实际机制为准。
- 不删除工作目录、Git 工作树或无法证明独占的附件。
- 不将“找不到列表项”视为彻底清理数据的充分证据。

若必须直接修改存储，将其作为独立适配阶段：先确认格式、引用关系、锁和缓存机制，再建立备份与恢复验证。不能默认在应用运行时直接改库，也不能未经评估复制正在写入的数据库作为有效备份。

### E：Tauri 桌面产品化

- 托盘：连接状态、打开回收站、连接/断开、暂停增强、退出。
- 窗口：回收站搜索、多选、恢复、永久删除、逐项结果。
- 配置：可执行文件路径、CDP 端口、目标实例、诊断开关。
- 使用 Rust `std::process::Command` 传递参数，不拼接 shell 命令。
- 已运行但没有调试端口时，提示保存工作并提供用户主动重启流程；不强杀应用。
- 退出工具默认保留 Harness；可连接时卸载注入，连接丢失时说明刷新或重启可清除注入。
- 使用 Tauri commands/events；通常不需要额外 HTTP 管理服务。
- 构建 Windows 安装包并说明 WebView2 需求；未签名安装包明确标记，不宣称已签名。

## 6. Rust 领域模型和适配器

以下为工具内部设计，不是 Harness 已存在的接口：

```rust
pub enum ActivityState { Idle, Running, Unknown }

pub struct SessionKey {
    pub instance_key: String,
    pub session_id: String,
}

pub struct SessionSummary {
    pub key: SessionKey,
    pub title: String,
    pub workspace_id: Option<String>,
    pub activity: ActivityState,
}

pub struct AdapterCapabilities {
    pub list: bool,
    pub archive: bool,
    pub restore: bool,
    pub permanently_delete: bool,
    pub activity_state: bool,
}
```

使用 async trait 或等效 Rust 接口封装：

```text
identify_instance()
capabilities()
list_sessions()
get_session(key)
archive_session(key)
restore_session(key, target_workspace)
permanently_delete_session(key)
```

统一返回类型化错误：Disconnected、Unsupported、Busy、UnknownActivity、IdentityMismatch、TargetNotFound、VerificationFailed 等。

实例身份应基于已验证的 profile 或数据目录标识；端口、PID、页面 ID 都不是持久身份。身份不能确认时，不关联旧回收站记录并禁止变更。

能力探测失败不等于目标不存在。数据库、CDP 和 Harness 操作不组成原子事务，必须通过操作日志与重查收敛状态。

## 7. 持久化、状态机和故障恢复

建议表：

| 表 | 字段 |
| --- | --- |
| instances | instance_key、已验证身份元数据、adapter_version |
| trash_entries | instance_key、session_id、workspace_id、title、trashed_at、status、last_error |
| operations | operation_id、instance_key、session_id、kind、state、started_at、completed_at、error_code |
| schema_migrations | version、applied_at |

`trash_entries` 的 `(instance_key, session_id)` 唯一。时间用 UTC 持久化，界面按本地时区显示。

状态：pending_trash、trashed、pending_restore、restored、pending_delete、deleted、needs_attention。

先在数据库事务里保存意图，再执行外部操作，最后根据重新查询更新结果。不要在外部调用期间持有长数据库事务。

启动时核对未完成操作：

- 已归档：补记移入成功。
- 已恢复：更新恢复结果。
- 无法确认：标记需要处理，禁止盲目重复非幂等调用。
- 永久删除中断：根据适配器证据核对各清理步骤，缺少证据则保持失败状态。

数据库默认放到工具自己的应用数据目录；日志轮转且默认不含标题、正文、路径和令牌。标题在回收站数据库中属于必要元数据，可在诊断导出时脱敏。

## 8. 页面注入与桥接

使用 `window.__HARNESS_SESSION_PLUS__` 保存安装状态，提供 install、uninstall、getStatus。

- 样式与 DOM 使用独立前缀。
- observer 合并调度，尽量观察菜单/侧栏容器，不高频扫描全文。
- 菜单打开时解析真实 ID，点击后 Rust 再核对，不信任页面 payload。
- 不向 Harness 页面暴露 Rust 文件系统或任意命令执行。
- 可以使用 CDP `Runtime.addBinding` 发送固定动作，例如 request_trash、open_recycle_bin。
- Rust 校验 binding 事件的目标、execution context、payload 长度、动作白名单和会话身份。
- 真正永久删除必须由工具管理窗口确认，不允许注入页面直接无确认删除。
- 注入参数通过 JSON 序列化传入，禁止直接拼接用户标题为代码。
- 工具窗口 IPC 独立于 Harness binding；不要把任意外部页面接入 Tauri capabilities。

## 9. 连接和权限

- CDP 默认仅使用 `127.0.0.1`，不开放局域网监听。
- 端口可配置，检测占用和错误应用，核对页面 URL 后连接。
- 验证实际监听地址，不只相信启动参数。
- CDP 拥有较强控制能力，仅用于本机；工具不自动向外转发端口。
- 不要求管理员权限、不上传聊天数据。
- Tauri 使用最小 capabilities 和 CSP，不启用不必要的 shell、filesystem 或远程来源权限。
- 未知版本、未知实例身份和无法确认的状态均禁用数据操作，诊断及卸载仍可用。

## 10. 测试与真实验收

必要自动化测试：

- CDP 响应乱序、事件混入、超时和断线。
- 两个同名会话不会误操作。
- 重复点击只触发一个操作。
- 成功外部操作后崩溃，重启能核对。
- 失败不标记成功，未知结果不盲目重试。
- 不支持能力、身份不匹配和未知运行状态拒绝变更。
- 数据库迁移和操作记录完整性。

真实验收只使用专用测试会话，记录执行环境和结果：

- 移入、恢复、重启后状态一致。
- 当前会话移入后的界面行为。
- 原工作区不存在。
- 活动会话拒绝操作。
- 连接断开、页面刷新、重新注入和卸载。
- 永久删除不影响其他会话；核验不能仅检查 DOM。

常规检查：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

根据当前阶段执行相关检查；桌面构建命令在真实工具链配置后写入 README，不编造已通过结果。

## 11. Codex 工作要求与交付物

1. 先检查项目及适用 AGENTS.md，复用已有实现。
2. 分阶段创建可运行代码，先 A/B，后真实数据操作。
3. 可以独立完成的代码、文档和模拟测试应持续完成；需要用户目标环境时提供简短命令和明确输出要求。
4. 不因缺少真实删除接口而停止全部开发；交付已验证功能，并明确禁用不支持功能。
5. 不安装或依赖此前讨论的第三方删除插件。若参考公开实现，先核实仓库、源码、许可证和兼容性。
6. 不自动启用开机启动、遥测或后台数据清理。
7. 报告已完成行为、验证证据、未验证能力及限制。

交付：源码、Cargo.lock、CLI、Tauri 管理窗口、适配器证据文档、测试结果、手动验收记录、Windows 构建/安装说明；只有真实打包完成才提供安装包。

## 12. 推荐给 Codex 的首次执行指令

```text
读取项目根目录 DEVELOPMENT_PLAN.md，按 Rust 方案实际开发。
本次先完成阶段 A：Cargo workspace、Rust CDP 客户端、diagnose、
inject-test 和 remove-injection，编写必要测试并完成检查。
随后推进阶段 B，生成真实会话身份、归档与恢复能力探测报告。
目标 Harness 的 CDP 端口为 9223；操作前重新确认连接和页面。
没有真实环境时完成可独立验证的部分，提供最短手动验证步骤。
不要修改 Harness 安装包，不要编造接口，不操作现有重要会话，
本次不执行真实永久删除。所有数据操作只使用专用测试会话。
```
