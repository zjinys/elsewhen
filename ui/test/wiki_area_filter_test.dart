import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

class _LibraryFake extends KnowledgeRepository {
  @override
  Future<api.LibraryPage> browse(
    String query,
    String? area,
    String? kind,
    String? tag,
    String? state,
    int offset,
  ) async => api.LibraryPage(
    items: query.isNotEmpty
        ? []
        : [
            const api.LibraryEntry(
              slug: 's',
              title: '测试素材',
              summary: '摘要',
              kind: 'source',
              area: 'imported',
              readingState: 'unread',
            ),
          ],
    hasMore: false,
  );
}

void main() {
  testWidgets(
    'empty area keeps filters available and can return to materials',
    (tester) async {
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            knowledgeRepositoryProvider.overrideWithValue(_LibraryFake()),
          ],
          child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('全部分区'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('素材库').last);
      await tester.pumpAndSettle();
      expect(find.text('测试素材'), findsOneWidget);
      await tester.enterText(find.byType(TextField).first, '不存在的关键词');
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pumpAndSettle();
      expect(find.text('没有匹配的知识页'), findsOneWidget);
      expect(find.text('素材库'), findsOneWidget);
      await tester.tap(find.byTooltip('清除搜索'));
      await tester.pumpAndSettle();
      expect(find.text('测试素材'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );
}
