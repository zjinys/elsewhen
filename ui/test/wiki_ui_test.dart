import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/main_screen.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// Headless UI verification of the new left sidebar (对话 / 知识库 tabs + 设置入口)
/// and the wiki browsing flow, driven by the REAL Rust bridge against the demo DB.
/// Run with: ELSEWHEN_DATA_DIR=/tmp/opencode/frb-wiki-test flutter test test/wiki_ui_test.dart
void main() {
  testWidgets('left sidebar tabs + wiki browsing with real bridge', (tester) async {
    final repo = RustBridgeRepository();
    await tester.runAsync(() => repo.initialize());

    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
        child: const MaterialApp(home: MainScreen()),
      ),
    );
    await tester.pump();

    // 1. 左侧栏结构：双 Tab + 底部设置入口
    expect(find.text('对话'), findsOneWidget, reason: '对话 tab 应在左侧栏');
    expect(find.text('知识库'), findsOneWidget, reason: '知识库 tab 应在左侧栏');
    expect(find.text('设置'), findsOneWidget, reason: '设置入口应在左侧栏底部');

    // 2. 初始为对话 Tab：右侧是 MessageArea
    expect(find.byType(MessageArea), findsOneWidget, reason: '对话 Tab 右侧应为消息区');
    expect(find.text('知识库'), findsOneWidget);

    // 3. 切到知识库 Tab，等待桥接数据加载
    await tester.tap(find.text('知识库'));
    await tester.pump();
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 500)));
    await tester.pump();

    // 分组列表应显示 demo 库的 wiki 页面（列表懒加载，靠下分组先滚动到可见）
    expect(find.text('闲置的旧笔记本电脑'), findsOneWidget, reason: 'demo 库 asset 页应列出');
    expect(find.text('Rust 和 Flutter 语言能力'), findsOneWidget, reason: 'demo 库 capability 页应列出');

    // 手动滚动 sidebar ListView 至底部，验证靠下的分组
    Future<void> reveal(String text) async {
      final sidebarList = find.byWidgetPredicate(
        (w) => w is ListView && w.scrollDirection == Axis.vertical,
      ).first;
      for (var i = 0; i < 6; i++) {
        if (find.text(text).evaluate().isNotEmpty) return;
        await tester.drag(sidebarList, const Offset(0, -250));
        await tester.pump();
      }
      expect(find.text(text), findsOneWidget, reason: '滚动后应能看到 $text');
    }

    await reveal('东莞往返惠州的交通费用');
    expect(find.text('东莞往返惠州的交通费用'), findsOneWidget, reason: 'demo 库 recurring_cost 页应列出');

    await reveal('顺风车分摊通勤成本');
    // 该 insight 页标题与摘要相同，列表项渲染两处 → 用 findsWidgets
    expect(find.text('顺风车分摊通勤成本').evaluate().length, greaterThanOrEqualTo(2),
        reason: 'demo 库 insight 页应列出');

    // 4. 点击一页查看详情（当前可见项）
    await tester.tap(find.text('东莞往返惠州的交通费用'));
    await tester.pump();
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 300)));
    await tester.pump();

    // 详情视图：标题 + 证据徽章 + 正文 + 溯源说明
    expect(find.text('闲置的旧笔记本电脑'), findsWidgets, reason: '详情头应包含标题');
    expect(find.textContaining('证据'), findsWidgets, reason: '应显示证据徽章');
  });
}