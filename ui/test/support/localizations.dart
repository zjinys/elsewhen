import 'package:material_ui/material_ui.dart';

import 'package:elsewhen_ui/l10n/app_localizations.dart';

/// 应用本地化代理的标准组合（测试用；生产入口见 lib/main.dart）。
///
/// Flutter 3.50 迁移注意：`material_ui` 的 `MaterialApp` 会自动注入
/// material_ui/cupertino_ui 的 `Default*Localizations`（en-only）。
/// gen-l10n 生成文件里的 `AppLocalizations.localizationsDelegates` 来自
/// 框架 `flutter_localizations` 包，其 `MaterialLocalizations` /
/// `CupertinoLocalizations` 与 mui/cui 包是同名不同库的类（不同 `Type`），
/// 无法覆盖注入的 en-only 代理——混用会让 zh 环境报
/// 「not supported by all delegates」且 Material 文案静默回落英文。
/// 因此这里不使用生成文件的 `localizationsDelegates`，改用 mui 的
/// `GlobalMaterialLocalizations.delegates`（含 mui material + cui cupertino
/// + widgets 三件套）。
const List<LocalizationsDelegate<dynamic>> appTestLocalizationDelegates =
    <LocalizationsDelegate<dynamic>>[
  AppLocalizations.delegate,
  ...GlobalMaterialLocalizations.delegates,
];

/// 供测试构建 MaterialApp 时直接使用的完整 delegates（含编辑器代理时请自行追加）。
Iterable<LocalizationsDelegate<dynamic>> appTestDelegates() =>
    appTestLocalizationDelegates;
