import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/widgets/named_secret_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  testWidgets(
    'name-only edit keeps key, rejects empty title and duplicate code',
    (tester) async {
      await tester.runAsync(() => createIsolatedBridge());
      await tester.runAsync(
        () => api.saveNamedSecret(
          name: 'stable',
          title: '旧名称',
          value: 'synthetic-secret',
        ),
      );
      await tester.pumpWidget(
        const MaterialApp(home: Scaffold(body: NamedSecretSettings())),
      );
      Future<void> settle() async {
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 200)),
        );
        await tester.pumpAndSettle();
      }

      await settle();
      await tester.tap(find.byTooltip('编辑密钥'));
      await tester.pumpAndSettle();
      final name = find.byWidgetPredicate(
        (w) => w is TextField && w.decoration?.labelText == '名称 *',
      );
      await tester.enterText(name, '');
      await tester.tap(find.widgetWithText(FilledButton, '保存'));
      await tester.pump();
      expect(find.text('请填写名称'), findsOneWidget);
      await tester.enterText(name, '新名称');
      await tester.tap(find.widgetWithText(FilledButton, '保存'));
      await settle();
      await settle();
      expect(find.text('新名称'), findsOneWidget);
      final secrets = await tester.runAsync(() => api.listNamedSecrets());
      expect(secrets!.single.name, 'stable');
      expect(secrets.single.title, '新名称');
      await tester.runAsync(() async {
        await expectLater(
          api.saveNamedSecret(name: 'stable', title: '重复', value: 'other'),
          throwsA(anything),
        );
      });
    },
  );
}
