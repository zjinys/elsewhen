// M3 集成测试（§6 / §7）：
//  1. 详情页正文从 MarkdownView 换为 AppFlowy 编辑器（浏览态只读渲染）；
//  2. 「编辑正文」→ 编辑态 →「完成」保存：documentToMarkdown → saveWikiPageContent；
//  3. Ctrl+S 保存（HardwareKeyboard 全局监听）；
//  4. 「取消」丢弃未保存修改；
//  5. 保存失败：留在编辑态并展示错误条；
//  6. 素材页（kind=source/note）不显示编辑入口；
//  7. 聊天移入正文尾部对话块（页尾仅一个聊天面板），footer 不再有独立聊天面板；
//  8. wikilink 点击跳转目标页 tab；
//  9. 关闭有未保存修改的 tab 前弹确认。
import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:collection/collection.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/bridge/generated.dart/api.dart' show EntityFactDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/relation.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';
import 'package:elsewhen_ui/widgets/wiki_derivatives.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';

const _testMd = '# 测试标题\n\n正文段落提到[[人物/张三|张三]]';

/// 带代码块的正文字样（§11 Q4 降级展示）
const _codeMd = '# 带代码\n\n```dart\nfinal x = 1;\n```\n\n尾部段落。';

final _testPage = WikiPage(
  id: 'p1',
  slug: 'topic/测试',
  kind: 'topic',
  title: '测试页面',
  summary: '一句话摘要',
  contentMd: _testMd,
  tags: const ['test'],
  sourceEventIds: const ['ev1'],
  evidenceCount: 1,
  firstSeenAt: DateTime(2025, 1, 1),
  lastSeenAt: DateTime(2025, 1, 2),
  status: 'active',
  createdAt: DateTime(2025, 1, 1),
  updatedAt: DateTime(2025, 1, 2),
  area: 'insight',
);

final _sourcePage = WikiPage(
  id: 'p2',
  slug: 'tweet-1',
  kind: 'source',
  title: '素材推文',
  summary: '素材',
  contentMd: '# 素材\n\n不可编辑',
  tags: const [],
  sourceEventIds: const ['ev2'],
  evidenceCount: 1,
  firstSeenAt: DateTime(2025, 2, 1),
  lastSeenAt: DateTime(2025, 2, 2),
  status: 'active',
  createdAt: DateTime(2025, 2, 1),
  updatedAt: DateTime(2025, 2, 2),
  area: 'imported',
);

final _wikilinkTarget = WikiPage(
  id: 'p3',
  slug: '人物/张三',
  kind: 'person',
  title: '张三',
  summary: '',
  contentMd: '# 张三',
  tags: const [],
  sourceEventIds: const [],
  evidenceCount: 0,
  firstSeenAt: DateTime(2025, 3, 1),
  lastSeenAt: DateTime(2025, 3, 1),
  status: 'active',
  createdAt: DateTime(2025, 3, 1),
  updatedAt: DateTime(2025, 3, 1),
  area: 'network',
);

/// 带代码块正文的页面（M4 §11 Q4）
final _codePage = WikiPage(
  id: 'p4',
  slug: 'topic/带代码',
  kind: 'topic',
  title: '带代码页面',
  summary: '',
  contentMd: _codeMd,
  tags: const [],
  sourceEventIds: const [],
  evidenceCount: 0,
  firstSeenAt: DateTime(2025, 4, 1),
  lastSeenAt: DateTime(2025, 4, 1),
  status: 'active',
  createdAt: DateTime(2025, 4, 1),
  updatedAt: DateTime(2025, 4, 1),
  area: 'insight',
);

void main() {
  late _FakeRepo repo;
  late ProviderContainer container;

  Future<void> pumpDetail(WidgetTester tester, {WikiPage? page}) async {
    final p = page ?? _testPage;
    repo = _FakeRepo(page: p, wikilinkTarget: _wikilinkTarget);
    container = ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    );
    container.read(wikiOpenTabsProvider.notifier).state = [
      ImportTabEntry(),
      PageTabEntry(slug: p.slug, title: p.title),
    ];
    container.read(wikiActiveTabIdProvider.notifier).state = 'page-${p.slug}';
    addTearDown(container.dispose);

    tester.view.physicalSize = const Size(1600, 2200);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          localizationsDelegates: const [
            DefaultMaterialLocalizations.delegate,
            DefaultWidgetsLocalizations.delegate,
            AppFlowyEditorLocalizations.delegate,
          ],
          home: const Scaffold(body: WikiPageDetailView()),
        ),
      ),
    );
    await tester.pump(); // wikiPageProvider 等异步 provider 落地
    await tester.pumpAndSettle();
  }

  /// 在正文首段落末尾插入文本（模拟用户输入，绕开 IME 层）
  Future<void> insertText(WidgetTester tester, String text) async {
    final editorState = tester
        .state<WikiContentEditorState>(find.byType(WikiContentEditor))
        .editorState;
    final firstPara = editorState.document.root.children.first;
    final t = editorState.transaction;
    t.insertText(firstPara, firstPara.delta?.length ?? 0, text);
    await editorState.apply(t);
    await tester.pump();
  }

  group('wiki 详情页 · M3 编辑器集成', () {
    testWidgets('浏览态：正文经编辑器渲染，聊天块在页尾且仅一个', (tester) async {
      await pumpDetail(tester);

      expect(find.text('测试页面'), findsWidgets);
      expect(find.text('正文段落提到张三', findRichText: true), findsOneWidget);
      expect(find.byType(WikiContentEditor), findsOneWidget);
      expect(
        tester
            .widget<WikiContentEditor>(find.byType(WikiContentEditor))
            .editable,
        isFalse,
        reason: '浏览态应只读',
      );
      // 对话默认缩小，展开后使用浮层，正文不再被侧栏挤窄。
      expect(find.byType(WikiAiChatPanel), findsNothing);
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsOneWidget);
      expect(find.text('AI对话'), findsOneWidget);
      expect(find.byTooltip('缩小对话窗口'), findsOneWidget);
      await tester.tap(find.byTooltip('缩小对话窗口'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsNothing);
      // 编辑入口存在
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('素材页（kind=source）不显示编辑入口', (tester) async {
      await pumpDetail(tester, page: _sourcePage);

      expect(find.text('素材推文'), findsWidgets);
      expect(find.text('编辑正文'), findsNothing, reason: '素材采集页内容不可改，不提供人工编辑入口');
    });

    testWidgets('浮层回复渲染 Markdown，双击展开阅读', (tester) async {
      await pumpDetail(tester);
      repo.chatMessages = [
        Message(
          id: 'reply-1',
          conversationId: 'conv-page',
          role: MessageRole.assistant,
          content: '## 回答标题\n\n**重要内容**',
          createdAt: DateTime(2026, 1, 1),
        ),
      ];
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(MarkdownView), findsWidgets);
      await tester.ensureVisible(find.text('回答标题').first);
      await tester.tap(find.text('回答标题').first);
      await tester.pump(const Duration(milliseconds: 80));
      await tester.tap(find.text('回答标题').first);
      await tester.pumpAndSettle();
      expect(find.text('AI 回复'), findsOneWidget);
      expect(find.byType(Dialog), findsOneWidget);
      expect(find.byType(MarkdownView), findsAtLeastNWidgets(2));
    });

    testWidgets('编辑 → 修改 → 完成：saveWikiPageContent 收到含改动的 markdown，退出编辑态', (
      tester,
    ) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      expect(find.text('完成'), findsOneWidget, reason: '编辑态出现「完成」');
      expect(
        tester
            .widget<WikiContentEditor>(find.byType(WikiContentEditor))
            .editable,
        isTrue,
        reason: '编辑态应可写',
      );

      await insertText(tester, '人工补充内容');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
        reason: '改动后应标记脏',
      );

      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '应只保存一次');
      final (slug, md, reason) = repo.savedContent.single;
      expect(slug, _testPage.slug);
      expect(reason, 'GUI 编辑');
      expect(md, contains('人工补充内容'), reason: '保存的 markdown 应包含人工改动');
      expect(md, isNot(contains('wiki_chat')), reason: '对话块不进 markdown');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '保存后清除脏标记',
      );
      expect(find.text('完成'), findsNothing, reason: '保存成功退出编辑态');
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('无改动点「完成」：不写库直接退出', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 0, reason: '无改动不触发保存');
      expect(find.text('编辑正文'), findsOneWidget);
    });

    testWidgets('编辑 → 修改 → 取消：丢弃改动，不写库', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '将被丢弃');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
      );

      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 0, reason: '取消不保存');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '取消后清除脏标记',
      );
      expect(find.text('完成'), findsNothing, reason: '退出编辑态');
      // 文档回到加载快照
      final editorState = tester
          .state<WikiContentEditorState>(find.byType(WikiContentEditor))
          .editorState;
      expect(editorState.document.toJson(), isNot(contains('将被丢弃')));
    });

    testWidgets('Ctrl+S 保存并退出编辑态', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '保存快捷键');

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: 'Ctrl+S 应触发保存');
      expect(repo.savedContent.single.$2, contains('保存快捷键'));
      expect(find.text('编辑正文'), findsOneWidget, reason: '保存后退出编辑态');
    });

    testWidgets('保存失败：留在编辑态并展示错误条', (tester) async {
      repo = _FakeRepo(page: _testPage)..saveThrows = true;
      container = ProviderContainer(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
      );
      container.read(wikiOpenTabsProvider.notifier).state = [
        ImportTabEntry(),
        PageTabEntry(slug: _testPage.slug, title: _testPage.title),
      ];
      container.read(wikiActiveTabIdProvider.notifier).state =
          'page-${_testPage.slug}';
      addTearDown(container.dispose);
      tester.view.physicalSize = const Size(1600, 2200);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: MaterialApp(
            localizationsDelegates: const [
              DefaultMaterialLocalizations.delegate,
              DefaultWidgetsLocalizations.delegate,
              AppFlowyEditorLocalizations.delegate,
            ],
            home: const Scaffold(body: WikiPageDetailView()),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '保存会失败');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '尝试过一次保存');
      expect(find.textContaining('保存失败'), findsOneWidget, reason: '应展示错误条');
      expect(find.text('完成'), findsOneWidget, reason: '失败后留在编辑态');
    });

    testWidgets('wikilink 点击：跳转目标页 tab', (tester) async {
      await pumpDetail(tester);

      // 段落渲染为纯文本「正文段落提到张三」，其中「张三」是 wikilink span
      final richTextFinder = find.byWidgetPredicate(
        (w) => w is RichText && w.text.toPlainText() == '正文段落提到张三',
      );
      expect(richTextFinder, findsOneWidget);
      final renderParagraph = tester.renderObject<RenderParagraph>(
        richTextFinder,
      );
      final localBox = renderParagraph
          .getBoxesForSelection(
            const TextSelection(baseOffset: 6, extentOffset: 8),
          )
          .first;
      final center = renderParagraph.localToGlobal(
        Offset(
          (localBox.left + localBox.right) / 2,
          (localBox.top + localBox.bottom) / 2,
        ),
      );
      await tester.tapAt(center);
      await tester.pumpAndSettle();

      expect(repo.wikilinkLookups, contains('人物/张三'));
      expect(
        container.read(wikiActiveTabIdProvider),
        'page-人物/张三',
        reason: 'wikilink 应打开目标页',
      );
    });

    testWidgets('code 块：详情页渲染只读降级块，复制按钮写剪贴板（§11 Q4）', (tester) async {
      final clipboardLog = <MethodCall>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          clipboardLog.add(call);
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );

      await pumpDetail(tester, page: _codePage);

      // 经 WikiContentEditor（生产路径）渲染：不再是 30px placeholder
      expect(find.text('placeholder'), findsNothing);
      expect(find.text('final x = 1;', findRichText: true), findsOneWidget);
      expect(find.text('dart'), findsOneWidget, reason: '语言角标');
      expect(find.text('复制'), findsOneWidget);

      // 点复制：Clipboard.setData 收到代码全文
      await tester.tap(find.text('复制'));
      await tester.pumpAndSettle();
      final copyCall = clipboardLog
          .where((c) => c.method == 'Clipboard.setData')
          .lastOrNull;
      expect(copyCall, isNotNull, reason: '应调用 Clipboard.setData');
      expect(
        (copyCall!.arguments as Map)['text'],
        'final x = 1;',
        reason: '复制内容应去掉围栏与语言行',
      );
    });

    testWidgets('移动端走查（390×844）：窄视口不溢出，编辑器与卡座可用', (tester) async {
      // 单独起容器，视口改为手机尺寸（M4 移动端走查）
      repo = _FakeRepo(page: _testPage, wikilinkTarget: _wikilinkTarget);
      container = ProviderContainer(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
      );
      container.read(wikiOpenTabsProvider.notifier).state = [
        ImportTabEntry(),
        PageTabEntry(slug: _testPage.slug, title: _testPage.title),
      ];
      container.read(wikiActiveTabIdProvider.notifier).state =
          'page-${_testPage.slug}';
      addTearDown(container.dispose);

      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(tester.view.reset);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: MaterialApp(
            localizationsDelegates: const [
              DefaultMaterialLocalizations.delegate,
              DefaultWidgetsLocalizations.delegate,
              AppFlowyEditorLocalizations.delegate,
            ],
            home: const Scaffold(body: WikiPageDetailView()),
          ),
        ),
      );
      await tester.pump();
      await tester.pumpAndSettle();

      // 无溢出/布局异常（RenderFlex overflow 等会在此抛出）
      expect(tester.takeException(), isNull, reason: '窄视口不应有布局溢出');

      // 正文经编辑器渲染、编辑入口可见；窄屏（<1040）聊天不内联，
      // 入口是右下角 FAB（点开弹 bottom sheet）
      expect(find.byType(WikiContentEditor), findsOneWidget);
      expect(find.text('正文段落提到张三', findRichText: true), findsOneWidget);
      expect(find.text('编辑正文'), findsOneWidget);
      expect(find.byType(WikiAiChatPanel), findsNothing);
      expect(find.byIcon(Icons.auto_awesome_outlined), findsWidgets);
      await tester.tap(find.byTooltip('和 AI 讨论此页'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiAiChatPanel), findsOneWidget);
      expect(tester.takeException(), isNull, reason: '窄视口展开浮层不应溢出');
      await tester.tap(find.byTooltip('缩小对话窗口'));
      await tester.pumpAndSettle();

      // 底部卡座在「产出」页签下：窄屏切换页签同样有界（内部自滚动）
      await tester.tap(find.text('产出'));
      await tester.pumpAndSettle();
      expect(find.byType(WikiDerivatives), findsOneWidget);
      expect(tester.takeException(), isNull, reason: '产出页签窄屏不应溢出');

      // 回到内容页签，继续编辑
      await tester.tap(find.text('内容'));
      await tester.pumpAndSettle();

      // 编辑态在窄屏同样可用：进入 → 插入 → 保存
      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '窄屏也编辑');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();
      expect(repo.savedCount, 1, reason: '窄屏保存正常');
      expect(repo.savedContent.single.$2, contains('窄屏也编辑'));
    });

    testWidgets('关闭有未保存修改的 tab：先确认，放弃才关', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '未保存');

      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      expect(find.text('关闭前确认'), findsOneWidget, reason: '脏 tab 关闭应弹确认');

      // 取消：tab 保留
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();
      expect(container.read(wikiActiveTabIdProvider), 'page-topic/测试');

      // 再次关闭并确认放弃
      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      await tester.tap(find.text('放弃修改并关闭'));
      await tester.pumpAndSettle();

      expect(
        container.read(wikiActiveTabIdProvider),
        'import',
        reason: '确认放弃后关闭',
      );
      expect(repo.savedCount, 0, reason: '放弃不触发保存');
    });

    testWidgets('鼠标中键点击脏 tab：弹同款关闭确认', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '中键未保存');

      // 中键点 tab 标题（chip 内 12.5px，区别于页面大标题 26px）
      final chipTitle = find.byWidgetPredicate(
        (w) => w is Text && w.data == '测试页面' && w.style?.fontSize == 12.5,
      );
      expect(chipTitle, findsOneWidget);
      await tester.tap(chipTitle, buttons: kMiddleMouseButton);
      await tester.pumpAndSettle();

      expect(find.text('关闭前确认'), findsOneWidget, reason: '中键关闭脏 tab 应先确认');
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();
      expect(container.read(wikiActiveTabIdProvider), 'page-topic/测试');
    });

    testWidgets('关闭脏 tab 选「保存并关闭」：先保存再关', (tester) async {
      await pumpDetail(tester);

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '关前保存');

      await tester.tap(find.byIcon(Icons.close));
      await tester.pumpAndSettle();
      expect(find.text('关闭前确认'), findsOneWidget);

      await tester.tap(find.text('保存并关闭'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '关闭前应先保存');
      expect(repo.savedContent.single.$2, contains('关前保存'));
      expect(
        container.read(wikiActiveTabIdProvider),
        'import',
        reason: '保存成功后关闭',
      );
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
      );
    });

    testWidgets('编辑冲突 → 强制覆盖：跳过乐观锁再存', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '覆盖后台更新');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      expect(find.text('页面已被后台更新'), findsOneWidget, reason: '应弹冲突确认');
      expect(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
        findsOneWidget,
      );
      expect(find.text('重新加载'), findsOneWidget);

      await tester.tap(find.text('强制覆盖'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 2, reason: '冲突一次 + 覆盖一次');
      expect(repo.savedExpectedUpdatedAt[0], isNotNull, reason: '首次携带乐观锁');
      expect(repo.savedExpectedUpdatedAt[1], isNull, reason: '覆盖时跳过乐观锁');
      expect(repo.savedContent[1].$3, 'GUI 编辑（冲突后覆盖）');
      expect(find.text('编辑正文'), findsOneWidget, reason: '覆盖成功退出编辑态');
    });

    testWidgets('编辑冲突 → 取消：留在编辑态、无错误条、改动保留', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '先不存了');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text('取消'),
        ),
      );
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '只有被拒的那一次尝试');
      expect(find.text('完成'), findsOneWidget, reason: '取消后留在编辑态');
      expect(find.textContaining('保存失败'), findsNothing, reason: '取消不是失败');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isTrue,
        reason: '改动仍未保存，脏标记保留',
      );
    });

    testWidgets('编辑冲突 → 重新加载：放弃本地改动并退出编辑态', (tester) async {
      await pumpDetail(tester);
      repo.conflictOnLockedSave = true;

      await tester.tap(find.text('编辑正文'));
      await tester.pumpAndSettle();
      await insertText(tester, '将被重载丢弃');
      await tester.tap(find.text('完成'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('重新加载'));
      await tester.pumpAndSettle();

      expect(repo.savedCount, 1, reason: '重载不再写库');
      expect(find.text('编辑正文'), findsOneWidget, reason: '重载后回到浏览态');
      expect(
        container.read(wikiDirtyTabsProvider).contains(_testPage.slug),
        isFalse,
        reason: '重载视为放弃改动，清脏标记',
      );
    });
  });
}

/// 假仓库：内存应答 + 记录保存调用与 wikilink 查询
class _FakeRepo extends RustBridgeRepository {
  _FakeRepo({required this.page, this.wikilinkTarget});

  final WikiPage page;
  final WikiPage? wikilinkTarget;
  int savedCount = 0;
  bool saveThrows = false;
  List<Message> chatMessages = const [];

  /// 模拟「编辑期间页面被后台更新」：携带乐观锁的保存一律报冲突
  bool conflictOnLockedSave = false;
  final List<(String, String, String)> savedContent = [];
  final List<String?> savedExpectedUpdatedAt = [];
  final List<String> wikilinkLookups = [];

  @override
  Future<WikiPage?> getWikiPage(String slug) async {
    wikilinkLookups.add(slug);
    if (slug == wikilinkTarget?.slug) return wikilinkTarget;
    return page;
  }

  @override
  Future<List<WikiPage>> listWikiPages({String? kind, String? area}) async => [
    page,
  ];

  @override
  Future<List<WikiPage>> listWikiPageDerivatives(String slug) async => const [];

  @override
  Future<List<EntityFactDto>> listEntityFacts(
    String entityKind,
    String entitySlug,
  ) async => const [];

  @override
  Future<List<Relation>> listRelationsForPage(String slug) async => const [];

  @override
  Future<WikiPage> saveWikiPageContent({
    required String slug,
    required String contentMd,
    String reason = 'GUI 编辑',
    String? expectedUpdatedAt,
  }) async {
    savedCount++;
    savedContent.add((slug, contentMd, reason));
    savedExpectedUpdatedAt.add(expectedUpdatedAt);
    if (conflictOnLockedSave && expectedUpdatedAt != null) {
      throw Exception('编辑冲突：页面在你编辑期间已被更新（模拟）');
    }
    if (saveThrows) throw Exception('模拟保存失败');
    return page;
  }

  @override
  Future<Conversation> ensureWikiPageChat(String pageSlug) async {
    return Conversation(
      id: 'conv-page',
      createdAt: DateTime(2026, 1, 1),
      updatedAt: DateTime(2026, 1, 1),
      messageCount: 0,
      wikiPageSlug: pageSlug,
    );
  }

  @override
  Future<List<Message>> listMessages(String conversationId) async =>
      chatMessages;

  @override
  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) async {
    return Message(
      id: 'm-reply',
      conversationId: conversationId,
      role: MessageRole.fromString('assistant'),
      content: '模拟回复',
      createdAt: DateTime(2026, 1, 1),
    );
  }
}
