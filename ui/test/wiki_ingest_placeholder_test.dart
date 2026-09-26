import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

/// 知识库首页（合并工作区）：上方两张导入入口卡片，点击弹对话框。
/// 只测本地校验路径（空输入提示），不发起真实网络/FFI 调用。
void main() {
  testWidgets('首页展示网址/直接文本入口，两对话框各有空输入校验', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: const MaterialApp(home: Scaffold(body: WikiPageDetailView())),
      ),
    );
    await tester.pump();
    await tester.pump();

    // 首页（不再有独立「导入」tab）：两张入口卡片
    expect(find.text('网址导入'), findsOneWidget, reason: '首页应有网址导入入口');
    expect(find.text('直接文本'), findsOneWidget, reason: '首页应有直接文本入口');

    // 网址模式对话框：输入框 + 抓取按钮；空输入 → 本地校验错误
    await tester.tap(find.text('网址导入'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200)); // 对话框动画落地
    expect(
      find.descendant(
        of: find.byType(Dialog),
        matching: find.byType(TextField),
      ),
      findsOneWidget,
      reason: '网址对话框应有输入框',
    );
    expect(find.text('抓取'), findsOneWidget);
    await tester.tap(find.text('抓取'));
    await tester.pump();
    expect(find.text('请先粘贴一个链接'), findsOneWidget);

    // 点 barrier 关闭对话框，进入「直接文本」
    await tester.tapAt(const Offset(5, 5));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200)); // 对话框退场动画
    await tester.tap(find.text('直接文本'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.text('保存到知识库'), findsOneWidget, reason: '文本对话框应有保存按钮');
    await tester.tap(find.text('保存到知识库'));
    await tester.pump();
    expect(find.text('请先输入要保存的内容'), findsOneWidget);
  });
}