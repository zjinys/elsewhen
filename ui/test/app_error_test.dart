import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/utils/app_error.dart';
import 'package:elsewhen_ui/widgets/app_error_notice.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final en = lookupAppLocalizations(const Locale('en'));
  final zh = lookupAppLocalizations(const Locale('zh'));
  const coded =
      'AnyhowException(elsewhen-error-v1:{"version":1,"code":"field_required","params":{"field":"title"}}\nCaused by:\nprivate /database/path)';

  test(
    'versioned codes localize parameters without displaying diagnostics',
    () {
      final parsed = AppError.decode(coded)!;
      expect(parsed.message(en), 'Title is required.');
      expect(parsed.message(zh), '标题不能为空。');
      for (final invalid in [
        'some text $coded',
        'elsewhen-error-v1:{"version":2,"code":"field_required","params":{}}',
        'elsewhen-error-v1:{"version":1,"code":"field_required","params":[]}',
        'elsewhen-error-v1:{"version":1,"code":"field_required","params":{"field":{}}}',
        'elsewhen-error-v1:not json',
      ]) {
        expect(AppError.decode(invalid), isNull);
      }
      expect(
        AppError.decode(
          'elsewhen-error-v1:{"version":1,"code":"new_code","params":{}}',
        )!.message(en),
        isNull,
      );
      expect(
        AppError.decode(
          'elsewhen-error-v1:{"version":1,"code":"field_required","params":{"field":"private /path"}}',
        )!.message(en),
        isNull,
      );
    },
  );

  test(
    'knowledge review and source changes have stable bilingual guidance',
    () {
      for (final code in [
        'knowledge_review_changed',
        'knowledge_source_changed',
      ]) {
        final error = AppError.decode(
          'elsewhen-error-v1:{"version":1,"code":"$code","params":{}}',
        )!;
        expect(error.message(en), contains('changed'));
        expect(
          error.message(zh),
          code == 'knowledge_review_changed'
              ? zh.errorKnowledgeReviewChanged
              : zh.errorKnowledgeSourceChanged,
        );
      }
    },
  );

  testWidgets(
    'unknown errors show localized fallback with explicit diagnostic details',
    (tester) async {
      const privateError = 'SELECT secret FROM /private/database';
      await tester.pumpWidget(
        MaterialApp(
          locale: const Locale('en'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                onPressed: () =>
                    showAppError(context, context.l10n.save, privateError),
                child: const Text('fail'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('fail'));
      await tester.pumpAndSettle();
      expect(find.text(en.operationFailed(en.save)), findsOneWidget);
      expect(find.textContaining('SELECT'), findsNothing);
      await tester.tap(find.text(en.errorDetails));
      await tester.pumpAndSettle();
      expect(find.text(privateError), findsOneWidget);
      expect(find.text(en.copy), findsOneWidget);
    },
  );
}
