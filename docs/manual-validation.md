# 手动与自动验收记录

日期：2026-10-12（Asia/Shanghai）。Windows / Rust 1.97.1 / Node v24.14.0（仅测试运行器）。

## 真实环境现状

| 项目 | 此次结果 |
| --- | --- |
| 127.0.0.1:9223 TCP | 未连接 |
| CLI diagnose --port 9223 --retries 0 | 退出 1：loopback discovery failed |
| 真实 inject-test / remove-injection | 未运行；缺少连接环境 |
| 创建专用同名会话 / 原生归档 / 恢复 | 未执行 |
| 真实永久删除 | 未执行；程序无此能力 |

## 最短阶段 A 验收

用户主动启动带本机 CDP 参数的 Harness。先用以下命令核对监听地址，只接受本机 loopback；若尚在重要会话中，先保存工作，不强杀应用。

```powershell
Get-NetTCPConnection -LocalPort 9223 -State Listen | Select-Object LocalAddress,LocalPort
cargo run --locked -p hsp-cli -- diagnose --port 9223
cargo run --locked -p hsp-cli -- inject-test --port 9223
```

打开包含“归档会话”的菜单，应只出现一个“回收站 · 连接测试”。点击仅显示提示，原生列表及数据不应改变。再运行 inject-test，仍只应有一个测试按钮。

```powershell
cargo run --locked -p hsp-cli -- remove-injection --port 9223
cargo run --locked -p hsp-cli -- diagnose --port 9223
cargo run --locked -p hsp-cli -- inject-test --port 9223
cargo run --locked -p hsp-cli -- probe-capabilities --port 9223
cargo run --locked -p hsp-cli -- remove-injection --port 9223
```

卸载后按钮应消失，diagnose 显示 installed=false；再次注入应成功。页面刷新后旧注入自然消失，再次 inject-test 可恢复。CLI 无自动常驻重新注入。断线时返回失败，卸载不可确认则刷新/重启页面清除旧注入。

多个 target 时按错误中 ID 加 `--target-id`，不能默认选首个。实际输出只需提供 diagnose/probe 的脱敏 JSON、应用版本、上述菜单/重复注入/卸载/刷新结果，不提供正文、调试 WebSocket 地址或认证信息。

## 阶段 B 原生验证

仅使用新建同名专用测试会话 T1/T2。在正式核实资源与 ID 关联后，通过 Harness 自己的归档/恢复界面只处理 T1；比较两者真实不同 ID、工作区、归档集合和恢复结果。不要点击“停止并归档”，不操作已有重要会话。记录重启后的实例身份和状态一致性。

若没有可靠的完整运行状态、持久实例身份或恢复成功证据，阶段 C 仍禁用。浏览器 fetch/Network/WebSocket/桥接观察应按实际载体进行；原始 body/headers/正文不得保存到报告。

自动检查最终结果见 README；模拟测试验证协议与注入生命周期，不能替代以上真实验收。

## 此次最终自动检查结果

- cargo fmt --all -- --check：通过。
- cargo clippy --locked --workspace --all-targets -- -D warnings：通过，零警告。
- cargo test --locked --workspace：10 项集成测试通过（2 项 HTTP、8 项协议/安全边界）。
- node --test fixtures/injection.test.cjs：3 项通过。使用简化 DOM/observer 模拟，未宣称真实浏览器验证。
- cargo build --locked --release -p hsp-cli：通过；生成 target/release/hsp-cli.exe（未签名 CLI，不是安装包）。

Rust 覆盖响应乱序、事件混入、唯一请求 ID、超时及迟到响应、取消、断线唤醒所有 pending、协议/脚本错误、上下文清空后拒绝旧结果、新请求不用失效 contextId、多目标显式选择、loopback endpoint 校验、拒绝 HTTP 重定向及全部变更能力禁用。JS 覆盖安装幂等、点击仅提示、observer 调度合并、卸载全部资源、重新安装和拒绝覆盖未知安装。

## 2026-10-12 后续真实 CDP 测试

此前离线状态已变化：本次 release CLI 成功连接 127.0.0.1:9223，识别 dsh-app://app/，浏览器 Chrome/152.0.7977.54，CDP 1.3。

实际执行 diagnose → inject-test → inject-test → diagnose → remove-injection → diagnose → inject-test → probe-capabilities → remove-injection，各命令成功。首次/重复/再次安装均 installed=true；卸载后 diagnose 确认 installed=false。最终已卸载注入。

测试时菜单未打开：testButtons=0，探针没有发现 menu/menuitem。因此本次只证明真实 CDP 连接、脚本安装幂等状态、卸载和再次安装；没有证明真实菜单按钮显示、点击提示、页面刷新后的重新注入或同名会话/归档恢复闭环。实际按钮资源清理仍需打开菜单后验证。

只读探针看到 button=38、tree=1、treeitem=5，external script=2、inline script=3。未收集正文、标题、资源 URL、认证或 ID 值。全部会话变更能力继续禁用，无归档/恢复/永久删除执行。

按用户“运行”尝试 Start-Process 时，自动审批服务因配置模型不受支持（HTTP 404）无法完成审核，启动命令未执行；随后独立诊断发现现有 CDP 服务已可用，直接复用连接完成上述测试，未绕过启动审批。

## 桌面版源码检查（2026-10-12）

已增加 Tauri 管理宿主、托盘及五个管理页面。原 workspace 的 fmt、离线 Clippy 与 10 项 Rust 测试通过，注入 3 项模拟测试通过。桌面 JavaScript 语法检查及 4 项 DOM/IPC 模拟测试通过；桌面 Rust rustfmt 解析/格式检查通过。

真实桌面编译、窗口、视觉、IPC 及托盘未验收：Tauri 依赖下载在沙箱内失败，沙箱外 cargo check 的自动审批因审核服务模型不支持（HTTP 404）无法完成，命令未执行；离线检查确认 Tauri crate 不在缓存。Playwright 导入同样被审批故障阻止，未进行浏览器视觉检查。

没有桌面 EXE、安装包或桌面 Cargo.lock；只有原 CLI/core 的已验证锁文件。桌面包单独解析依赖，不影响现有 CLI。后续完整步骤见 desktop/README.md。

## 菜单匹配修复与真实反馈（2026-10-12）

用户报告菜单只有原生项。现场只读 CDP 检查确认：role=menuitem 的归档按钮包含标签及快捷键，整项 textContent 不等于“归档会话”，但独立 span 标签与其精确相等；按钮所属 DIV 才是菜单项容器。

已修复 bootstrap.js：匹配标签 span，插入原生按钮的父容器；安装脚本版本升为 0.1.1，升级已知 0.1.0 时先完整卸载旧注入，未知版本仍拒绝覆盖。数据操作保持 false，点击仍仅提示。

已通过 5 项注入生命周期测试（新增快捷键标签及已知版本升级回归）、4 项桌面前端模拟测试、10 项 Rust 测试及 core/CLI Clippy 零警告。更新 CLI 构建成功并在真实 Harness 页面注入 0.1.1。用户重新打开菜单后明确确认“已看到测试按钮”。此次确认覆盖真实按钮显示，不包含用户点击提示或真实归档/恢复。

旧桌面 EXE 正在运行，初次新版构建覆盖失败；已将旧文件在同一构建目录重命名为 hsp-desktop.previous.exe，保留运行中的程序和 Harness，不终止进程。新版构建结果后续记录。
## 更新构建结果（2026-10-12）

菜单修复版已实际完成 `cargo build --manifest-path desktop/src-tauri/Cargo.toml --locked --offline --release`，输出 desktop/src-tauri/target/release/hsp-desktop.exe。这是实际桌面 EXE，嵌入注入脚本 0.1.1；没有生成安装包。此前依赖/审批阻塞的描述是历史记录，当前本机已有依赖及桌面锁文件。

旧程序仍运行在已重命名的 hsp-desktop.previous.exe。请从工具内退出旧版，再打开新 hsp-desktop.exe，避免重复托盘或旧版仍使用旧注入脚本。无需退出或重启 Harness。真实菜单按钮已由用户确认可见，真实数据操作继续禁用。