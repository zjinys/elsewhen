import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';

import 'package:elsewhen_ui/main.dart';
import 'package:elsewhen_ui/providers/app_provider.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';

void main() {
  testWidgets('App launches successfully', (WidgetTester tester) async {
    // Override initialization to skip platform-specific services
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 桌面宽度（≥1024）：导航抽屉停靠在内容左侧，入口带「对话 / 知识」文字。
    // 默认 800×600 测试窗口落在「桌面小窗」区间，抽屉是模态的、平时收起，
    // 没有文字可查——这不是回归，是响应式设计的预期形态。
    tester.view.physicalSize = const Size(1440, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const ElsewhenApp(),
      ),
    );

    // Wait for async operations to complete
    await tester.pumpAndSettle();

    expect(find.text('对话'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
    expect(find.text('编辑器测试'), findsNothing);
    expect(find.byType(SvgPicture), findsOneWidget);
    expect(find.byType(NavigationDrawer), findsOneWidget);
  });

  testWidgets('Wide window: menu button collapses and restores the docked drawer', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    tester.view.physicalSize = const Size(1440, 900);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const ElsewhenApp(),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byType(NavigationDrawer), findsOneWidget);

    // 停靠的抽屉不是模态的：没有遮罩，菜单按钮直接收起，内容区让出宽度。
    await tester.tap(find.byIcon(Icons.menu_outlined));
    await tester.pumpAndSettle();
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(find.text('对话'), findsNothing);
    expect(
      tester.firstState<ScaffoldState>(find.byType(Scaffold)).isDrawerOpen,
      isFalse,
    );

    await tester.tap(find.byIcon(Icons.menu_outlined));
    await tester.pumpAndSettle();
    expect(find.byType(NavigationDrawer), findsOneWidget);
    expect(find.text('对话'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Small desktop window uses a modal drawer behind the menu button', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 桌面小窗（700–1024）：主区本来就挤，导航不常占一栏，收进模态抽屉。
    tester.view.physicalSize = const Size(900, 700);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const ElsewhenApp(),
      ),
    );
    await tester.pumpAndSettle();

    // 平时收起：没有入口文字，也没有旧的图标轨。
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(find.text('对话'), findsNothing);
    expect(find.byIcon(Icons.chat_bubble_outline), findsNothing);

    // 菜单按钮打开抽屉。
    await tester.tap(find.byIcon(Icons.menu_outlined));
    await tester.pumpAndSettle();
    expect(find.byType(NavigationDrawer), findsOneWidget);
    expect(find.text('对话'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
    expect(
      tester.firstState<ScaffoldState>(find.byType(Scaffold)).isDrawerOpen,
      isTrue,
    );

    // 选一个入口：切换并自动收起。
    await tester.tap(find.text('对话'));
    await tester.pumpAndSettle();
    expect(container.read(sidebarTabProvider), SidebarTab.conversation);
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Widening the window with the modal drawer open docks it cleanly', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    tester.view.physicalSize = const Size(900, 700);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const ElsewhenApp(),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(Icons.menu_outlined));
    await tester.pumpAndSettle();
    ScaffoldState scaffold() =>
        tester.firstState<ScaffoldState>(find.byType(Scaffold));
    expect(scaffold().isDrawerOpen, isTrue);

    // 抽屉开着时把窗口拖宽：导航改为停靠，模态抽屉连同遮罩一起收掉，
    // 屏幕上只剩停靠的那一个。
    tester.view.physicalSize = const Size(1440, 900);
    await tester.pumpAndSettle();
    expect(scaffold().isDrawerOpen, isFalse);
    expect(find.byType(NavigationDrawer), findsOneWidget);
    expect(find.text('对话'), findsOneWidget);

    // 再缩回小窗：抽屉保持收起，不会自己弹出来。
    tester.view.physicalSize = const Size(900, 700);
    await tester.pumpAndSettle();
    expect(scaffold().isDrawerOpen, isFalse);
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Phone width uses a modal drawer', (
    WidgetTester tester,
  ) async {
    final container = ProviderContainer(
      overrides: [appInitializationProvider.overrideWith((ref) async => true)],
    );

    // 手机宽度（<600）：导航收进模态抽屉。
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const ElsewhenApp(),
      ),
    );
    await tester.pumpAndSettle();

    // 抽屉关闭时导航不可见；打开抽屉后出现文字导航。
    expect(find.text('对话'), findsNothing);
    final scaffoldState = tester.firstState<ScaffoldState>(
      find.byType(Scaffold),
    );
    scaffoldState.openDrawer();
    await tester.pumpAndSettle();
    expect(find.text('对话'), findsOneWidget);
    expect(find.text('知识'), findsOneWidget);
  });

}
