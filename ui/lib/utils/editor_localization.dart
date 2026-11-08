import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

// Vendor loading changes Intl.defaultLocale. Serialize loads so an older
// asynchronous request cannot overwrite a more recent display selection.
Future<void> _editorLoads = Future.value();
Locale? _loadedLocale;
int _pendingLoads = 0;

Future<AppFlowyEditorLocalizations> _loadEditorLocalizations(Locale locale) {
  final editorLocale = locale.languageCode == 'en'
      ? const Locale('en')
      : const Locale('zh', 'CN');
  if (_pendingLoads == 0 &&
      _loadedLocale == editorLocale &&
      Intl.getCurrentLocale() == editorLocale.toString()) {
    return SynchronousFuture(AppFlowyEditorL10n.current);
  }
  _pendingLoads++;
  final next = _editorLoads.then((_) async {
    await initializeDateFormatting(editorLocale.toString());
    final l10n = await AppFlowyEditorLocalizations.load(editorLocale);
    AppFlowyEditorL10n.current = l10n;
    _loadedLocale = editorLocale;
    return l10n;
  });
  _editorLoads = next.then<void>(
    (_) {
      _pendingLoads--;
    },
    onError: (Object _, StackTrace _) {
      _pendingLoads--;
    },
  );
  return next;
}

/// Static editor strings and Intl must use the same locale as the application.
Future<void> configureEditorLocalization([
  Locale locale = const Locale('zh'),
]) async {
  await _loadEditorLocalizations(locale);
}

class AppEditorLocalizationsDelegate
    extends LocalizationsDelegate<AppFlowyEditorLocalizations> {
  const AppEditorLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) =>
      locale.languageCode == 'en' || locale.languageCode == 'zh';

  @override
  Future<AppFlowyEditorLocalizations> load(Locale locale) =>
      _loadEditorLocalizations(locale);

  @override
  bool shouldReload(AppEditorLocalizationsDelegate old) => false;
}
