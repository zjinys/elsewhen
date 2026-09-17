import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';

/// 知识库右侧 tab 面板：固定「导入」tab 的抓取输入 UI 验证。
/// 只测本地校验路径（空输入提示），不发起真实网络/FFI 调用。
void main() {
  testWidgets('默认 tab 展示导入 UI（网址/文本双模式）并校验空输入', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        child: const MaterialApp(
          home: Scaffold(body: WikiPageDetailView()),
        ),
      ),
    );
    await tester.pump();

    // 缺省「导入」tab：模式切换 + 网址模式输入框 + 抓取按钮 + 提示
    expect(find.text('导入'), findsOneWidget, reason: '固定 tab 应是导入');
    expect(find.text('🌐 网址导入'), findsOneWidget, reason: '应有网址导入模式');
    expect(find.text('📝 直接文本'), findsOneWidget, reason: '应有直接文本模式');
    expect(find.text('抓取'), findsOneWidget);
    expect(find.byType(TextField), findsOneWidget);

    // 空输入点「抓取」→ 本地校验错误，不发请求
    await tester.tap(find.text('抓取'));
    await tester.pump();
    expect(find.text('请先粘贴一个链接'), findsOneWidget);

    // 切到「直接文本」模式：文本框 + 保存按钮；空输入校验
    await tester.tap(find.text('📝 直接文本'));
    await tester.pump();
    expect(find.text('保存到知识库'), findsOneWidget, reason: '文本模式应有保存按钮');
    expect(find.text('抓取'), findsNothing);
    await tester.tap(find.text('保存到知识库'));
    await tester.pump();
    expect(find.text('请先输入要保存的内容'), findsOneWidget);
  });
}