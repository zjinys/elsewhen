import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

/// 知识库右侧 tab 面板：固定「推文导入」tab 的抓取输入 UI 验证。
/// 只测本地校验路径（空输入提示），不发起真实网络/FFI 调用。
void main() {
  testWidgets('默认 tab 展示推文导入 UI 并校验空输入', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: const MaterialApp(
          home: Scaffold(body: WikiPageDetailView()),
        ),
      ),
    );
    await tester.pump();

    // 缺省只有「推文导入」tab：输入框 + 抓取按钮 + 提示
    expect(find.text('推文导入'), findsOneWidget, reason: '固定 tab 应是推文导入');
    expect(find.text('导入推文内容'), findsOneWidget);
    expect(find.text('抓取'), findsOneWidget);
    expect(find.byType(TextField), findsOneWidget);

    // 空输入点「抓取」→ 本地校验错误，不发请求
    await tester.tap(find.text('抓取'));
    await tester.pump();
    expect(find.text('请先粘贴一个 x.com / twitter.com 推文链接'), findsOneWidget);
  });
}