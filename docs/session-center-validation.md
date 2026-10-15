# 会话中心 0.6.0：实现边界与验收

## 已验证的接口依据

适配 Windows Harness 0.2.0-rc.2，继续使用 `profile.ps1` 中的安装包 SHA256 白名单、监听进程、实际 exe 和 profile 核验。没有修改安装包或 Harness 数据库，也没有使用任何删除接口。

本机安装包只读提取并核对的资源：

- `@deepseek-ai/dsh-api-session-controller/lib/index.js`：`SessionController.list/search/projections/page`，冷会话查询不激活 Agent；`SessionHistoryController.page` 的 address、throughSeq、maxMessages 和 records 契约。
- `@deepseek-ai/dsh-session/lib/index.js`：用户消息 `event.data`、助手消息 `event.data.message`、文本 content parts、`surfaceOp: append`；使用原始对话追加记录而非模型替换副本。
- `@deepseek-ai/dsh-client-ui-workspace/lib/client.js`：真实 `WorkspaceBrowser.open(sessionId)`、`SessionNode.pendingInteraction`；只读已提交的 React props，不调用 hook，不修改 props，不创建新的事件接管者。
- `@deepseek-ai/dsh-client-ui-session/lib/client.js`：已提交 hook 的统一状态 Map（running、pendingInteraction、completionUnread）。仅在全部字段符合已核对结构时读取；无法观测则标记未知。
- `@deepseek-ai/dsh-agent/lib/index.js`：`turn/end` 的 `reason.kind: error`。只有新结束事件晚于上次运行基线的 sequenced cursor 时，才认定新错误。

临时源码证据保存在 Git 忽略的 `target/source-evidence/`。产品不引入对应 Node 包、服务或未验证的 RPC；读取和导航只允许固定动作。

## 小闭环

1. 列表 → 标题/路径/工作区/ID 筛选 → 正文命中。
2. 按唯一 sessionId 读取最近消息和事件序号，显示范围及历史截断。
3. 原文摘录 → 继续任务提示词 → 用户编辑 → 复制或本地 Markdown 导出。
4. 按 profile 关注最多 20 个未归档会话，首次读取只建立基线，10 秒轮询只对变化提醒。
5. 点击工具内提醒或 Windows 通知，在核对当前 profile 后调用已存在的原生侧栏导航回调。工作区没有渲染、会话不存在或已归档时拒绝并说明条件。

不会自动发送提示词、批准请求、停止任务、恢复归档会话或永久删除。

## 搜索回退

2026-10-15 真实 `session/search` 返回 `gateway/internal`，具体原因为当前部署设置索引 `openAt: never`。仅对这一已识别原因启用最近消息回退；其他网络或协议错误不会被当成空搜索结果。

回退最多读取 50 个普通非空会话，每个最近 20 条消息，最多三个并发；四秒后不再开始新批次，在途 RPC 单次仍有五秒超时。只扫描本机原生查询返回的文本，不直接打开或维护索引。返回实际扫描数、失败数和范围不完整标志；部分读取失败也会显示，不能把有界结果描述为完整历史全文搜索。

摘要为抽取式最近对话，不调用模型，不承诺理解全部任务。每条摘录最多 6000 个 UTF-16 单元；不包含系统提示、工具输出、推理或附件。正文只在内存中停留，只有用户主动复制和导出时输出。导出为应用配置目录的 `exports/session-context-<timestamp>.md`，最大 1 MiB，路径不接受外部输入。

## 通知与边界

运行状态来自原生列表，等待审批、问题回答和计划审阅来自可观测的统一 UI 状态。10 秒以内出现又消失的状态可能漏报；被折叠/虚拟化且统一状态无法观测时，不推断等待状态。

“运行已停止”不等于成功，只有新原生错误结束事件才能触发错误提醒。实例、配置或 profile 改变后停止关注；重新开启建立新基线，不重放旧状态。

Windows 通知默认关闭，可用单独按钮主动测试。原生实现使用 WinRT Toast，注册当前用户 AUMID，并创建开始菜单的 `Harness Session Plus Notifications.lnk`（带 AUMID，普通启动快捷方式，不是开机自启）。操作系统通知仅包含产品名和固定状态文字；不含标题、路径或正文。系统禁用、创建或显示失败时在界面报告错误环节/错误码，保留工具内通知。通知点击的会话路由只在工具进程仍运行时有效；完全退出后不保证旧通知能恢复目标会话。

## 自动测试

- Rust：查询动作/长度限制、未知动作拒绝、未知状态不误报、旧错误不误报；原 CDP 连接与导航失效测试继续覆盖。
- JS：原生搜索与索引关闭回退、限定只读 endpoint、文本范围、同名 ID 导航、归档拒绝、UI 搜索/复制/导出/关注、迟到的读取结果不能覆盖新选择。
- 模拟测试不等于真实任务或 Windows 点击验收。

## 真实本机验证（2026-10-15）

- 原生列表：5 个会话；导航回调可用，5 个会话的统一/侧栏状态可观测。
- 最近消息回退：扫描 3 个非空会话，正文关键词找到 1 个命中，读取失败 0；存在更早历史时准确标记范围不完整。
- 最近摘录：实际读取 18 条文本消息，序号截至 258，原生最近轮次结束原因 `aborted`；没有将其误称为错误或完成。
- Tauri 桌面真实 IPC：搜索、摘录、继续任务提示词、Markdown 导出、关注 1 个会话、停止关注、按 ID 返回 Harness 会话均跑通。没有发送模型任务或变更会话数据。
- Windows 原生测试通知已被 `ToastNotifier.Show` 接受。首次权限查询曾返回 `80070490`（应用权限记录不存在），已修复为仅在这一错误下继续尝试首次发送，其他权限错误仍拒绝。系统异步显示失败会报告并保留工具内通知。没有把 API 接受描述为用户已看到并点击通知；真实审批、失败任务和 OS 通知点击仍需人工场景验收，不会为此自动发起付费模型任务。

开发验收脚本为 `fixtures/desktop-session-center-live.cjs`。需要唯一测试桌面开启临时本机 WebView 调试端口 9224、Harness 的调试端口 9223；普通产品不需要 9224。该脚本只读查询和切换查看会话，导出固定验收文字并关注/取消关注，不发送消息、不归档、不删除。截图与原始结果保存在 Git 忽略的 `target/`。
