# 当前架构（A / B）

Cargo workspace：hsp-core 与 hsp-cli。CLI → 本机 HTTP discovery → 显式 Harness target 选择 → 已校验 WebSocket → CDP request/event dispatcher → 当前主世界 evaluate。注入和结构探针由 include_str! 嵌入，不从网络加载代码。

CDP 单读任务按 id 分发响应，事件使用 broadcast。单写任务串行发送；唯一递增请求 ID。pending 表保存 oneshot sender，RAII guard 在成功、失败、超时、取消或 future 被丢弃时移除请求。连接关闭唤醒所有 pending 请求；Client 被丢弃或 close 都终止连接任务。

导航/上下文销毁/清空推进 epoch；跨 epoch 的 evaluate 结果拒绝。没有缓存 contextId，每次在当前主世界执行。每个 CLI 动作还在相同 evaluate 表达式中再次检查 dsh-app://app/，避免发现后导航到其他页面仍注入。失效结果不自动重试操作。

连接重试只覆盖 discovery、握手和 Runtime/Page enable；上限 3 次，指数退避上限 1000ms。每次重新获取 target 信息，不缓存完整调试地址。异步取消可中断初始化、等待、退避。

注入 API window.__HARNESS_SESSION_PLUS__：install / getStatus / uninstall。未知既有版本拒绝覆盖；同版本安装幂等。只查询 role=menu，寻找已观测的中文归档项；不解析标题为对象 ID。由于菜单可由 portal 插入，observer 观察 document 根节点，80ms 合并调度，仅扫描菜单而非全文。移除时断开 observer、清除 timeout、移除按钮监听器与样式，并清除全局 API。

阶段 A 没有 native data binding、文件操作或数据库。阶段 B 使用结构探针和独立源码调查；模型中的 capabilities 表示工具可安全启用的能力，全部 false。公开源码存在方法不自动改变它们。

后续按方案增加版本适配器、独立 SQLite 工作线程及 Tauri commands/events；不让页面注入接触本地文件系统或 shell。当前没有常驻自动重新注入或托盘，刷新后需重新运行 CLI。

## 桌面宿主源码更新

desktop/src-tauri 为独立解析依赖的 Tauri 2 包，避免未下载的 Tauri 依赖阻塞原 core/CLI workspace。core/management 提供固定管理动作与 Harness URL 校验，桌面以明确 commands 调用；CLI 复用相同 URL 校验。桌面仅本地 main 窗口可监听事件，无 shell/filesystem 插件、远程脚本或任意代码执行入口。

托盘和窗口生命周期已写入源码，但尚未编译或真实验收。桌面调用使用互斥 gate 防止并发。注入前确认 target 并记录明确目标，即使结果超时仍保留清理意图；配置更改必须先卸载，退出只清理当前桌面安装的增强并保留 Harness。无法确认清理时保留工具窗口并提示用户刷新/重启 Harness 或主动直接退出。

HTML/CSS/JavaScript 仅展示状态和调用 commands。配置及报告由 Rust 保存；活动列表为内存 UI 记录，无正文与默认日志上传。当前没有 SQLite 或真实回收站操作。构建状态详见 desktop/README.md。
