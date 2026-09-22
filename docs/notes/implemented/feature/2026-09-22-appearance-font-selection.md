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
