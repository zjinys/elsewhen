# Agent Note: 外观设置字体选择（内建 + 系统字体枚举）

Status: implemented

## Problem

App 字体硬编码 Inter（google_fonts），中文走系统回退；用户需要可选字体，且固定四项不够，希望能直接列出系统已安装字体。

## Decision

**存值为自由字符串**（`app_meta.theme_font`，默认 `inter`）：内建特殊值 `inter` / `notoSansSc` / `notoSerifSc` / `system` 映射 google_fonts 或系统默认；其余一律按**系统字体族名**处理，`textTheme.apply(fontFamily:)` 直接套用——Flutter Linux 引擎经 fontconfig 解析，无需打包字体资产。`AppFontFamily` 枚举收缩为内建四项的元数据（displayName / builtinNames / displayNameOf）。

**系统字体枚举**：`systemFontFamiliesProvider` 用 `fc-list : family`（Linux only，其它平台空表）。两个解析细节：
1. fc-list 输出对家族名里的 `- , : \` 做反斜杠转义（如 `FZSongS\-Extended(SIP)`），必须反转义，否则字族名不匹配；
2. 一行多个别名按逗号分隔，但 `\,` 是转义逗号——按字符扫描分割，取首个别名展示。

**下拉 UX**：内建四项 + 分隔线 + 系统字体（条目用自身字体渲染预览）；当前值不在候选（字体被卸载）时补占位项防 DropdownButton 断言。选择即生效（themeKey 含 fontName 触发整树重建）并落库。

## Alternatives considered

- 枚举固定枚举值（初版只有 4 项）：用户明确要系统字体列表，弃。
- pubspec 打包字体文件：体积大、选择固定，不如 fontconfig 动态解析。
- 跨平台（macOS 无 fc-list）：当前 App 是 Linux 桌面，其它平台返回空表只显示内建项，够用。
- 加搜索框：列表 ~345 项但下拉可滚动，先保持简单。

## Consequences

外观 tab 字体下拉 = 4 内建 + 全部系统字体（本机 345 个）。中文用户可直接选 Noto Sans CJK / 文泉驿 / 霞鹜文楷等。注意 flutter test 运行器用内置 fonts.conf 隔离 fontconfig（测试里只能枚举到 3 个 Flutter 自带字体），provider 测试断言不依赖具体字体存在。测试：app_theme_test 3 项 + settings_font_test 1 项全绿；Rust 侧无改动（theme_font 本就存任意字符串）。