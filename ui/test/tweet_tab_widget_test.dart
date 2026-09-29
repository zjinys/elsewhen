import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/tweet_fetch.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

/// 推文预览 tab 的本地渲染验证（不触达 FFI/网络）：
/// 打开抓取结果的 tab 后应展示内容原文、保存按钮与对话输入区。
void main() {
  testWidgets('fetch 成功后新开的推文预览 tab 渲染内容 + 保存 + 对话', (tester) async {
    const fetch = TweetFetch(
      tweetId: '20',
      url: 'https://twitter.com/jack/status/20',
      text: 'just setting up my twttr',
      authorName: 'jack',
      screenName: 'jack',
    );

    final container = ProviderContainer();
    addTearDown(container.dispose);
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      const TweetTabEntry(fetch: fetch),
    ]);
    container.read(wikiActiveTabIdProvider.notifier).set('tweet-20');

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();

    // tab 条：首页 + 推文 20
    expect(find.text('首页'), findsOneWidget);
    expect(find.text('推文 20'), findsOneWidget);

    // 内容区：作者标题 + 原文卡片 + 原文文本 + 保存按钮
    expect(find.text('jack 的推文'), findsOneWidget);
    expect(find.text('抓取内容（原文）'), findsOneWidget);
    expect(
      find.textContaining('just setting up my twttr'),
      findsOneWidget,
      reason: '抓取到的原文应展示在 tab 内',
    );
    expect(find.text('保存到知识库'), findsOneWidget);

    // 对话区：引导语 + 输入框 + 发送按钮
    expect(find.textContaining('与 AI 讨论这篇推文'), findsOneWidget);
    expect(find.textContaining('就这篇推文问问 AI'), findsOneWidget);
    expect(find.byIcon(Icons.arrow_upward), findsOneWidget);
  });

  testWidgets('文章型推文：标题进头部，正文段落展示在内容区', (tester) async {
    const fetch = TweetFetch(
      tweetId: '2099411545117831668',
      url: 'https://x.com/Huouo908070/status/2099411545117831668',
      text: '过去，一个人赚不到钱，往往还能找到很多具体的理由。\n\n这些理由过去确实成立。',
      title: '为什么你手握 Codex、Claude，依然赚不到钱？',
      authorName: '伟大',
      screenName: 'Huouo908070',
    );

    final container = ProviderContainer();
    addTearDown(container.dispose);
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      const TweetTabEntry(fetch: fetch),
    ]);
    container
        .read(wikiActiveTabIdProvider.notifier)
        .set('tweet-2099411545117831668');

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();

    // 头部主标题 = 文章标题（而非「伟大 的推文」），原文卡片展示正文段落
    expect(find.text('为什么你手握 Codex、Claude，依然赚不到钱？'), findsOneWidget);
    expect(find.text('伟大 的推文'), findsNothing);
    expect(find.textContaining('过去，一个人赚不到钱'), findsOneWidget);
    expect(find.textContaining('这些理由过去确实成立'), findsOneWidget);
    expect(
      find.textContaining('x.com/i/article'),
      findsNothing,
      reason: '不应显示文章链接',
    );
  });

  testWidgets('AI 整理为 Markdown：保留原文，整理稿单独保存并关联来源', (tester) async {
    const fetch = TweetFetch(
      tweetId: '20',
      url: 'https://twitter.com/jack/status/20',
      text: 'just setting up my twttr',
      authorName: 'jack',
      screenName: 'jack',
    );
    final repo = _FakeBeautifyRepo();
    final sources = _FakeSourceRepo();

    final container = ProviderContainer(
      overrides: [
        storageRepositoryProvider.overrideWithValue(repo),
        knowledgeRepositoryProvider.overrideWithValue(sources),
      ],
    );
    addTearDown(container.dispose);
    container.read(wikiOpenTabsProvider.notifier).set([
      const ImportTabEntry(),
      const TweetTabEntry(fetch: fetch),
    ]);
    container.read(wikiActiveTabIdProvider.notifier).set('tweet-20');

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();

    // 触发整理 → 显示预览，并说明原文与整理稿分别保存。
    await tester.ensureVisible(find.text('AI 整理为 Markdown'));
    await tester.pump();
    await tester.tap(find.text('AI 整理为 Markdown'));
    await tester.pump();
    await tester.pump();
    expect(find.text('AI 整理结果（Markdown 预览）'), findsOneWidget);
    expect(find.text('保存原文，整理稿另存为产物'), findsOneWidget);
    expect(repo.chatContent, fetch.fullContent, reason: '整理请求以上下文+固定指令发起');

    // 保存：来源原文保持完整，派生正文关联该来源。
    await tester.tap(find.text('保存到知识库'));
    await tester.pump();
    await tester.pump();
    expect(sources.savedRaw, fetch.text);
    expect(repo.basedOnSlug, 'source/jack-20');
    expect(
      repo.savedText,
      _FakeBeautifyRepo.beautified,
      reason: '整理后的 Markdown 保存为派生产物',
    );
  });
}

/// 假仓库：固定返回整理后的 Markdown，并记录保存入参（不触达 FFI/网络）
class _FakeBeautifyRepo extends RustBridgeRepository {
  static const beautified = '## 整理后\n\n- 要点一\n- 要点二';

  String? chatContent;
  String? savedText;
  String? basedOnSlug;

  @override
  Future<String> generateContentChat({
    required String content,
    required List<ContentChatMessage> messages,
  }) async {
    chatContent = content;
    return beautified;
  }

  @override
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async =>
      const [];

  @override
  Future<WikiPage> createWikiDerivative({
    required String basedOnSlug,
    required String contentType,
    required String title,
    required String contentMd,
  }) async {
    this.basedOnSlug = basedOnSlug;
    savedText = contentMd;
    return WikiPage(
      id: 'p1',
      slug: 'source/jack-20',
      kind: 'source',
      title: 'jack 的推文',
      summary: '',
      contentMd: contentMd,
      tags: const [],
      sourceEventIds: const [],
      evidenceCount: 0,
      firstSeenAt: DateTime(2025),
      lastSeenAt: DateTime(2025),
      status: 'active',
      createdAt: DateTime(2025),
      updatedAt: DateTime(2025),
      area: 'imported',
    );
  }
}

class _FakeSourceRepo extends KnowledgeRepository {
  String? savedRaw;
  @override
  Future<api.SourceUpdatePreview> preview(String url, String content) async =>
      const api.SourceUpdatePreview(changed: false);
  @override
  Future<api.WikiPageDto> confirmSource({
    required String title,
    required String contentMd,
    required String sourceUrl,
    required String sourceKind,
    required List<String> tags,
    String? expectedSnapshotId,
  }) async {
    savedRaw = contentMd;
    return api.WikiPageDto(
      id: 'p1',
      slug: 'source/jack-20',
      kind: 'source',
      title: title,
      summary: '',
      contentMd: contentMd,
      tags: [],
      sourceEventIds: [],
      evidenceCount: 0,
      firstSeenAt: '2025-01-01',
      lastSeenAt: '2025-01-01',
      status: 'active',
      createdAt: '2025-01-01',
      updatedAt: '2025-01-01',
      area: 'imported',
    );
  }
}
