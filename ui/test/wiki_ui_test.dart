import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/main_screen.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

import 'support/isolated_bridge.dart';

/// Headless UI verification of the left sidebar（对话 / 知识库 tabs；设置入口在标题栏）
/// and the wiki browsing flow, driven by the REAL Rust bridge and an isolated DB.
void main() {
  testWidgets('left sidebar tabs + wiki browsing with real bridge', (
    tester,
  ) async {
    // 接近真实主窗口（1920×1080）的测试画布，避免小屏导致 ListView 懒渲染
    // 把详情正文挤出视口
    tester.view.physicalSize = const Size(1600, 1000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    final repo = await tester.runAsync(createIsolatedBridge);
    final bridge = repo!;
    await tester.runAsync(
      () => bridge.saveTextPage(
        text: '用于验证知识库浏览 UI 的测试内容。',
        title: '隔离知识页',
        tags: const ['test'],
      ),
    );

    // 主对话流是「最近使用」区块的前提：左侧栏 gate 在 mainConversationProvider
    // 的 data 分支，主对话缺失（本测试不跑 appInit）会落到 error 分支。先用真实
    // 桥接建一个「主对话流」会话。走真实异步的桥接写/读必须放在 runAsync 里，
    // 在 fake-async 沙盒中直接 await 会死锁。
    await tester.runAsync(
      () => bridge.createConversation(title: '主对话流', tag: 'diary'),
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(bridge)],
        child: const MaterialApp(home: MainScreen()),
      ),
    );
    await tester.pump();

    // 真实桥接的异步 provider（mainConversation / wikiPages 等）在 fake-async 的
    // 普通 pump 下不会 resolve——底层走 frb worker 线程的真实 Future。多轮
    // 「runAsync 真实延时 + pump」推进：runAsync 跳出 fake-async 让真实 Future
    // 完成，pump 把解析结果刷进 widget 树。
    for (var i = 0; i < 6; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 200)),
      );
      await tester.pump();
    }

    // 1. 左侧栏结构：主对话 + 最近页面 + 底部工作区入口
    expect(find.text('对话'), findsOneWidget, reason: '对话 tab 应在左侧栏');
    expect(find.text('知识库'), findsOneWidget, reason: '知识库应在左侧统一入口');
    expect(find.text('知识库'), findsOneWidget, reason: '知识库应在左侧快捷入口');
    // 设置入口已移到标题栏：图标按钮（tooltip「设置」），不再出现在侧边栏
    expect(find.byTooltip('设置'), findsOneWidget, reason: '设置入口应在标题栏右侧');

    // 2. 初始为对话 Tab：右侧是 MessageArea
    expect(find.byType(MessageArea), findsOneWidget, reason: '对话 Tab 右侧应为消息区');

    // 3. 通过左侧快捷入口打开入库工作区
    await tester.tap(find.text('知识库'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 500)),
    );
    await tester.pump();

    expect(find.text('首页'), findsOneWidget, reason: '知识库入口应打开首页工作区');

    // 返回主对话后，从“最近使用”打开知识页详情
    await tester.tap(find.text('对话'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump();

    // 4. 用真实桥接读取页面，取最近使用列表里的页面做浏览验证
    final pages = await tester.runAsync(() => bridge.listWikiPages());
    expect(pages, isNotNull);
    final all = pages ?? [];
    expect(all, isNotEmpty, reason: '知识库应至少有一页');

    final page = all.first;
    final title = page.title;

    expect(find.text(title), findsOneWidget, reason: '最近使用列表应显示页面 $title');

    // 5. 点击最近页面查看详情
    await tester.tap(find.text(title));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump();

    // 详情视图：标题（在头部重复出现）+ 证据 + AI 处理面板（由右下角 FAB 展开）
    expect(find.text(title), findsWidgets, reason: '详情头应包含标题');
    expect(find.textContaining('证据'), findsWidgets, reason: '应显示证据徽章');
    await tester.tap(find.byTooltip('和 AI 讨论此页'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump();
    expect(find.text('AI对话'), findsWidgets, reason: '页面应有 AI 处理面板');
  });
}
