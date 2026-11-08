import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  late RustBridgeRepository repository;
  setUpAll(() async {
    repository = await createIsolatedBridge();
  });

  testWidgets('MCP intent keywords edit roundtrips through the real bridge', (
    tester,
  ) async {
    final id = await tester.runAsync(
      () => repository.saveMcpServer(
        const api.McpServerDto(
          id: '',
          name: 'social-kit',
          keywords: '小红书,RED',
          platform: 'all',
          transport: 'stdio',
          command: 'fixture',
          argsJson: '[]',
          envJson: '{}',
          url: '',
          headersJson: '{}',
          enabled: true,
          trusted: false,
          allowReadonlyAuto: false,
        ),
      ),
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repository)],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('MCP'));
    await tester.pumpAndSettle();
    expect(find.text('关联意图：小红书,RED'), findsOneWidget);
    await tester.tap(find.byIcon(Icons.more_horiz));
    await tester.pumpAndSettle();
    await tester.tap(find.text('编辑'));
    await tester.pumpAndSettle();
    final field = find.byKey(const ValueKey('mcp-intent-keywords'));
    expect(tester.widget<TextField>(field).controller!.text, '小红书,RED');
    await tester.enterText(field, ' 小红书，种草, 卖货,种草 ');
    final save = find.widgetWithText(FilledButton, '保存');
    await tester.ensureVisible(save);
    await tester.tap(save);
    for (
      var attempt = 0;
      attempt < 50 && find.text('关联意图：小红书,种草,卖货').evaluate().isEmpty;
      attempt++
    ) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump();
    }
    await tester.pumpAndSettle();
    final saved = (await tester.runAsync(() => repository.listMcpServers()))!
        .single;
    expect(saved.id, id);
    expect(saved.keywords, '小红书,种草,卖货');
    expect(saved.command, 'fixture');
    expect(saved.trusted, isFalse);
    expect(saved.allowReadonlyAuto, isFalse);
    expect(find.text('关联意图：小红书,种草,卖货'), findsOneWidget);
  });
}
