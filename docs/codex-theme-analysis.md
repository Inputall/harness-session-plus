# Codex++ 主题分析与 Harness 适配（2026-10-14）

## 来源与范围

直接读取 Codex++ 上游的 `dream_skin_library.rs`、`dream_skin_market.rs`、`dream_skin_community.rs`、`settings.rs` 和主题文件，独立实现转换；没有移植其注入脚本。

| 来源 | 当前范围 | 本工具接入 |
| --- | --- | --- |
| [Codex++ Windows 内置](https://github.com/BigPizzaV3/CodexPlusPlus) | 桥本有菜 | 内置图片及默认配色转换 |
| [GitHub 主题市场](https://github.com/BigPizzaV3/CodexPlusPlus-Themes/blob/main/index.json) | 39 款 | 全量离线收录，逐文件摘要验证 |
| [DreamSkin 社区](https://dreamskin.cc/gallery) | API 当时返回 741 个条目，动态变化 | 显式加载、48 条分页、按需核验并安装；不保证每个条目可转换 |
| 用户自己的主题 | 每位用户不同 | 手动导入 theme.json 和背景图片，不读取 Codex 本地配置 |

Codex++ 上游社区代码最多载入 500 条；本工具使用接口分页访问后续条目，不把“500”视为社区总数。市场 README 表格仍列出早期 14 款，本次以机器索引实际 39 款为准。源码和市场快照提交摘要见 [snapshot.json](../assets/themes/snapshot.json)。不承诺自动同步后来新增的离线市场素材。

## 转换方式

市场主题是 schemaVersion 1 的 JSON、图片和许可，不是通用 Harness 插件。Codex++ 的 stylePreset 会绑定它自己的布局与装饰；Harness 没有已验证的同名接口。本工具仅把背景、面板、文字、强调色与边框转换为已识别的 Harness CSS 变量，背景图片嵌入当前页面自有样式。

桥本有菜的上游 Windows 配置没有 colors，使用上游 `DreamSkinColors::default()` 数值作为明确的转换默认值。原配置保存在素材目录的 upstream-theme.json；转换配置另外保存。背景位置当前采用居中 cover，未移植 art/safeArea、动效、毛玻璃布局或人物遮罩，视觉不会完全等同 Codex++。

完整图片嵌入 Rust 资源，管理窗口只加载小型 JPEG 缩略图，按来源分类、名称/作者/标签搜索及 12 张卡片分页。应用单个主题时才读取完整壁纸，避免目录 IPC 传输全部图片。

## 社区安装

使用上游代码公开的固定 `https://api.dreamskin.cc/v1/themes` 接口，禁用重定向，限制响应大小和下载时间。安装前用户看到作者和许可并确认；这是素材使用选择，不会变更会话。

下载后核对版本 ID、包大小、包 SHA256、manifest 中的身份，以及 theme.json/图片的大小和 SHA256。ZIP 在内存读取，拒绝异常路径、重复条目、符号链接及过大的解压数据；不提取到 Harness 安装目录，不执行 CSS 或脚本。主题图片限制 5 MiB；旧格式、缺少许可、缺少图片、身份或摘要不一致、颜色表达式不受支持的主题会拒绝安装并显示原因。

社区主题保存为独立本地文件和摘要列表，不占用手动导入的 20 个名额；安装不自动应用。上游 manifest 的许可与来源信息随本地文件保留。许可标签不是授权证明，尤其笼统的“CC”标签应结合主题原作者说明判断。

## 验证与边界

40 款内置/市场转换主题已在当前 Harness 0.2.0-rc.2 页面逐款核对：样式安装、强调色、壁纸实际解码与背景加载成功；恢复默认后背景和强调色与测试前一致。Rust 资源校验遍历所有主题，核对原始图像摘要与来源清单。

社区 API 清单真实加载 48 条，下载并成功校验转换 MIT 主题 millsage（ver_1877a6b6015e7306f116）。这代表安装链路通过，不代表全部 741 个社区条目经过逐款兼容测试。所有验收均未归档、恢复或删除会话。

原生 0.5.0 桌面窗口已验证 44 款本地主题目录、12 张卡片分页、首屏缩略图解码和动态配色；通过实际按钮选择内置主题并应用/恢复成功。社区样本通过桌面 Rust command 保存后出现在本地主题库。验收程序正常退出，临时 WebView2 调试参数未写入产品配置。

主题市场原许可文件完整保存在 assets/themes 各目录并随桌面前端保留副本；CC BY 4.0 素材保留作者、来源及转换说明。上游主程序采用 AGPL-3.0，本项目没有复制其 Rust 业务实现或注入代码。
