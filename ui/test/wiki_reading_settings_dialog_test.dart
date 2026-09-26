import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/wiki_reading_settings_dialog.dart';

import 'support/isolated_bridge.dart';

/// 知识页「AA」浮层（编辑器内容区阅读参数覆盖层）行为验证：
/// - 三项控件渲染、「跟随全局」默认全部选中（未设置 = 继承）；
/// - 拖动字号/行距滑块 → 进入覆盖态（编辑器覆盖层写入）；
/// - 点「跟随全局」→ 清空覆盖回归继承态。
/// 同一 isolate 内 RustLib.init 只能一次，故共享一个 repo。
void main() {
  late RustBridgeRepository repo;
  late ProviderContainer container;

  setUpAll(() async {
    repo = await createIsolatedBridge();
  });

  setUp(() {
    container = ProviderContainer(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    );
    addTearDown(container.dispose);
  });

  Future<void> pumpDialog(WidgetTester tester) async {
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: MaterialApp(
          theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
          home: const Scaffold(body: WikiReadingSettingsDialog()),
        ),
      ),
    );
    await tester.pump();
  }

  // 当前状态里的编辑器覆盖层取值
  double? editorFontSize() => container.read(settingsProvider).editorFontSize;
  double? editorLineHeight() =>
      container.read(settingsProvider).editorLineHeight;

  // key 挂在包装组件 _FollowGlobalChip 上；取其内部 ActionChip 读选中态
  ActionChip chipByKey(WidgetTester tester, Key key) =>
      tester.widget<ActionChip>(
        find.descendant(of: find.byKey(key), matching: find.byType(ActionChip)),
      );

  testWidgets('渲染：三项控件 + 默认全部「跟随全局」', (tester) async {
    await pumpDialog(tester);

    expect(find.text('正文阅读设置'), findsOneWidget);
    expect(find.text('字体'), findsOneWidget);
    expect(find.text('字号'), findsOneWidget);
    expect(find.text('行距'), findsOneWidget);
    expect(find.byType(Slider), findsNWidgets(2));
    expect(find.byType(ActionChip), findsNWidgets(3));
    // 未设置：字号/行距都没有覆盖
    expect(editorFontSize(), isNull);
    expect(editorLineHeight(), isNull);
  });

  testWidgets('拖动字号滑块 → 进入覆盖态，区域跟随全局 chip 取消选中', (tester) async {
    await pumpDialog(tester);

    // 字号滑块居中点击 → 值落在中部（12–24 间某处），形成覆盖
    await tester.tap(find.byType(Slider).at(0));
    await tester.pumpAndSettle(const Duration(milliseconds: 50));

    final override = editorFontSize();
    expect(override, isNotNull, reason: '拖动后应写入编辑器字号覆盖');
    expect(override, inInclusiveRange(12.0, 24.0));
    expect(editorLineHeight(), isNull, reason: '字号拖动不影响行距覆盖');

    // 覆盖态下该行的「跟随全局」chip 应可点（取消选中 / 变为可重置）
    final reset = chipByKey(tester, const Key('reading-size-reset'));
    expect(reset.onPressed, isNotNull);
  });

  testWidgets('点「跟随全局」→ 清空字号覆盖、回落全局', (tester) async {
    await pumpDialog(tester);

    // 先创建覆盖
    await tester.tap(find.byType(Slider).at(0));
    await tester.pumpAndSettle(const Duration(milliseconds: 50));
    expect(editorFontSize(), isNotNull);

    // 重置
    await tester.tap(find.byKey(const Key('reading-size-reset')));
    await tester.pump();

    expect(editorFontSize(), isNull, reason: '跟随全局 = 清空覆盖层');
    final resetChip = chipByKey(tester, const Key('reading-size-reset'));
    expect(resetChip.onPressed, isNull, reason: '已跟随全局时重置按钮禁用');
  });

  testWidgets('行距滑块同理：写入覆盖 → 重置回归', (tester) async {
    await pumpDialog(tester);

    await tester.tap(find.byType(Slider).at(1));
    await tester.pumpAndSettle(const Duration(milliseconds: 50));
    final override = editorLineHeight();
    expect(override, isNotNull, reason: '行距滑块应写入覆盖');
    expect(override, inInclusiveRange(1.0, 2.5));
    expect(editorFontSize(), isNull, reason: '行距拖动不影响字号覆盖');

    await tester.tap(find.byKey(const Key('reading-height-reset')));
    await tester.pump();
    expect(editorLineHeight(), isNull);
  });

  testWidgets('字体项点按打开 FontPicker 选择对话框', (tester) async {
    await pumpDialog(tester);
    expect(find.text('跟随全局'), findsWidgets);

    await tester.tap(find.byIcon(Icons.font_download_outlined));
    await tester.pumpAndSettle(const Duration(milliseconds: 100));

    expect(find.text('选择正文阅读字体'), findsOneWidget);
  });
}
