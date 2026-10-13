# 永久删除能力审计（2026-10-13）

结论：本机 Harness 0.2.0-rc.2 不具备已验证的会话永久删除接口。不能安全启用永久删除；本次未执行真实永久删除。新增的“检查删除能力”为只读预检，不代表已实现永久删除。

安装包 SHA256 仍为 `983ca71114e6dfd353fc79af5a1f9481a250ee64c2a3c757673029b811b23bc2`。通过 `fixtures/asar-inspect.cjs` 只读提取安装包内实际 JS 到 `target/source-evidence/delete-audit-*.js`，没有改写 Harness 安装包或用户数据。

证据：

- `dsh-api-session-controller/lib/index.js` 的 Remote 方法声明列出 list/search/create/rename/fork/prompt/cancel/page/follow/projections 等，没有 delete/remove/purge 会话的方法。
- `dsh-api-workspace-controller/lib/index.js` 支持 archiveSession/unarchiveSession；其中 delete 是移除工作区注册，不能用于删除会话。
- `dsh-session-persistence-jsonl/lib/index.js` 的 removeTemporary/removeCommittedTemporary 只删除格式迁移阶段临时文件，不是会话日志删除操作。
- `dsh-session-query-sqlite/lib/index.js` 的 `_deleteSession` 在 `_reconcile` 中维护 derived 搜索索引。它依据持久与 live 记录重新构建，单独清索引不能删除日志、live 会话或工作区引用。
- `dsh-session/lib/index.js` 的 detachEntered 只移除内存 store attachment 并发出 session/disposed，不删除持久日志。

新增 Rust 只读删除预检：核实当前安装版本与 profile、工具回收站归属、真实 ID、原工作区、运行状态及原生归档集合。每项返回不能删除的具体原因。不添加虚构 RPC，不提供文件删除后门，不将列表缺项当作完整清理成功。

后续可靠实现可采用 Harness 新增的原生删除能力，或者独立完成离线存储适配。后者必须先证明：完整数据和引用范围、fork/附件共享关系、profile 与共享 home 的对应、跨进程锁与停止条件、日志/工作区域/派生索引/缓存之间的事务及故障恢复、有效备份恢复验证。当前没有这些证明，不能在运行中的 Harness 上直接删日志或改库。

用户原先要求“不执行真实永久删除”继续生效。本次只读安装包审计与自动模拟测试，不删除截图中的 nihao 会话。
