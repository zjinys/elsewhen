# 中英文覆盖与例外清单

2026-11-08：承接[多语言路线](../roadmap/2026-11-06-multilingual-support-roadmap.md)的 L6。应用中英文资源当前为 **2018 项**。工程覆盖、真模型质量、真人设备验收分别记录；L7 尚未完成。

| 领域 | 工程覆盖 |
|---|---|
| 主界面、对话、目标、profile 讨论 | 导航、输入、运行/审阅、工具进度、恢复提示、原文复制；不新增 profile 入口 |
| 模型、密钥、Prompt | Provider 编辑/密钥选择、模板及版本、偏好与运行详情、知识语言权限/CAS |
| 待办、习惯、提醒 | 列表/编辑/状态/优先级/周计划/日期/问候/错误；保留用户内容 |
| 知识库、导入、派生、实体 | 分页/筛选/阅读/原文版本、导入/预览、编辑/关系/标签、派生采用与合并 |
| 知识维护 | 来源修复、提案/审阅/历史、处理队列、数据及后台日志；状态本地化，完整处理说明按原文查看 |
| 同步 | 配对/加入/owner/名册/移交、数据集切换、进度/积压、Provider/知识页/对象冲突 |
| 设置 | 搜索、代理/DoH、技能/MCP、规则/关系/情绪、数据维护及用量 |
| 平台 | 托盘热切换、锁屏生物识别原因、Android 通知标题/频道/内容；更新展示不改变服务生命周期 |
| 错误 | Rust 版本化 code/params、Dart 延迟解析通知、未知错误本地化回退及原始诊断详情；不按中文报错内容替换 |

运行以下检查：

```sh
python3 scripts/check-localization.py
python3 -m unittest discover -s scripts -p test_localization.py
cd ui
fvm dart run tool/check_localization.dart
fvm flutter test test/check_localization_test.dart
```

ARB 检查覆盖 key、占位符/类型、ICU 分支和复数数值参数。AST 检查遍历应用 Dart，包含手写 l10n 辅助代码、相邻字符串、插值及插值内嵌套表达式。新增文件、新增中文文案、例外数量变化和失效例外均会失败。CI 执行两类检查及负例；负例验证缺译、占位符/ICU 损坏、新增文件/出现次数及错误例外分类。英文复数显式插入 `{count}`，避免 Flutter 生成器把 `#` 当作字面值。

仅排除实际生成的 l10n/FRB 文件和 `.g.dart`，不排除整个 l10n 目录；测试/vendor 不属于应用源码。剩余 **39 处、34 种** 中文字符串全部在 [精确例外清单](../../scripts/localization-exceptions.json) 记录路径、内容、次数、封闭分类与理由：

- protocol：历史工具/产物类型/结构标记与兼容解析，不改 wire 值。
- audit：`GUI 编辑`、快记等已落盘修订原因，不随显示语言变化。
- identity：主对话历史识别值，不以翻译改变持久身份。
- diagnostic：日志和异常原文；普通界面使用本地化回退，详情保留原文。

用户事件/页面/profile/引文/自定义 Prompt、第三方技能和 MCP 说明保留原文；应用外层标签仍使用资源。历史 Prompt/bundle 不重写。静态检查只检查应用常量，不能证明运行时第三方内容的语言或真实模型质量。

2026-11-08 验证：Flutter **600 passed**（`/tmp/l6-ui-delivery.log`）；Rust all-targets 库1130+集成2，即 **1132 passed / 20 ignored**（`/tmp/l6-rust-delivery.log`）；`fvm flutter analyze lib tool` 无问题（`/tmp/l6-analyze-delivery.log`）；release 通过（`/tmp/l6-release-delivery.log`）；FRB 两侧 hash `-479447412`。覆盖门禁39处例外通过（`/tmp/l6-ast-delivery.log`），Python负例3项通过。专项18项检查错误热切换/诊断、英文380px同步审批及Provider冲突、队列原文详情、处理结果计数/人工保护，继续核验原 token、用户名称、掩码、禁止隐式写入和重试操作。

Android 原生 `:app:compileDebugKotlin -x :app:compileFlutterBuildDebug -Ptarget-platform=android-arm64` 通过（`/tmp/l6-android-arm64.log`，116 tasks）。这是原生 arm64 编译检查，未宣称完整APK/其他ABI或实机验收。2026-11-08 `adb devices -l` 无设备，L7 设备/多设备/真模型验收保持未完成。
