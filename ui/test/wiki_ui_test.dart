import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/main_screen.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

import 'support/isolated_bridge.dart';

/// Headless UI verification of the left sidebar（对话 / 知识库 / 待办 tabs + 设置入口）
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

    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(bridge)],
        child: const MaterialApp(home: MainScreen()),
      ),
    );
    await tester.pump();

    // 1. 左侧栏结构：三个 Tab + 底部设置入口
    expect(find.text('对话'), findsOneWidget, reason: '对话 tab 应在左侧栏');
    expect(find.text('知识库'), findsOneWidget, reason: '知识库 tab 应在左侧栏');
    expect(find.text('待办'), findsOneWidget, reason: '待办 tab 应在左侧栏');
    expect(find.text('设置'), findsOneWidget, reason: '设置入口应在左侧栏底部');

    // 2. 初始为对话 Tab：右侧是 MessageArea
    expect(find.byType(MessageArea), findsOneWidget, reason: '对话 Tab 右侧应为消息区');

    // 3. 切到知识库 Tab，等待桥接数据加载
    await tester.tap(find.text('知识库'));
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 500)),
    );
    await tester.pump();

    // 新 UI：搜索框 + kind 过滤 chips + 分组列表
    expect(find.text('搜索知识库…'), findsOneWidget, reason: '知识库 tab 应有搜索框');
    expect(find.textContaining('全部'), findsWidgets, reason: '应有「全部」过滤 chip');

    // 4. 用真实桥接读取页面列表，取列表里真实存在的一页做浏览验证
    final pages = await tester.runAsync(() => bridge.listWikiPages());
    expect(pages, isNotNull);
    final all = pages ?? [];
    expect(all, isNotEmpty, reason: '知识库应至少有一页');

    final page = all.first;
    final title = page.title;

    // 手动滚动 sidebar ListView（可能被搜索框/chips 顶到屏外）直到目标可见
    final sidebarList = find
        .byWidgetPredicate(
          (w) => w is ListView && w.scrollDirection == Axis.vertical,
        )
        .first;
    for (var i = 0; i < 10; i++) {
      if (find.text(title).evaluate().isNotEmpty) break;
      await tester.drag(sidebarList, const Offset(0, -250));
      await tester.pump();
    }
    expect(find.text(title), findsWidgets, reason: '滚动后应能看到列表页 $title');

    // 4b. 列表项应展示 tags/证据等元数据
    expect(find.textContaining('证据'), findsWidgets, reason: '列表项应显示证据徽章');

    // 5. 点击该页查看详情
    await tester.tap(find.text(title).first);
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump();

    // 详情视图：标题（在头部重复出现）+ 证据 + AI 处理面板标题
    expect(find.text(title), findsWidgets, reason: '详情头应包含标题');
    expect(find.textContaining('证据'), findsWidgets, reason: '应显示证据徽章');
    expect(find.text('AI 处理本页'), findsWidgets, reason: '页面应有 AI 处理面板');
  });
}
