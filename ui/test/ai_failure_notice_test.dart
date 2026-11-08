import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/l10n/notice_messages.dart';
import 'package:flutter/widgets.dart';

void main() {
  final en = lookupAppLocalizations(const Locale('en'));
  final zh = lookupAppLocalizations(const Locale('zh'));
  test('provider guidance remains localized', () {
    final error = _FakeAnyhow('No active AI provider');
    expect(aiFailureNotice(error, strings: en), en.aiProviderMissing);
    expect(aiFailureNotice(error), zh.aiProviderMissing);
  });
  test('unknown AI failures have localized summary and retain every diagnostic line', () {
    for (final error in [
      _FakeAnyhow('401 Unauthorized'),
      Exception('网络请求失败\n第二行细节'),
      _FakeAnyhow(
        'Failed to send request\nCaused by:\n0: Connect\n1: Connection timed out',
      ),
    ]) {
      expect(aiFailureNotice(error, strings: en), en.aiReplyFailedGeneric);
      expect(aiFailureNotice(error), zh.aiReplyFailedGeneric);
      final token = appFailure(error, key: 'aiReplyFailedGeneric');
      expect(resolveNotice(token, en), en.aiReplyFailedGeneric);
      expect(resolveNotice(token, zh), zh.aiReplyFailedGeneric);
      expect(noticeDiagnostic(token), error.toString());
    }
  });
}

class _FakeAnyhow implements Exception {
  final String message;
  _FakeAnyhow(this.message);
  @override
  String toString() => 'AnyhowException($message)';
}
