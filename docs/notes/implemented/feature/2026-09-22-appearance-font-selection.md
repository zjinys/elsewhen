# Agent Note: 外观设置字体选择（Google Fonts + flutter_font_picker）

Status: implemented

## Problem

App 字体硬编码 Inter（google_fonts），中文走系统回退；用户需要可选字体。迭代过程：固定 4 项下拉 → fc-list 枚举系统字体（345 项下拉）→ 用户指出下拉框不是好交互且 fc-list 仅限 Linux，建议看现有 widget、评估纯 Google Fonts 方案。

## Decision

**纯 Google Fonts 方案 + flutter_font_picker（2.0.0）搜索对话框**：

- **存值**（`app_meta.theme_font`）= Google Fonts 家族名字符串（`Inter`、`Noto Sans SC`…），特殊值 `system` 跟随系统不套网络字体。`AppFonts`（models/settings.dart）提供 `normalize()` 归一化旧机器名存值（inter/notoSansSc/notoSerifSc → 家族名）与 `displayNameOf()`。
- **主题层**：`AppTheme.buildTheme(preset, brightness, fontName)` —— `system` → flex 默认 textTheme；`GoogleFonts.asMap()` 命中 → `GoogleFonts.getTextTheme(family, base)` 动态加载；未知值（如 fc-list 时代存的系统字体名）回退 Inter。
- **UI**（外观 tab）：主按钮（当前家族名 + 自身字体预览 + 图标）打开 FontPicker 对话框（`showInDialog: true`，`showFontVariants: false`——全局正文字体只选家族，字重由主题管理）；旁边 ChoiceChip「系统默认」。picker 支持名称搜索、分类/字形（含 chinese-simplified，17 个 CJK 字体）过滤、最近使用。
- **关键坑**：FontPicker 对话框模式下「Select」按钮**内部自行 pop**，`onFontChanged` 里只更新状态，重复 pop 会误关下层路由。

## Alternatives considered

- 固定 4 项下拉（初版）：用户嫌少，弃。
- `fc-list` 枚举系统字体 + 下拉（第二版）：Linux 限定、345 项下拉交互差、预览样式无元数据（无法按分类/字形过滤），弃。fc-list 解析已写的反斜杠反转义/转义逗号处理随之删除。
- 自写搜索对话框：flutter_font_picker 是 verified publisher、10 个月前刚更新到 2.0.0（google_fonts 6.3.2），功能全（搜索/过滤/recents/预览），不重复造轮子。
- 限制字体列表 100-200 个（包 README 建议，滚动预览会触发下载）：用户要的就是字体多，保留全量 975，搜索+过滤可缓解；下载量由用户滚动行为决定且 google_fonts 有磁盘缓存。

## Consequences

跨平台（不再依赖 fontconfig）、单一数据路径（字体名即存值）、中文字体覆盖好（Noto Sans/Serif SC/TC/HK/JP/KR、ZCOOL 系列、马善政/站酷等 17 个）。代价：每个字体首次使用联网下载（ google_fonts 缓存到磁盘）；picker 预览滚动时同样下载。新增依赖 flutter_font_picker（传递依赖 shared_preferences，用于 recents）。测试：app_theme_test 3 项（主题字体应用 + normalize 映射）全绿；picker 本身为三方组件不加 widget 测试（预览字体在测试环境必然加载失败）。Rust 侧无改动（theme_font 本就存任意字符串）。

## Follow-up: 正文颜色与主题挂钩（2026-09-23）

用户报告「知识页内容字体缺省是白色，没法看」，询问是否本次修改所致，并要求字体颜色与 theme 挂钩。

**排查结论**：字体功能不涉及颜色——`fontAwareTextStyleConfiguration` 只 `copyWith(fontFamily)`，vendor 渲染（`appflowy_rich_text.dart` getTextSpan）基础 `text` 样式无色，正文颜色全靠上方 Material 的 DefaultTextStyle（= theme.textTheme.bodyMedium）继承。用真 AppTheme 的探针实测两极结果：深色 #F1F1F1 浅字 / 浅色 #111111 深字，均是主题正确值；google_fonts 的 `getTextTheme`（6.3.3 生成代码）只合并 fontFamily 不注入颜色。并行会话的 Tab 栏改造与 markdown_view 改动亦无颜色项。**白色正文非字体修改所致**。

**修复**：防御性加固 + 满足「与 theme 挂钩」的显式要求——`fontAwareTextStyleConfiguration(family, {color})` 把 `Theme.textTheme.bodyLarge.color`（fallback bodyMedium）注入基础样式（text/bold/italic/underline/strikethrough）。语义保证：

- `combine` 用 `copyWith(color: null)` 合并，基础色自动传递到加粗/斜体等组合样式；span 级显式颜色（attributes.color）仍覆盖。
- vendor 自带语义色不被覆盖：href 蓝 / code 红 / autoComplete 灰（仅 color==null 才注入）。
- family 与 color 均为空时返回 vendor 默认（system 场景零影响）。

**测试**：`wiki_content_editor_font_test.dart` +4 单测（注入/语义色保留/仅 color/null 零影响，共 7 项）；新增 `wiki_content_editor_theme_color_test.dart` 3 项 widget 级回归——真 AppTheme 下渲染实际渲染色深色为浅 / 浅色为深 / 两者不同。注意：同一 testWidgets 连续 pump 两个 MaterialApp，后一个的 Theme 会错位（实测），深浅各用独立 testWidgets。

**未代修**：并行会话的移动端走查溢出（_buildSectionBar 250px TabBar 在 390px 下溢出 55px）与 3 项遗留失败，均非本次范围。

## Follow-up: 正文字号设置 + 区块栏自适应（2026-09-23 下午）

用户两条新需求：① 系统设置除字体外支持**字体大小**；② 知识页详情某区块宽度不自适应（截图不可见，按已知 250px 溢出定位为区块栏）。

### 字体大小（仅知识库正文，默认 16 对应原观感）

- **存储**：Rust `ThemePrefsDto` 增加 `font_size: f64`，存 `app_meta["theme_font_size"]`（默认 16.0，解析失败回落）。`update_theme_prefs(mode, preset, font, font_size)`。生成面全套 regen（api.dart / frb_generated.rs / io / web），同轮 regen 也把并行会话 src 的 API 改动带进生成文件（实测仅我的 ThemePrefs hunk 进 diff，并行的 frb 改动不在 API 面）。**regen.sh 会重编 release lib**，走查 hash 一致。
- **模型/Provider**：`AppSettings.fontSize`（12–24，钳 clamp）；`SettingsNotifier.updateFontSize` + load/save 带字号。
- **UI**（外观 tab）：字号滑块 12–24 步进 1，拖拽实时更新状态、松手才 `saveTheme` 持久化；右侧显示 `N pt`。
- **生效**：`fontAwareTextStyleConfiguration(family, {color, fontSize})` 把字号只写入基础 `text` 样式——bold/italic/underline/strikethrough 自身不带 fontSize（vendor `text` 默认 16、组合均为 null），AppFlowy combine 时非空优先、null 回落基础值 → 组合样式自动继承字号；显式 delta 字号（heading/code 等）仍覆盖。为 null/0/负数不注入（vendor 默认零影响）。`WikiContentEditor.fontSize` 参数（默认 16），详情页在 `_buildSectionContent` 用 `ref.watch(settingsProvider.select((s) => s.fontSize))` 传入。
- **测试**：font_test +3（注入/默认/仅字号）；新增 `wiki_content_editor_font_size_test.dart` 3 项 widget 回归（RenderParagraph 真实渲染字号 18/16、加粗 span 继承字号）；settings_screen_test 外观 tab 滑块（共享 repo setUpAll——RustLib.init 每 isolate 只能一次）；settings_bridge_test theme prefs roundtrip 含字号。

### 区块栏自适应（内容/关联/产出 Tab 行）

- 病根：`SizedBox(width: 250)` 固定宽度（并行重构引入）+ 右侧「编辑/取消/完成」+ 聊天按钮，390px 下溢出 55px。
- 修复：去掉固定 250，`TabBar(isScrollable)` 自然宽度；Row 改 `spaceBetween`（Tab 贴左、操作居中、聊天贴右）；`LayoutBuilder` 内宽 <480 时 `_buildInlineEditActions(compact)`——编辑态「取消」收成 X 图标、「完成」缩 padding（保留文本，走查仍 tap 文本）；浏览态保持一致（'编辑正文' 文本保留）。
- 走查修复：窄屏（<1040）聊天本就不内联（FAB→bottom sheet），原 `WikiAiChatPanel findsOneWidget` 断言是陈旧拷贝 → 改 `findsNothing` + FAB 图标；卡座在「产出」页签下 → 切签验证再切回。**移动端走查转绿**（此前因 250px 溢出红）。

**协作交接**（与并行会话）：`wiki_page_detail_view.dart`（字号接线 + 区块栏自适应 + compact 编辑操作）与 `wiki_editor_integration_test.dart`（走查断言修复）两文件整体属并行重构区，本次提交不暂存，随并行会话的重构提交一起落库；工作时是同一工作区，功能即时可测。

## 后续设计存档：编辑器（内容区）覆盖层

用户要求「看知识页内容时能就地改字体/字号/行距」，讨论收敛为**两层覆盖模型**（编辑器可空覆盖层 ?? 全局层），并明确「先讨论、不动代码」。设计全文、模型、存储、UI 与开放问题见 [proposed：知识页阅读参数分层](../../proposed/product/2026-09-23-editor-reading-settings-layer.md)。本段（全局页正文字号）作为该模型的第一段已落地。

## Follow-up: 编辑器（内容区）阅读参数覆盖层 · AA 浮层（2026-09-23 晚）

按上述设计落地完整**两层覆盖模型**：知识页区块栏新增「AA」按钮（`IconButton` format_size）→ 浮层内三个独立控件（字体/字号/行距），每项都可「跟随全局」重置；值 `实际 = 编辑器覆盖(可空) ?? 全局`。

**存储**（app_meta）：
- 全局层保持 `theme_font / theme_font_size`（未来全局行距未做，回落 vendor 默认 1.5）。
- 覆盖层新增 `theme_editor_font / theme_editor_font_size / theme_editor_line_height`，均为可空；`update_theme_prefs` 传 `None` 时经新增的 `Store::remove_meta` 删除对应键（覆盖态回归继承态，两层互不污染）。
- Rust `ThemePrefsDto` 增三个 `Option` 字段 + `get/update_theme_prefs` 扩展；regen 桥接 + release lib 重编（hash 一致）。

**模型/Provider**：
- `AppSettings` 增 `editorFontName / editorFontSize / editorLineHeight`（可空）；求值 getter `contentFontSize`（`editorFontSize ?? fontSize`，钳 12–24）、`contentLineHeight`（`editorLineHeight ?? 1.5`，钳 1.0–2.5）、`contentFontName`。
- `copyWithEditorSettings({font?, fontSizeOverride?, lineHeightOverride?})`：用 const 哨兵 `_unset` 区分「不改动」与「显式置空」，写入时钳制越界值；普通 `copyWith` 保留覆盖层（改全局不冲掉覆盖）。
- Provider：`updateEditorFont/FontSize/LineHeight`（传 null = 跟随全局）+ `loadThemeFromBridge`/`saveTheme` 带覆盖层。

**渲染（求值点收敛于 fontAware）**：
- `fontAwareTextStyleConfiguration(family, {color, fontSize, lineHeight})` 新增 `lineHeight`，落在配置顶层（vendor `appflowy_rich_text` 用 `text.copyWith(height: lineHeight)` 渲染，实测 height 生效）。
- `WikiContentEditor` 增 `fontFamily`（null=跟随全局→回退主题 textTheme 的全局字体解析值；`AppFonts.system`=显式跟随系统不注入；家族名经 `GoogleFonts.getFont` 解析加载，与全局字体同一机制）与 `lineHeight` 参数。详情页接线：`fontFamily: s.editorFontName`、`fontSize: s.contentFontSize`、`lineHeight: s.contentLineHeight`。

**UI（AA 浮层）**：新组件 `wiki_reading_settings_dialog.dart`（`showWikiReadingSettings(context)`）。字体行：当前值 + FontPicker 对话框 + 跟随全局 chip；字号/行距行：滑块（字号 12–24 步进 1、行距 1.0–2.5 步进 0.1），拖拽实时预览、松手 `saveTheme`；每项「跟随全局」chip 高亮 = 覆盖为空，点按清空即回归继承。

**测试**（全绿）：
- `settings_reading_layer_test.dart`：两层求值（覆盖 ?? 全局）、显式清空、越界钳制、copyWith 原子性、fontAware 行距注入/回落（7 项）。
- `wiki_content_editor_reading_layer_test.dart`：编辑器层渲染 fontFamily 覆盖（经 GoogleFonts 解析的内部家族名）、system 不注入、lineHeight 2.0/默认 1.5（4 项）。
- `wiki_reading_settings_dialog_test.dart`：浮层三项控件、拖动滑块进入覆盖态、跟随全局重置、字体项打开 FontPicker（5 项，真实隔离桥接共享 repo）。
- `settings_bridge_test.dart`：覆盖层写入 + 跟随全局清除 roundtrip（1 项）。
- 回归：font_test / font_size_test / theme_color_test / settings_screen_test / wiki_editor_integration_test（含移动端走查）全绿。

**协作交接**（与并行会话）：`wiki_content_editor.dart`（fontFamily/lineHeight 参数 + fontAware lineHeight 注入）与 `wiki_page_detail_view.dart`（AA 按钮 + 两层求值接线）两文件内我的本次改动未单独提交，随并行会话的重构提交一起落库（工作区即测即用）；`src`、`settings.dart/provider`、桥接生成面、浮层组件与三个新测试文件已随本次提交。

**后续修复**（随并行提交落库）：
- `main_screen.dart` 右侧内容区原为 `Container(color: surface0, border: left)` 直接包页面——派生产物 `ExpansionTile`（其内部 ListTile `onTap: _tileController.expand` tearoff，`tilePadding: zero`）与 relations 页签 `CheckboxListTile` 等裸 ListTile 最近 Material 在 Scaffold，中间隔带底色 DecoratedBox → 运行时断言「ink splashes may be invisible」（报错 DecoratedBox 的 `surface3@0.65` 左框即该容器）。已在 `main_screen.dart` 把背景色移到内层 `Material(color: surface0)`，外层 Container 只留左边框——ListTile 有最近 Material 兜底且波纹可见，视觉不变。`wiki_ai_chat_panel.dart` 内无 ListTile 不受影响。
- `wiki_page_detail_view.dart` 区块栏：内容页签右侧改固定组 `Row[min]`（编辑操作在左、AA「正文阅读设置」永远贴最右）——旧布局 `spaceBetween` 三子项（TabBar/AA/编辑操作）导致 `_canEdit` 显隐编辑按钮时 AA 在中线与最右之间跳。现 AA 位置恒定；`wiki_editor_integration_test` 16 项回归全绿。
