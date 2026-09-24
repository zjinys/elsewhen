# 表格浏览态（只读）行错位修复

**状态**：已实现（代码在工作区，随并行会话的重构提交一起落库）
**日期**：2026-09-24

## 现象

知识页（如 `project/suanming` nizouyun）正文里的 Markdown 表格，浏览态下「完全错位」：
表头三格、数据行各格不在同一水平线上，列整体上下乱跳。

## 根因（vendor 只读失效）

层层验证排除了数据与解码问题：

1. **DB 内容正确**：`wiki_pages.content_md` 里是标准 GFM 表格，无错位；
2. **解码正确**：`MarkdownTableListParserV2` 虽是「按列跨步 `j += th.length`」读取，但对
   行优先平铺的 td 恰好还原出正确的 `cells[col][row]` 结构。用 position 逐格断言验证，
   `cell(row=0,col=0)=能力域`、`cell(row=0,col=1)=已具备…`，映射无错位；
3. **渲染才是问题**：vendor 表格行对齐依赖 `TableCol._buildCells` 里
   `updateRowHeightCallback` → `updateRowHeight`（把本行最高 cell 的内容高度写回所有
   cell 的 `height` attribute）→ `EditorState.apply`。但 `apply` 的
   `if (!editable || isDisposed) return;` 在**只读模式直接 return**——height 永不写回，
   每个 cell 高度回落到 `minHeight: cellHeight(40)`，长文本把 cell 撑到各自实际高度
   （实测 16~208px 不等）。
4. `TableView` 以「每列一个 `TableCol`（= Column）」布局，各列总高度不同，
   外层 `Row` 默认 `crossAxisAlignment.center` 垂直居中 → 列整体错位。

## 方案

新增只读表格组件 `ui/lib/wiki/wiki_table_block.dart`：

- **只读态**（`editable == false`）：不再依赖 vendor 行高回写，改用 Flutter `Table`
  重新布局——`TableRow` 天然「行内等高」，同一行所有 cell 高度取该行最大值，行列必然对齐；
  列宽固定 160（对齐 vendor `TableDefaults.colWidth`），长文本格内换行、行高自然撑起；
  每个 cell 内部仍走 `editorState.renderer.build` 渲染该格 paragraph——行内 code /
  href / wikilink 等样式与正文完全一致（textStyleConfiguration 与 textSpanDecorator
  全局生效，不需重复接线）；外层横向 `SingleChildScrollView` 兜底窄视口。
- **编辑态**（`editable == true`）：`apply` 生效、行高同步正常，直接委托 vendor
  `TableBlockComponentBuilder`，交互行为零改动。
- 接入：`wiki_content_editor.dart` 的 `blockComponentBuilders` 里
  `TableBlockKeys.type: WikiReadonlyTableBlockComponentBuilder()`（与 code 块 / heading /
  quote 同一覆写模式）。
- **观感调整（用户目验后）**：
  1. 表格靠左 + 列宽自适应——列宽不再固定 160（固定 320px 宽在阅读栏里居中悬停，
     观感像“浮块”），改 `MinColumnWidth(IntrinsicColumnWidth(), FixedColumnWidth(320))`：
     短列（如「能力域」）自然收窄、长列按内容放宽、上限 320 防超长文本把表格撑出
     阅读栏（超限内容格内换行）；`Table` 去掉显式 alignment（Flutter Table 无该参数，
     默认即贴内容区左缘，实测 left=56 = 页面 padding32 + 编辑器 padding24）。
  2. cell 底色移除——旧版给每个 cell 上 `surface1` 底色，与页面背景 surface0 割裂；
     改透明（只留 surface3 边框），背景与正文一致（AppFlowy 桌面端同款）。

前两者改动与表格组件同文件，测试同步更新：`test/wiki_table_readonly_test.dart` 的
「列宽固定 160」断言改为「列宽自适应 + 表格靠左」+「cell 无底色」，共 6 项。

## 二次反馈：第二张表「还是和之前一样」

用户目验后反馈：同页两表，**第一张好了，第二张（`资产证据`，3 列、内容窄）还是和之前
一样**。

排查过程（对齐结论逐步验证）：

1. **解码排除**：两表同走 `MarkdownTableListParserV2`，用真实 DB 内容 dump 两表
   block——表 2 为 3 列 × 14 行，cell 内容逐格正确，无结构问题。
2. **渲染排除**：结构级测量（直接枚举 `Table` 的 `TableRow` 逐行取各 cell RenderBox 的
   top）——**两表每一行 spread 均为 0，行对齐其实没问题**。之前直觉以为「第二张仍
   错位」，实测是 `find.textContaining(...).first` 匹配到了表格外正文/别的行，测试自身
   误判，不是渲染错位。
3. **真正的差异是水平位置**：表 1 左缘 x=56（贴左），表 2 左缘 x=120。逐层量 rect
   定位：`PageBlockComponent`（vendor，`page_block_component.dart` 非 shrinkWrap 分支）
   把每个块包成 `Center(Container(maxWidth: ∞, padding: 24))`，Container 收缩到子块
   自然宽（= 表格内容宽），再由 `Center` 水平居中：
   - 宽表（4 列 1168 > 可用 912）→ Container 触顶 960，无居中余量 → 贴左，横向滚动；
   - 窄表（3 列 784 < 912）→ Container 收缩 832，Center 居中 → 左缘 120（=56+64），
     视觉上与宽表割裂，观感仍是「居中 + 固定宽」——即用户说的「和之前一样」。

**修复**：只读表格组件最外层套 `SizedBox(width: double.infinity)`，块强制占满内容区
整宽——Container 不再收缩，Center 无居中余量，窄表也贴左（实测后两表左缘均 56），
列宽自适应行为不变。回归测试新增第 7 项「窄表格同样贴左，不被 Center 居中」。

## 验证

- `test/wiki_table_readonly_test.dart`（7 项）：只读表头三格与各数据行 cell 顶部
  y 一致（实测 56/56/56、116/116/116、344/344/344）；列宽自适应且表格贴左缘；
  窄表格同样贴左（左缘 56，不被 `Center(Container)` 居中——此次二次反馈的回归项）；
  cell 无独立底色；表格内行内 code 仍走 monospace；只读态渲染
  `WikiReadonlyTableBlockComponent`、编辑态不渲染（委托 vendor）。
- 真实内容整页验证：nizouyun 两表行级 spread 均为 0；改造后两表左缘均 56
  （修复前表 2 左缘 120）；analyze 无 issue。
- 全量 136 过 / 6 失败：失败名单与基线一致
  （message_copy/generating/retry/window + wiki_area_filter + wiki_ingest_placeholder——
  后两者属并行会话 _HomeTab 重构「首页」改动区，与本修复无关）。

## 协作交接

- 改动文件：`ui/lib/wiki/wiki_table_block.dart`（新增）、
  `ui/lib/wiki/wiki_content_editor.dart`（注册表格覆写）、
  `ui/test/wiki_table_readonly_test.dart`（新增）。均未提交，随并行会话重构提交落库。
- 已知边界：表格在**编辑态**仍走 vendor（未动）；只读表格不做单元格选择/逐字选中
  （块级整体选区），浏览语义下足够。