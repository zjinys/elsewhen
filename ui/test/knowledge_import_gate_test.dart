import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/import_fetch.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_import.dart';

import 'knowledge_flow_test.dart' show FakeKnowledge;

/// 孵化 UI 段（提案 2026-10-19）：导入命中负知识的知情确认——
/// 重新导入是重新纳入的唯一路径（reject → endorse 不可达），提示不拦截，
/// 但否定不得被静默绕过。
Future<void> pumpImport(WidgetTester tester, FakeKnowledge repo) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: [knowledgeRepositoryProvider.overrideWithValue(repo)],
      child: MaterialApp(
        home: Scaffold(
          body: Consumer(
            builder: (context, ref, _) => Center(
              child: ElevatedButton(
                onPressed: () => saveKnowledgeImport(
                  context,
                  ref,
                  const ImportFetch(
                    sourceUrl: 'https://example.com/a',
                    sourceKind: 'webpage',
                    contentMd: '正文',
                  ),
                  const [],
                ),
                child: const Text('导入'),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('导入命中负知识：先亮否定原因，取消不保存、确认才保存', (tester) async {
    final repo = FakeKnowledge();
    repo.priorRejection = '来源不可信，曾编造数据';
    repo.previewChanged = false;
    await pumpImport(tester, repo);
    await tester.tap(find.text('导入'));
    await tester.pumpAndSettle();
    expect(find.text('此前否定过这条来源'), findsOneWidget);
    expect(find.textContaining('来源不可信'), findsOneWidget);
    // 取消：不落库，否定不被静默绕过。
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(repo.saves, 0);
    // 确认：重新导入放行（重新纳入的唯一路径）。
    await tester.tap(find.text('导入'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('仍要保存'));
    await tester.pumpAndSettle();
    expect(repo.saves, 1);
    expect(tester.takeException(), isNull);
  });

  testWidgets('导入未命中负知识：不弹否定提示，直接落库', (tester) async {
    final repo = FakeKnowledge();
    repo.previewChanged = false;
    await pumpImport(tester, repo);
    await tester.tap(find.text('导入'));
    await tester.pumpAndSettle();
    expect(find.text('此前否定过这条来源'), findsNothing);
    expect(repo.saves, 1);
    expect(tester.takeException(), isNull);
  });
}
