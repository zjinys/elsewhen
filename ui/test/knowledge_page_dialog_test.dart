import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_page_dialog.dart';

void main() {
  final now = DateTime.now();
  final samplePage = WikiPage(
    id: 'test-id-1',
    slug: 'kb-Elsewhen-5ae954d0',
    kind: 'method',
    title: '知识页的修订方式',
    summary: '这是关于知识页如何修订的说明摘要',
    contentMd: '''
# 知识页修订指南

这是第一段说明。

- 步骤一：定位知识条目
- 步骤二：发起修订草稿
- 步骤三：确认并提交生效

更多内容请参考 [[topic/system-rules|系统规则]]。
''',
    tags: const ['规则', '维护'],
    sourceEventIds: const [],
    evidenceCount: 3,
    firstSeenAt: now,
    lastSeenAt: now,
    status: 'active',
    createdAt: now,
    updatedAt: now,
    area: 'insight',
  );

  for (final locale in [const Locale('zh'), const Locale('en')]) {
    final strings = lookupAppLocalizations(locale);
    group(locale.languageCode, () {
      testWidgets('KnowledgePageDialog 成功加载：展示类型、标题、slug、标签与 Markdown 正文', (
        tester,
      ) async {
        await tester.pumpWidget(
          ProviderScope(
            overrides: [
              wikiPageProvider('kb-Elsewhen-5ae954d0')
                  .overrideWith((ref) async => samplePage),
            ],
            child: MaterialApp(
              locale: locale,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: KnowledgePageDialog(slug: 'kb-Elsewhen-5ae954d0'),
              ),
            ),
          ),
        );

        await tester.pumpAndSettle();

        // 1. 标题与头部
        expect(
          find.text(strings.pageKindMethod),
          findsOneWidget,
        ); // kindLabel for 'method'
        expect(find.text('知识页的修订方式'), findsOneWidget);
        expect(find.text('kb-Elsewhen-5ae954d0'), findsOneWidget);

        // 2. 标签
        expect(find.text('#规则'), findsOneWidget);
        expect(find.text('#维护'), findsOneWidget);

        // 3. 正文 Markdown 元素
        expect(find.text('知识页修订指南'), findsOneWidget);
        expect(
          find.byWidgetPredicate(
            (w) => w is RichText && w.text.toPlainText().contains('这是第一段说明。'),
          ),
          findsWidgets,
        );
        expect(
          find.byWidgetPredicate(
            (w) => w is RichText && w.text.toPlainText().contains('步骤一：定位知识条目'),
          ),
          findsOneWidget,
        );

        // 4. 底部操作按钮
        expect(find.text(strings.openInKnowledgeBase), findsOneWidget);
        expect(find.text(strings.close), findsOneWidget);
      });

      testWidgets('KnowledgePageDialog 未找到页面：友好展示未找到提示与原因说明', (tester) async {
        await tester.pumpWidget(
          ProviderScope(
            overrides: [
              wikiPageProvider('kb-missing-slug')
                  .overrideWith((ref) async => null),
            ],
            child: MaterialApp(
              locale: locale,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: KnowledgePageDialog(
                  slug: 'kb-missing-slug',
                  fallbackTitle: '缺失的目标页面',
                ),
              ),
            ),
          ),
        );

        await tester.pumpAndSettle();

        expect(find.text(strings.knowledgePageMissing), findsOneWidget);
        expect(find.text('kb-missing-slug'), findsWidgets);
        expect(find.text(strings.knowledgePageMissingHelp), findsOneWidget);
        expect(find.text(strings.openInKnowledgeBase), findsNothing);
        expect(find.text(strings.close), findsOneWidget);
      });

      testWidgets('KnowledgePageDialog 点击「在知识库中打开」：激活 tab 并切换侧栏', (
        tester,
      ) async {
        final container = ProviderContainer(
          overrides: [
            wikiPageProvider('kb-Elsewhen-5ae954d0')
                .overrideWith((ref) async => samplePage),
          ],
        );
        addTearDown(container.dispose);

        await tester.pumpWidget(
          UncontrolledProviderScope(
            container: container,
            child: MaterialApp(
              locale: locale,
              supportedLocales: AppLocalizations.supportedLocales,
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: Builder(
                  builder: (context) => ElevatedButton(
                    onPressed: () => showKnowledgePageDialog(
                      context,
                      slug: 'kb:kb-Elsewhen-5ae954d0',
                    ),
                    child: const Text('打开弹窗'),
                  ),
                ),
              ),
            ),
          ),
        );

        // 点击按钮弹窗
        await tester.tap(find.text('打开弹窗'));
        await tester.pumpAndSettle();

        expect(find.text('知识页的修订方式'), findsOneWidget);

        // 点击「在知识库中打开」
        await tester.tap(find.text(strings.openInKnowledgeBase));
        await tester.pumpAndSettle();

        // 弹窗已关闭
        expect(find.byType(KnowledgePageDialog), findsNothing);

        // 校验侧栏标签已切到 wiki
        expect(container.read(sidebarTabProvider), SidebarTab.wiki);

        // 校验 wikiOpenTabsProvider 中已打开对应 page tab
        final tabs = container.read(wikiOpenTabsProvider);
        expect(
          tabs.any((t) => t.id == 'page-kb-Elsewhen-5ae954d0'),
          isTrue,
          reason: 'openWikiPageTab 应将该页面加入已打开 tab',
        );
      });
    });
  }
}
