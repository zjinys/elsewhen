import 'package:flutter/widgets.dart';

/// Display supports Chinese and English; all OS variants have a stable fallback.
/// null is an explicit system preference, not a missing/unloaded setting.
Locale resolveAppLocale(String? preference, List<Locale> systemLocales) {
  if (preference == 'en') return const Locale('en');
  if (preference != null) return const Locale('zh');
  for (final locale in systemLocales) {
    if (locale.languageCode == 'en') return const Locale('en');
    if (locale.languageCode == 'zh') return const Locale('zh');
  }
  return const Locale('zh');
}

String promptLocaleFor(Locale locale) =>
    locale.languageCode == 'en' ? 'en' : 'zh-Hans';
