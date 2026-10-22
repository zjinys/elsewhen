import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/date_symbol_data_local.dart';

const _editorLocale = Locale('zh', 'CN');

Future<AppFlowyEditorLocalizations> _loadEditorLocalizations() =>
    AppFlowyEditorLocalizations.load(_editorLocale);

/// AppFlowy editor 的块组件（段落占位符、斜杠菜单等）直接读取静态
/// [AppFlowyEditorL10n.current]，不会随 MaterialApp 的
/// `localizationsDelegates` 自动切换；不显式加载时始终是英文。
///
/// 当前 App 界面是中文，启动时加载同一套 zh-CN 文案。以后要做多语言时，
/// 把这里换成跟随用户 locale 的加载即可。
Future<void> configureEditorLocalization() async {
  // AppFlowy 的 load 会把 Intl.defaultLocale 设成 zh_CN；App 里大量
  // DateFormat 依赖它，必须先把中文日期符号表初始化好，否则运行到
  // 「最近 …」「MM-dd HH:mm」这类时间文本时会抛 LocaleDataException。
  await initializeDateFormatting('zh_CN');
  try {
    AppFlowyEditorL10n.current = await _loadEditorLocalizations();
  } catch (error, stack) {
    // 文案加载失败不应阻塞启动；编辑器退回 vendor 默认英文。
    debugPrint('configureEditorLocalization failed: $error\n$stack');
  }
}

/// 替换 vendor 的 [AppFlowyEditorLocalizations.delegate]。
///
/// MaterialApp 默认只支持 en_US，vendor delegate 会按 en 重新加载编辑器文案，
/// 把 `Intl.defaultLocale` 改回英文——即使启动时已经加载过 zh-CN。这里固定
/// 返回中文实例，避免 editor 文案被系统语言覆盖。
class ChineseAppFlowyEditorLocalizationsDelegate
    extends LocalizationsDelegate<AppFlowyEditorLocalizations> {
  const ChineseAppFlowyEditorLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) => true;

  @override
  Future<AppFlowyEditorLocalizations> load(Locale locale) =>
      _loadEditorLocalizations();

  @override
  bool shouldReload(
    covariant LocalizationsDelegate<AppFlowyEditorLocalizations> old,
  ) => false;
}
