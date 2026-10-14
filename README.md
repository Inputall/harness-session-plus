# Harness Session Plus

为 Windows 版 DeepSeek Harness 提供会话回收站的独立桌面工具。通过本机 Chrome DevTools Protocol（CDP）添加菜单增强，使用 Harness 原生归档与恢复机制保留会话数据，在管理窗口中搜索和恢复会话。

当前桌面版本：**0.5.3**。采用 Rust + Tauri 2，前端使用 HTML/CSS/JavaScript，没有 Node 后端或 Electron 宿主，不修改 Harness 安装包。

> “移入回收站”是可恢复的原生归档。**永久删除尚未实现，按钮保持禁用**；“检查删除能力”只读取状态，不删除文件或会话。

## 阅读导航

- 首次使用：[快速开始](#快速开始)、[管理窗口](#管理窗口)
- 连接或菜单异常：[常见问题](#常见问题)
- 数据保存与恢复：[操作保护和数据保存](#操作保护和数据保存)
- 开发与验收：[开发和验证](#开发和验证)、[项目结构](#项目结构)
- 本次更新：[版本记录](#版本记录)

## 功能

| 功能 | 状态 |
| --- | --- |
| 会话菜单“移入回收站” | 核对真实会话 ID 后执行原生归档 |
| Harness 侧栏回收站入口 | 打开本工具的管理窗口 |
| 回收站列表 | 展示标题、原工作区、移入时间、操作状态 |
| 搜索、单项及批量恢复 | 已实现，批量逐项处理 |
| 操作记录 | 独立 SQLite，按 Harness profile 隔离 |
| 连接诊断、注入与卸载 | 已实现，仅连接配置的本机端口 |
| 托盘、明暗主题 | 已实现，关闭窗口隐藏到托盘 |
| 官网推荐卡片 | 概览页显示 CoderPlant · 码源，点击后用默认浏览器打开官网 |
| Harness 皮肤中心 | 四套配色 + 40 款转换主题、背景清晰度调节、社区按需安装及恢复默认 |
| 主题保持 | 管理工具退出后由独立守护进程按已保存配置恢复页面主题 |
| 结构探测、删除能力检查 | 只读，不执行删除 |
| 永久删除、自动清空 | 未实现 |

只收录**由本工具移入**的会话，不接管 Harness 自行归档的记录。同名会话按真实 ID 区分，标题不能作为身份。不会自动停止任务，不删除项目目录或 Git 工作树。

概览页的推荐卡片链接至 [CoderPlant · 码源](https://coderplant.com/)。卡片不自动加载第三方内容，点击后由系统默认浏览器打开固定官网地址，不携带会话、profile 或工作区信息。模型、价格、套餐及服务承诺以官网当前说明为准。

## 兼容性

当前回收站仅适配已验证的 **Windows DeepSeek Harness 0.2.0-rc.2**，并核对安装包 SHA256：

```text
983ca71114e6dfd353fc79af5a1f9481a250ee64c2a3c757673029b811b23bc2
```

同版本号但安装包摘要不同也会拒绝启用。其他版本需要重新验证，不保证自动兼容。当前 profile 核验仅支持实际 backend 命令行中以 `profiles\desktop` 结尾的规范路径。

## 快速开始

### 1. 构建桌面程序

需要 Windows Rust MSVC 工具链、Visual Studio C++ 构建工具（含 Windows SDK）和 WebView2 Runtime。首次构建需要下载 Cargo 依赖。

先进入**项目根目录**，确认存在 `desktop/src-tauri/Cargo.toml`：

```powershell
cd C:\path\to\harness-session-plus
cargo build --manifest-path desktop/src-tauri/Cargo.toml --release --locked
& .\desktop\src-tauri\target\release\hsp-desktop.exe
```

静态前端嵌入 EXE，不需要 npm。当前没有生成 MSI/NSIS 安装包，也没有代码签名。更新时从托盘退出旧管理工具，再打开新版。

### 2. 开启 Harness 调试连接

在“设置”配置 Harness 可执行文件路径，然后在“连接与增强”点击“启动 Harness”。也可以手动启动：

```powershell
& "$env:LOCALAPPDATA\Programs\DeepSeek Harness\DeepSeek Harness.exe" --remote-debugging-port=9223 --remote-debugging-address=127.0.0.1
```

已运行且开启端口的 Harness 可以直接连接。如果普通启动时未开启端口，需要保存工作、正常退出，再带参数启动；重复启动不保证参数对已有实例生效。工具不会强制结束 Harness。

检查本机监听：

```powershell
Get-NetTCPConnection -LocalPort 9223 -State Listen
```

期望地址为 `127.0.0.1`。默认端口 9223，可在设置修改；多个页面需要明确选择目标。

调试服务可访问时，下面的只读命令会返回浏览器信息和页面列表：

```powershell
Invoke-RestMethod http://127.0.0.1:9223/json/version
Invoke-RestMethod http://127.0.0.1:9223/json/list
```

工具选择的是 `type = page`、`url = dsh-app://app/` 的页面。端口可访问只说明调试服务在运行；安装增强还需要通过版本、profile 和会话能力核验。

### 3. 安装和使用回收站

1. 点击“检查连接”，确认识别到 Harness 页面。
2. 在“会话回收站”点击“安装回收站增强”。
3. 重新打开 Harness 会话的“…”菜单，点击“移入回收站”并确认。
4. 通过侧栏入口或管理窗口进入回收站，搜索并单项或批量恢复。

关闭窗口会隐藏到托盘。退出管理工具不会移除已应用的皮肤：独立主题守护会按保存配置恢复 Harness 页面样式。电脑重启、用户注销或手动结束守护后，需重新打开工具启动守护；本版不设置开机自启。回收站菜单增强与皮肤相互独立，页面刷新后回收站增强仍需重新安装。断线后停止接收菜单动作，不自动重放旧请求。

## 管理窗口

| 页面 | 用途 |
| --- | --- |
| 概览 | 查看连接与增强状态、最近活动；打开 CoderPlant 推荐网站 |
| 会话回收站 | 安装增强、刷新记录、按标题或工作区搜索、选择并恢复会话、只读检查删除能力 |
| 连接与增强 | 检查连接、启动 Harness、安装测试增强、卸载增强 |
| 能力探测 | 主动采集页面结构证据，并导出报告 |
| 皮肤中心 | 44 款本地主题、搜索分类分页、社区按需安装、应用及恢复默认 |
| 设置 | 配置端口、Harness EXE 路径、目标页面与明暗主题 |

“安装测试增强”显示的是连接测试按钮；要使用真实回收站，请在“会话回收站”安装回收站增强。推荐网站入口不需要连接 Harness。

可选的每 15 秒自动检查只更新连接状态，连续 3 次失败后暂停；它不会自动安装增强、归档或恢复会话。

## 操作保护和数据保存

### 皮肤中心使用

打开“皮肤中心”，选择一张卡片，再点击“应用到 Harness”。卡片展示配色及壁纸，选择卡片不会修改 Harness。可用“检查皮肤状态”核对当前页面，点击“恢复 Harness 默认”移除本工具的样式覆盖。

通过现有 CDP 连接覆盖已识别的 `--dsw-*` 主题变量，不调用未验证的插件接口、不修改 Harness 安装包或原生主题偏好，不读写会话。缺少主题变量、目标页面不明确或存在未知同名样式时拒绝应用。选择主题后可调节背景清晰度（0–100%，默认 70%），点击“应用到 Harness”后配色、壁纸和清晰度会一起保存到本机。独立后台主题守护每三秒检查页面，并在样式丢失或版本不符时恢复；只连接到唯一的 Harness 页面，多个候选页面时不会自动选择。退出管理工具不清除主题；点击“恢复 Harness 默认”会先停止自动恢复，再移除页面覆盖。

内置四套配色，全量收录当前 [CodexPlusPlus 主题市场](https://github.com/BigPizzaV3/CodexPlusPlus-Themes/blob/main/index.json)的 **39 款主题**及 Codex++ Windows 内置的 **桥本有菜**，共 44 款本地主题。支持名称、作者和标签搜索、来源分类、12 张卡片分页。来源与许可见[主题素材说明](desktop/ui/themes/README.md)，实现分析见[Codex++ 主题分析](docs/codex-theme-analysis.md)。

“DreamSkin 社区”需要主动点击“加载 / 刷新社区”才联网，按 48 条分页浏览接口条目（本次返回 741 条，数量动态变化）。点击“安装到我的主题”，确认作者与许可后下载；核对版本、包和素材摘要，再转换配色与图片。安装不自动应用，已安装主题可离线使用，不占用手动导入的 20 个名额。旧格式、图片超过 5 MiB、许可或图片缺失及校验失败会显示拒绝原因，不能保证所有社区条目都兼容。

其他 Codex++ 主题可以通过“导入主题”加入：

1. 如果下载的是 ZIP，先解压，选择其中的 `theme.json`（`schemaVersion: 1`，最大 64 KiB）。
2. 可选对应的 PNG、JPEG 或 WebP 背景图片（最大 5 MiB），填写作者与许可信息。
3. 点击“导入到我的主题”。导入保存到本机，不会自动应用；选择新卡片，再点击“应用到 Harness”。

最多保存 20 个手动导入主题，重启管理工具后仍可选择。转换器读取背景、面板、文字、强调色和边框颜色，不运行包内 CSS、脚本或插件，不自动同步离线市场或移植 Codex 专属布局。手动导入的 ZIP 需解压；社区 ZIP 由专用流程在内存校验。不是所有 Codex++ 主题格式都兼容。填写许可信息仅用于记录来源，不代表工具确认了素材授权。

皮肤仅作用于 Harness 页面，需保持 Harness 的本机调试端口可访问。断线后守护会等待；页面重新上线后会重新查找唯一目标并恢复，通常最多有数秒延迟。电脑重启或用户注销后需打开工具一次启动守护，本版不设置开机自启。主题保持详见[主题保持说明](docs/persistent-themes.md)。它与管理窗口的明暗主题独立，不覆盖代码语法及成功、错误等语义配色。已验证 40 款内置/市场转换主题的壁纸、核心配色及默认恢复；完整退出并重启 Harness 的流程尚未实测，部分组件、原生窗口或第三方插件也可能保留原色。

### 会话操作

- 执行前核对安装版本、监听进程、backend profile、真实会话 ID、工作区及原生状态。
- 运行中或运行状态未知时拒绝操作；归档不携带 `stopActivity`，原生接口还会检查活动 provider。
- 先事务保存操作意图，再调用原生接口，最后独立读取归档集合、会话及工作区核对结果。
- 超时或断线保留“结果未确认”，重连后只读核对，不自动重试数据操作。
- 恢复仅允许当前 profile 下由工具收录、归档状态已确认的会话。
- 退出尝试卸载增强；无法确认时显示提示，保留 Harness 运行。

工具数据保存在 Tauri `app_config_dir`：

| 文件 | 用途 |
| --- | --- |
| `settings.json` | 连接、程序路径、管理窗口主题配置 |
| `active-theme.json` | 当前应用的完整 Harness 主题、清晰度及恢复版本 |
| `imported-themes.json` | 本地导入主题的颜色、图片及来源信息 |
| `community-themes.json`、`community-themes/` | 已安装社区主题清单及独立主题文件（含图片与上游 manifest） |
| `recycle.sqlite3` | 回收站元数据及操作日志，可能伴有 WAL/SHM 文件 |
| `capability-report.json` | 用户主动导出的结构报告 |

回收站元数据包含 ID、标题、工作区名称，不保存聊天正文。结构报告不采集正文、认证数据或属性值。构建产物和本机提取的源码证据位于被 Git 忽略的 `target/`。

备份工具记录时，先从托盘正常退出，再复制应用配置目录。`recycle.sqlite3` 是记录与核对依据，不能代替 Harness 自身会话数据的备份。不要通过删除这个数据库来“清空会话”：这会丢失工具记录，并不会永久删除 Harness 中的会话。

## CLI

CLI 用于诊断及测试注入；真实回收站操作由桌面工具完成：

```powershell
cargo run --locked -p hsp-cli -- diagnose --port 9223
cargo run --locked -p hsp-cli -- inject-test --port 9223
cargo run --locked -p hsp-cli -- remove-injection --port 9223
cargo run --locked -p hsp-cli -- probe-capabilities --port 9223
```

`inject-test` 只加入连接测试按钮，不归档或删除。多个匹配页面需要 `--target-id <ID>`；页面 ID 不是持久实例身份。`--retries 0..3` 只重试连接初始化，默认 2 次，不重放注入或业务操作。JSON 输出到 stdout，失败输出到 stderr，退出码非零。

## 常见问题

**manifest path 不存在？** 先进入项目根目录，或使用 manifest 的实际绝对路径。根目录 `cargo build` 只构建 core/CLI，不构建独立桌面包。

**无法连接？** 检查调试参数、端口和监听地址。已普通启动的实例通常需要正常退出后重启。

**必须从本工具启动 Harness 吗？** 不需要。通过快捷方式、终端或其他启动器开启本机调试端口的实例也可连接。已经运行但没有调试端口的实例需要正常退出后带参数重启。

**只有原菜单？** 在新版工具安装“回收站增强”，关闭并重新打开会话菜单，避免同时运行旧版。身份无法唯一核实时按钮会禁用并显示原因。

**只看到“回收站 · 连接测试”？** 这是测试增强，不会修改会话。进入“会话回收站”安装回收站增强，然后重新打开会话菜单。

**显示不兼容或无法核实 profile？** 当前只适配上文列出的安装包摘要和 profile 结构。能力探测报告可以帮助定位差异；不能靠修改版本标签或跳过检查保证兼容。

**回收站为空？** 仅显示当前 profile 下由工具移入的会话。自行归档的记录不会导入；恢复成功的记录不再显示。

**恢复后显示“结果未确认”？** 保持原 Harness 实例和 profile，重新检查连接并刷新回收站，让工具只读核对实际状态。不要连续重复操作。

**点击关闭后工具还在运行？** 窗口关闭会隐藏到托盘。要更新或彻底退出，请使用托盘退出或设置页“退出工具”。

**为什么不能永久删除？** 当前适配版本没有已验证的原生会话删除接口。清搜索索引不等于删会话；离线日志清理需要另行核实引用、锁、缓存、附件和备份恢复。当前只有只读预检，见[删除审计](docs/permanent-delete-audit.md)。

## 开发和验证

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --locked --all-targets -- -D warnings
node --test fixtures/injection.test.cjs fixtures/recycle-menu.test.cjs fixtures/desktop-ui.test.cjs fixtures/skin.test.cjs
```

Node 仅用于开发脚本和 JS 模拟测试，不是产品运行依赖。截至 2026-10-14，桌面 0.5.3 release 构建成功；workspace/core 与桌面合计 18 项 Rust 测试、25 项 JS 测试通过，core/desktop Clippy 零警告。主题守护的真实 Harness 验收细节和限制见[主题保持说明](docs/persistent-themes.md)。

真实 Harness 验收使用两个新建同名专用会话：仅按指定 ID 归档其中一个，再恢复并核对工作区，另一个未被归档。没有执行真实永久删除。模拟测试不能替代完整窗口、托盘及升级兼容验收，见[回收站验证](docs/recycle-validation.md)。

`crates/core/examples/recycle_acceptance.rs` 会创建并变更新建测试会话，**不是只读测试**，仅应在专用验收环境主动运行。

## 项目结构

```text
crates/core/          Rust CDP、发现、适配器、SQLite、回收站业务
crates/cli/           Rust CLI 诊断与测试注入
desktop/src-tauri/    Tauri 宿主、commands 与托盘
desktop/ui/           HTML/CSS/JavaScript 管理窗口
injected/            Harness 菜单增强与接口桥接
fixtures/            开发检查脚本与 JS 测试
docs/                架构、验证、兼容性与审计
DEVELOPMENT_PLAN.md   原始分阶段开发方案
```

桌面包独立于根 Cargo workspace，使用自己的 manifest 和 Cargo.lock。

## 版本记录

| 桌面版本 | 主要变化 |
| --- | --- |
| 0.5.3 | 应用主题及清晰度写入本机配置；新增独立主题守护，管理工具退出后仍可恢复页面覆盖，并在显式恢复默认时停止守护 |
| 0.5.2 | 新增本地主题库与社区按需安装；增强背景清晰度调节和主题状态管理 |
| 0.5.1 | 扩展主题导入、预览与 Harness 页面应用能力 |
| 0.5.0 | 全量收录 39 款 Codex++ 市场主题和 Windows 内置主题；新增搜索分类分页、轻量预览及 DreamSkin 社区分页/按需核验安装 |
| 0.4.0 | 新增三款 Codex++ 转换主题及壁纸；支持 theme.json 与本地图片导入，保存作者、许可及本地主题库 |
| 0.3.0 | 新增 Harness 皮肤中心：四套预设配色、局部预览、状态检查、应用与恢复默认；退出尝试清理外观覆盖 |
| 0.2.2 | 概览页新增 CoderPlant · 码源推荐卡片，适配明暗主题与窄窗口；固定官网入口通过默认浏览器打开；修正侧栏版本和底部状态文字 |
| 0.2.1 | 新增永久删除能力的只读预检与逐项原因展示；永久删除仍未开放 |
| 0.2.0 | 接入已验证的 Harness 原生归档与恢复，加入真实 ID 绑定、profile 隔离记录和操作结果核对 |

桌面版本、Harness 版本和注入脚本版本各自独立。例如桌面 0.2.2 仍使用回收站注入脚本 0.2.0，这不表示程序没有更新。

## 参考和文档

界面风格参考 CodexPlusPlus，按项目设计记录独立实现。本项目不是 DeepSeek 官方插件或 CodexPlusPlus 官方版本。当前仓库未附许可证文件，不应据此推定代码的开源授权条款。

- [桌面版说明](desktop/README.md)
- [皮肤验收](docs/skin-validation.md)、[主题素材与许可](desktop/ui/themes/README.md)
- [原生回收站验证](docs/recycle-validation.md)
- [永久删除审计](docs/permanent-delete-audit.md)
- [架构](docs/architecture.md)、[兼容性](docs/compatibility.md)
- [手动验收](docs/manual-validation.md)、[视觉参考](docs/design.md)

部分阶段文档保留了早期开发记录，当前功能状态以本 README 与最新验证文档为准。
