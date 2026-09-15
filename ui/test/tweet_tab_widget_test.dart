import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/tweet_fetch.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

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
    container.read(wikiOpenTabsProvider.notifier).state = [
      const ImportTabEntry(),
      const TweetTabEntry(fetch: fetch),
    ];
    container.read(wikiActiveTabIdProvider.notifier).state = 'tweet-20';

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: WikiPageDetailView()),
        ),
      ),
    );
    await tester.pump();

    // tab 条：推文导入 + 推文 20
    expect(find.text('推文导入'), findsOneWidget);
    expect(find.text('推文 20'), findsOneWidget);

    // 内容区：作者标题 + 原文卡片 + 原文文本 + 保存按钮
    expect(find.text('jack 的推文'), findsOneWidget);
    expect(find.text('抓取内容（原文）'), findsOneWidget);
    expect(find.textContaining('just setting up my twttr'), findsOneWidget,
        reason: '抓取到的原文应展示在 tab 内');
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
    container.read(wikiOpenTabsProvider.notifier).state = [
      const ImportTabEntry(),
      const TweetTabEntry(fetch: fetch),
    ];
    container.read(wikiActiveTabIdProvider.notifier).state = 'tweet-2099411545117831668';

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const MaterialApp(
          home: Scaffold(body: WikiPageDetailView()),
        ),
      ),
    );
    await tester.pump();

    // 头部主标题 = 文章标题（而非「伟大 的推文」），原文卡片展示正文段落
    expect(find.text('为什么你手握 Codex、Claude，依然赚不到钱？'), findsOneWidget);
    expect(find.text('伟大 的推文'), findsNothing);
    expect(find.textContaining('过去，一个人赚不到钱'), findsOneWidget);
    expect(find.textContaining('这些理由过去确实成立'), findsOneWidget);
    expect(find.textContaining('x.com/i/article'), findsNothing, reason: '不应显示文章链接');
  });
}