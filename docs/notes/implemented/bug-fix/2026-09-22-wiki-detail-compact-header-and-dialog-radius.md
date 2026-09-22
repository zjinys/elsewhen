# Agent Note: Wiki 详情页头部紧凑化 + 全局对话框圆角收敛

Status: implemented

## Problem

1. Wiki 详情页头部（元数据行 + 26px 大标题 + 2 行摘要 + 标签行 + 别名行 + 关系行 + 工具栏）叠了 5~6 层才把正文顶出来，内容区域被严重压缩，用户反馈「内容区域要足够大，标题元数据不能占这么大区域」。
2. 全局对话框用 Material 3 默认圆角 28，视觉上过大，与卡片/输入框的 12 不一致。

## Decision

**头部紧凑合并**（`_WikiPageBodyState._buildHeader` 重写）：
- 主行合并为一个 Wrap：kind 徽章 + 标题（26→20px、单行省略）+ 元数据纯文本（slug · 证据 N · 更新日期）+ 来源 chip，窄屏自然换行；
- 摘要 2 行→1 行（12px tertiary）；
- 标签 + 别名折叠进「标签（N）· 别名」折叠行，默认收起（`_metaExpanded`），点击展开后可编辑；
- 人物关系行保持直接可见（内容级信息，空时自隐藏；且 `wiki_relations_ui_test` 依赖其默认可见，不动它避免踩并行改动）；
- work-item 面板维持原样（功能性区块）。

**对话框圆角**：`AppTheme.buildTheme` 增加 `dialogTheme`（radiusMedium=12），一处改动覆盖全部 16 处 AlertDialog/Dialog；无局部 shape 覆盖需要清理。新增 `app_theme_test` 钉住该行为（防 flex_color_scheme 升级回归）。

## Alternatives considered

- 可折叠头部（默认只显示一行，点击展开全部元数据）：收起太极致，标题扫描成本变高；紧凑合并已释放约 60% 头部高度，够用。
- 关系行也折叠：会打破并行会话的 `wiki_relations_ui_test`（断言头部直接可见「人物关系」），且关系是内容而非元数据，弃。
- 对话框逐个传 shape：16 处调用点，维护面大；主题层一处定义是正解。
- 圆角用 radiusSmall(6)：对话框是独立浮层，12 与卡片/输入框同 token 更协调。

## Consequences

正文区域显著放大（头部从 ~6 层压到 2~3 层）；移动端 390×844 走查测试因头部释放高度而通过。标签/别名编辑需多点一次展开（可接受，低频操作）。注意：实施期间并行会话向同文件顶部加了 `_KnowledgeBrowser`（知识库浏览/筛选栏），曾覆盖掉本改动一版，已重新应用；该并行功能目前使 `wiki_ingest_placeholder_test` / `wiki_area_filter_test` 两个旧测试失败（搜索框 TextField 数量变化），由并行会话收尾处理，本轮未动。全部自有测试 20 项 + 主题测试绿，analyze 无新增告警。