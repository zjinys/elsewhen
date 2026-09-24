# 2026-09-24 系统字体枚举：fc-list → Rust fontdb（桥内实现）

## 结论
**已迁移**：`SystemFontService.listFonts()` 从「spawn `fc-list` 进程」改为经
flutter_rust_bridge 调 Rust 侧 `list_system_fonts()`（fontdb 0.23 直接扫描）。
跨 Linux/macOS/Windows 一份代码，不再依赖外部命令。

## 为什么放弃 fc-list
- `fc-list` 是 fontconfig 的命令行工具，**只有 Linux 有**；macOS/Windows 上没有，
  原来的实现天然不可移植。
- 桌面应用 spawn 系统工具查字体，既不优雅也不可靠（PATH 依赖、i18n/格式差异）。
- 本仓库是 Flutter + flutter_rust_bridge 架构，字体枚举本该下沉到 Rust 侧。

## crate 选择：fontdb 而非用户点名的 font-kit
用户建议 font-kit；复核后选 **fontdb**，理由：
- **font-kit 的底层数据库层就是 fontdb**（它用 `fontdb::Database` 装载系统字体）。
- 我们要的数据是「家族名 + **字体文件路径** + 样式」枚举清单——`file` 路径是给
  Dart `FontLoader` 加载字节用的，fontdb 的 `FaceInfo.source`（`Source::File`）直接暴露；
  font-kit 外层 API 面向「按家族/字面取 Font 渲染」，不直接吐文件路径。
- fontdb 0.23 **已随 iced 在依赖树里**（`Cargo.lock` 早有 0.23.0），列为直接依赖
  零新增编译量；三平台均只扫描本地字体数据库，**全程不 spawn 任何进程**：
  - Linux：解析 fontconfig 配置（`/etc/fonts/fonts.conf` 等，自动含
    `/usr/share/fonts`、`/usr/local/share/fonts`、`~/.fonts`、`~/.local/share/fonts`），
    遍历 `<dir>` 递归扫描；
  - macOS：`/Library/Fonts`、`/System/Library/Fonts`、AssetsV2、`~/Library/Fonts`；
  - Windows：`%SystemRoot%\Fonts` + USERPROFILE 字体目录。

## 改动清单
- `Cargo.toml`：+ `fontdb = "0.23"`。
- `src/fonts.rs`（新模块）：`system_font_faces() -> Vec<(family, file, style)>`，
  fontdb 枚举 + `style_label()`（Weight/Style → fc-list 风格串，供 Dart `_styleRank` 复用）。
- `src/api.rs`：+ `SystemFontFace` DTO + `list_system_fonts()` 桥函数（既有 api.rs 模式）。
- `src/lib.rs` / `src/main.rs`：挂 `fonts` 模块（**bin 与 lib 两棵 crate 树都要加**——
  main.rs 是自组织 `mod ai; mod api; …` 的独立树，只改 lib.rs 会导致 bin 编译报
  `unresolved import crate::fonts`）。
- `ui/lib/system_fonts.dart`（重写枚举段）：
  - 删 `Process.run('fc-list')` + 5s timeout，改 `api.listSystemFonts()`；
  - 家族去重 / `_styleRank` 首选样式 / 字母序排序逻辑原样保留（抽成 `_buildEntries`），
    行为与旧 fc-list 时期一致；
  - `_isTestEnvironment` 守卫保留：测试（FakeAsync）里不碰真实 FFI，直接空列表。
- `regen.sh` 全量重生成桥 + 重编 release `.so`（保持 rustContentHash 一致）。

## 验证
- 原生 `cargo test --lib fonts::` 2/2 过；`cargo check --all-targets` 干净。
- 原生枚举实测：**1113 张字面**，`LXGW WenKai Mono`（2 个字面）、Noto Sans Mono CJK SC、
  JetBrains Mono 等全部在列（fc-list 报 1228，差异是配置条目 vs 字面粒度）。
- Dart 侧：`flutter analyze` 改动文件 0 issue；字体相关 widget 测试（settings_screen /
  settings_reading_layer / wiki_reading_settings_dialog）14 个全过；
  全量 `flutter test` **155 过 / 6 挂**——6 挂恰为并行基准（message_copy/generating/
  retry/window + wiki_area_filter/wiki_ingest_placeholder），零新增失败。

## 坑位备忘
- **flutter test 会注入 `FONTCONFIG_FILE`** 指向 flutter_tools 临时目录里的测试字体配置
  （只有 Roboto/Roboto Condensed/Material Icons 共 19 张）。因此「在 widget test 里调
  桥枚举」只会看到这 19 张——不是实现坏了，是测试环境有意隔离字体。真机运行时该变量
  不存在，走完整 fontconfig 配置。这也是测试守卫必须保留的又一个理由。
- frb 桥再生成会整体重写 `frb_generated.rs` 与 `ui/lib/bridge/generated.dart/*`：
  并行会话的桥改动正是同一条生成链（其 api.rs 为当前源），本次再生成后全量测试通过，
  说明生成链一致、无互相覆盖。

## 残留风险
- macOS/Windows 未实测（依赖 crate 内部目录扫描，理论上无需真机验证逻辑）。
- Dart 聚合把 `_styleRank` 保留为「字符串匹配 fc-list 风格标签」：fontdb 风格串是对标
  fc-list 手写的（Weight 400→"Regular"、700→"Bold"，Normal→无后缀等），若以后需要
  更精细（如可变字体轴），可在 Rust 侧把 weight/style 结构化字段直接透传。