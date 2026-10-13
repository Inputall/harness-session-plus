# 视觉参考与阶段范围

用户指定参考 [CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus)。已读取其 `apps/codex-plus-manager/src/styles.css` 和 package.json；仓库声明 AGPL-3.0-only。本工具不复制其 React/Tauri 组件、注入实现或管理业务代码。

参考视觉特征：蓝黑层级背景、冷白浅色模式、蓝紫强调色、紧凑侧栏/管理区、6/8/10/14px 圆角、140ms 快速交互反馈、清晰状态与禁用原因。未来阶段 E 的管理窗口采用相同方向，以原生 HTML/CSS/JavaScript 独立实现，无 Node 后端。

阶段 A 不依赖桌面管理窗口。当前测试按钮使用原创 CSS，继承 Harness 的字体和前景，6px 圆角、紧凑内边距、低对比 hover、蓝紫键盘焦点圈和 140ms 交互反馈，以免覆盖原生菜单主题。所有样式均限定 hsp- 前缀并可卸载。按钮文案明确“连接测试”，不暗示真实回收站能力。

阶段 A/B 没有伪造会话列表、恢复按钮或永久删除界面。桌面视觉验收、暗/亮模式及多语言验证留待阶段 E 真实实现。

参考源码：[styles.css](https://github.com/BigPizzaV3/CodexPlusPlus/blob/main/apps/codex-plus-manager/src/styles.css)。此文件只是设计参考记录，不表示阶段 E 已完成。

## 桌面界面源码更新

已新增 desktop/ui：218px 侧栏、紧凑卡片、蓝紫强调、深/浅主题、明确连接/增强状态与能力禁用原因。概览装饰由原创 CSS 和 SVG 绘制，没有示例会话或假数据。回收站页面为待验证状态，真实数据操作继续禁用。Tauri 源码已接入 Rust 管理动作及托盘；尚未构建可执行程序，视觉和原生窗口验收未完成。
