import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/left_sidebar.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

/// 左侧导航是标准的 Material 3 导航抽屉（NavigationDrawer）。
///
/// 这里只测抽屉组件自身：标准入口、选中态、面板高度的计算、模态下选完即收。
/// 抽屉在主界面里是模态还是停靠，见 widget_test.dart。
///
/// 用「回顾」入口做夹具：它的上下文面板（近 7 天）不依赖 Rust 桥。
/// 测试宿主是桌面平台，入口行高是桌面的 48。
void main() {
  void useView(WidgetTester tester, Size size) {
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
  }

  /// 停靠形态：抽屉放在一个定宽的格子里。
  Widget docked({double height = 800}) {
    return ProviderScope(
      overrides: [
        sidebarTabProvider.overrideWith(() => StateHolder(SidebarTab.review)),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: Align(
            alignment: Alignment.topLeft,
            child: SizedBox(
              width: 280,
              height: height,
              child: const LeftSidebar(),
            ),
          ),
        ),
      ),
    );
  }

  /// 模态形态：抽屉放进 Scaffold.drawer；主区显示当前入口名。
  Widget modal() {
    return ProviderScope(
      overrides: [
        sidebarTabProvider.overrideWith(() => StateHolder(SidebarTab.review)),
      ],
      child: MaterialApp(
        home: Scaffold(
          drawer: const LeftSidebar(),
          body: Consumer(
            builder: (context, ref, _) =>
                Text('tab:${ref.watch(sidebarTabProvider).name}'),
          ),
        ),
      ),
    );
  }

  /// 抽屉自己的滚动位置（面板内部还有各自的列表，取最外层那个）。
  ScrollPosition drawerScroll(WidgetTester tester) => tester
      .state<ScrollableState>(
        find
            .descendant(
              of: find.byType(NavigationDrawer),
              matching: find.byType(Scrollable),
            )
            .first,
      )
      .position;

  testWidgets('一级入口是标准的 NavigationDrawerDestination，选中态跟随当前入口', (tester) async {
    useView(tester, const Size(800, 800));
    await tester.pumpWidget(docked());
    await tester.pump();

    final drawer = tester.widget<NavigationDrawer>(
      find.byType(NavigationDrawer),
    );
    expect(find.byType(NavigationDrawerDestination), findsNWidgets(6));
    for (final label in ['对话', '知识', '目标', '回顾', '同步', '情报']) {
      expect(
        find.descendant(
          of: find.byType(NavigationDrawerDestination),
          matching: find.text(label),
        ),
        findsOneWidget,
      );
    }
    // 顺序：对话 / 知识 / 目标 / 情报 / 回顾 / 同步。
    expect(drawer.selectedIndex, 4);
    // 设置不是导航目的地：固定在底部，不参与选中态。
    expect(find.text('设置'), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(NavigationDrawerDestination),
        matching: find.text('设置'),
      ),
      findsNothing,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('点入口切换当前入口，回顾不再重复侧栏面板', (tester) async {
    useView(tester, const Size(800, 800));
    await tester.pumpWidget(docked());
    await tester.pump();
    expect(find.text('近 7 天'), findsNothing);

    await tester.tap(find.text('同步'));
    await tester.pumpAndSettle();

    expect(
      tester
          .widget<NavigationDrawer>(find.byType(NavigationDrawer))
          .selectedIndex,
      5,
    );
    expect(find.text('近 7 天'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('窗口够高：上下文面板正好填满入口与设置之间，抽屉不滚动', (tester) async {
    useView(tester, const Size(800, 800));
    await tester.pumpWidget(docked());
    await tester.pump();

    expect(drawerScroll(tester).maxScrollExtent, moreOrLessEquals(0));
    // 入口按桌面密度排布：行距 48。
    expect(
      tester.getCenter(find.text('知识')).dy -
          tester.getCenter(find.text('对话')).dy,
      moreOrLessEquals(48),
    );
    // 设置贴着抽屉底边，占一个入口行高。
    final settings = tester.getRect(find.text('设置'));
    expect(settings.top, greaterThan(800 - 49));
    expect(settings.bottom, lessThan(800));
    // 面板一直铺到设置上方：最下面一个日期还在面板里。
    expect(find.text('近 7 天'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('窗口很矮：抽屉整体滚动，不溢出，设置仍固定在底部', (tester) async {
    useView(tester, const Size(800, 360));
    await tester.pumpWidget(docked(height: 360));
    await tester.pump();

    // 六个入口加设置就接近 360，面板拿保底高度，其余靠滚动。
    expect(tester.takeException(), isNull);
    expect(drawerScroll(tester).maxScrollExtent, greaterThan(0));
    final settings = tester.getRect(find.text('设置'));
    expect(settings.top, greaterThan(360 - 49));
    expect(settings.bottom, lessThan(360));

    // 滚到底能看到面板内容。
    await tester.drag(find.text('对话'), const Offset(0, -300));
    await tester.pumpAndSettle();
    expect(find.text('近 7 天'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('模态抽屉：点入口后切换并自动收起', (tester) async {
    useView(tester, const Size(390, 844));
    await tester.pumpWidget(modal());
    await tester.pump();
    expect(find.byType(NavigationDrawer), findsNothing);

    tester.firstState<ScaffoldState>(find.byType(Scaffold)).openDrawer();
    await tester.pumpAndSettle();
    expect(find.byType(NavigationDrawer), findsOneWidget);

    await tester.tap(find.text('对话'));
    await tester.pumpAndSettle();

    expect(find.text('tab:conversation'), findsOneWidget);
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('模态抽屉：选择同步自动收起', (tester) async {
    useView(tester, const Size(390, 844));
    await tester.pumpWidget(modal());
    await tester.pump();

    tester.firstState<ScaffoldState>(find.byType(Scaffold)).openDrawer();
    await tester.pumpAndSettle();
    await tester.tap(find.text('同步'));
    await tester.pumpAndSettle();

    expect(find.text('tab:sync'), findsOneWidget);
    expect(find.byType(NavigationDrawer), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('模态抽屉避开系统安全区：入口在状态栏下方，设置在手势条上方，高度仍然算准', (tester) async {
    useView(tester, const Size(390, 844));
    tester.view.padding = const FakeViewPadding(top: 24, bottom: 34);
    tester.view.viewPadding = const FakeViewPadding(top: 24, bottom: 34);
    await tester.pumpWidget(modal());
    await tester.pump();

    tester.firstState<ScaffoldState>(find.byType(Scaffold)).openDrawer();
    await tester.pumpAndSettle();

    expect(tester.getRect(find.text('对话')).top, greaterThan(24 + 12));
    final settings = tester.getRect(find.text('设置'));
    expect(settings.bottom, lessThan(844 - 34));
    expect(settings.top, greaterThan(844 - 34 - 49));
    // 安全区只垫一次：面板仍然正好填满，抽屉不需要滚动。
    expect(drawerScroll(tester).maxScrollExtent, moreOrLessEquals(0));
    expect(tester.takeException(), isNull);
  });

  testWidgets('停靠形态：点入口不会去关任何东西', (tester) async {
    useView(tester, const Size(800, 800));
    await tester.pumpWidget(docked());
    await tester.pump();

    await tester.tap(find.text('同步'));
    await tester.pumpAndSettle();

    expect(find.byType(NavigationDrawer), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
