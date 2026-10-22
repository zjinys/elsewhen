import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/left_sidebar.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 回归：侧栏底色放在 Material 而不是外层 DecoratedBox。
///
/// ListTile 的 selectedTileColor / ink 画在最近的 Material 上；若中间的
/// DecoratedBox 先铺了底色，ListTile 会触发
/// "ListTile background color or ink splashes may be invisible"。
void main() {
  testWidgets('回顾侧栏 ListTile 不触发背景不可见断言', (tester) async {
    // LeftSidebar 用 MediaQuery 宽度决定宽侧栏 / 64px 图标轨；
    // 500 宽才会渲染带列表的宽侧栏。
    tester.view.physicalSize = const Size(500, 600);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          sidebarTabProvider.overrideWith(
            () => StateHolder(SidebarTab.review),
          ),
        ],
        child: const MaterialApp(
          home: Scaffold(
            body: SizedBox(width: 280, height: 600, child: LeftSidebar()),
          ),
        ),
      ),
    );
    await tester.pump();

    expect(tester.takeException(), isNull);
    expect(find.text('近 7 天'), findsOneWidget);
  });
}
