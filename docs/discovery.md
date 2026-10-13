# 阶段 B：会话身份与原生能力探测

调查日期：2026-10-12（Asia/Shanghai）。证据分为用户先前观测、公开源码、此次真实环境、模拟测试。它们不相互替代。

## 当前环境

本次 `cargo run -p hsp-cli -- diagnose --port 9223 --retries 0` 实际退出 1，原因是 loopback discovery failed。先前 TCP 检查同样未连接到 9223。方案给定的安装路径在此次文件检查中未找到；这不能证明其他位置不存在 Harness。

本次没有真实浏览器/应用版本，没有已加载前端资源或真实网络流，没有专用测试会话，没有运行原生归档/恢复。阶段 B 真实验收尚未完成；禁止进入阶段 C。

方案中 `Chrome/152.0.7977.54`、`dsh-app://app/` 和“归档会话”菜单为用户先前观测，不能冒充此次验证。

## 公开源码版本和许可证

官方仓库：[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)。调查固定到提交 `d743267388641bc76f17c45ce8b4c231aed1d32c`，默认分支 master。仓库元数据标为 MIT，`apps/desktop/package.json` 声明 `0.2.1-alpha.2`。这不是当前用户安装版本。公开源码文件与 SHA-256 清单见 `source-evidence.json`；下载副本仅保存在忽略的 target/source-evidence，产品不嵌入这些副本。

## 能力证据

下列路径均以固定提交为基准，行号来自实际读取的源码。工具未调用这些 Harness 方法。

| 能力 | 源码证据 | 本机结论 |
| --- | --- | --- |
| 会话 ID | [Rows.tsx L589](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/client/ui-workspace/src/client/rows/Rows.tsx#L589) 的 `data-row-key` 是 `session:${node.id}`；L681-L687 把 `node.id` 作为菜单及行操作的 sessionId | 只是候选 DOM 身份证据，尚未核对已加载资源或实际菜单 |
| 菜单操作对应 ID | [ArchiveSession.tsx L43](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/client/ui-workspace/src/client/session-actions/ArchiveSession.tsx#L43) 将传入 sessionId 交给归档/恢复方法 | 未验证菜单弹层与行元素的实时对应；禁止以标题、DOM 序号或“选中行”推断 |
| 会话列表 | [types.ts L272](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/api/session-controller/src/types.ts#L272) 的 SessionListValue.items 为 SessionSummary；L182 有 sessionId | 未查询真实列表；未核对错误、重连基线或权限 |
| 工作区 | [workspace types.ts](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/api/workspace-controller/src/types.ts) 的 WorkspaceView 包含工作区身份及 sessionIds | 未确认持久 profile 身份及跨实例 ID 隔离 |
| 原生归档 | [index.ts L154](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/api/workspace-controller/src/index.ts#L154) 在 workspace 命名空间注册 archiveSession；[commands.ts L162](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/api/workspace-controller/src/commands.ts#L162) 调用 registry 并返回归档集合 | 源码存在，本机禁用 |
| 原生恢复 | index.ts L164 注册 unarchiveSession；commands.ts L192 调用 registry 移除归档状态并返回集合；UI 的 ArchiveSession.tsx 同时支持恢复 | 源码存在可逆方法，本机未证明恢复到原工作区及重启一致性 |
| 活动状态 | session-controller/types.ts L184 为 running:boolean；workspace/types.ts L118-L124 指出归档活动包含当前轮次、子代理、后台任务、计划任务 | 不能把 running=false 或无动画当作完整空闲；可靠完整来源未知 |
| 永久删除 | 尚未完成删除、引用关系及持久化清理调查 | 未实现、不可调用；没有执行 |

### 原生归档/恢复调用结构（仅源码契约）

workspaceController/src/index.ts 的构造函数注册命名空间 `workspace`。workspaceController/src/types.ts L115-L134 定义：

```text
archiveSession request: { sessionId: <真实 SessionId>, stopActivity?: boolean }
unarchiveSession request: { sessionId: <真实 SessionId> }
result: { archivedSessionIds: <SessionId[]> }
```

这不是已验证的 HTTP URL、IPC 通道或 window 全局方法。`stopActivity:true` 会要求停止多种活动；工具第一版禁止该选项，且本次没有发送任何归档请求。工具也不暴露任意 evaluate 或应用方法调用 CLI。

源码归档成功的初步判断：结果包含目标 ID，之后通过独立重查/重连基线再次确认。恢复成功的初步判断：结果不包含目标 ID，并独立重查其原工作区归属与可打开性。只看 DOM 消失或 resolved Promise 均不足以验收。

源码失败结构：commands.ts L169-L177 将未知会话映射为 `session/not-found`，活动会话映射为 `workspace/session-active`，details 中包含活动信息。其他错误继续上抛。unarchive 对已经不在归档集合的 ID 可幂等完成，不能把这种 no-op 当作证明对象存在、完整数据恢复或原工作区已正确绑定。断线、超时和未知结果均视为未确认，禁止自动重放数据操作。

### 通信与桥接

[connection/client/rpc.ts L34-L77](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/packages/client/connection/src/client/rpc.ts#L34) 的通用 unary caller 在该源码版本中可使用 fetch 或 transport override，以文档相对 route POST 包络：

```text
{ type: 'client-request', rpcId: <随机关联 ID>, method: <生成端点>, payload: <参数> }
```

源码校验响应包络、rpcId 及 result；另有可注入的 worker-local stream carrier。因此不能把 HTTP 当作唯一应用通信方式。尚未核对生成端点与原生 dsh-app 协议载体，不提供或试探调用 URL。

[preload-app.ts](https://github.com/deepseek-ai/deepseek-harness/blob/d743267388641bc76f17c45ce8b4c231aed1d32c/apps/desktop/src/preload-app.ts) 暴露 dshDesktop 等平台桥接，包含版本、窗口/快捷键/更新/浏览器能力。所读文件不是会话归档桥接；不猜测不存在的 `window.harness.*` 或 IPC 方法。

## 当前只读探针

`probe-capabilities` 重新发现页面，原子核对 URL，再使用嵌入的 probe.js 输出：允许列表中的 role 计数、菜单 data-/aria- 属性名称、外部/内联脚本数量、自定义元素种类数量。完全不读取聊天正文、标题、属性值、资源 URL、内联脚本、storage、令牌或数据库。

这些结构指标只支持后续人工核对；所有 capability flags 为 false，instance_identity_verified 与 mutations_enabled 为 false。不宣称从 DOM 证明身份、运行状态或恢复能力，不发起原生调用。

## 必须继续完成的真实验证

1. 用户在可连接环境启动 Harness，核对仅本机监听；记录应用精确版本及所加载资源与固定源码的对应关系。
2. 仅创建两个同名专用测试会话 T1/T2，记录实际不同 ID 的对应别名；工具报告不使用名称作身份。
3. 对 T1 原生归档，再恢复；T2 始终不变。观察菜单触发、生成端点、载体、请求结构及重查结果，敏感值仅在现场核对，不导出聊天/认证数据。
4. 重启 Harness 验证身份及原工作区归属；验证运行中的测试任务和未知状态均拒绝变更，不确认“停止并归档”。
5. 确认持久 profile / 数据目录身份，活动 provider 及数据/同步/引用关系。没有恢复证据或完整身份时保持禁用，停在 B。

本次没有执行真实永久删除，没有修改 Harness 安装包、项目目录、工作树、数据库或日志。

## 后续真实连接更新（2026-10-12）

后续测试已成功连接本机 9223 并识别 Harness 页面，浏览器 Chrome/152.0.7977.54、CDP 1.3。前述“未连接”是首次调查时的结果，不能代表最新状态。安装/重复安装/卸载/重新安装及结构探测命令成功，最终注入已移除。菜单当时未打开（testButtons=0），按钮显示及点击仍未验收。源码版本匹配、真实会话身份、完整运行状态、归档/恢复闭环仍未验证，全部数据操作保持禁用。详情见 manual-validation.md 的后续真实测试记录。
## 菜单探测更新

实际菜单结构已通过只读 CDP 核实：归档菜单项的文本含快捷键，标签独立放在 span，按钮父 DIV 是菜单项列表容器。原注入整项文本精确匹配失效，现已按标签匹配并插入该容器。脚本 0.1.1 实际注入后用户确认测试按钮可见。此修复不解析会话 ID，不启用任何数据变更。