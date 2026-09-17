import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';

/// 设置页预填验证：进入设置页应从 Rust DB 加载真实 AI provider（.env 导入的那份），
/// 而不是硬编码默认值。需要一份带 provider 配置的库副本。
/// 运行：ELSEWHEN_DATA_DIR=/tmp/opencode/frb-settings-widget flutter test test/settings_screen_test.dart
void main() {
  testWidgets('settings screen prefills real provider from bridge', (tester) async {
    final repo = RustBridgeRepository();
    await tester.runAsync(() => repo.initialize());

    // 真实桥接读取当前生效的 provider（demo 副本里有 hub.oaifree.com / gpt-4o）
    final expected = await tester.runAsync(() => repo.getAiProviderConfig());
    expect(expected, isNotNull, reason: '测试库副本应含 provider 配置');
    final expectedUrl = expected!.baseUrl;
    final expectedModel = expected.model;

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          storageRepositoryProvider.overrideWithValue(repo),
        ],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );

    // 等 post-frame 的 _loadProviders 跑完真实 FFI
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 400)),
    );
    await tester.pump();

    // 模型 tab（默认）：AI provider 是卡片列表（多配置 + 单激活），
    // 活跃卡片应展示库中的真实配置，而非硬编码默认值（openai.com / gpt-3.5-turbo）。
    expect(find.text('添加配置'), findsOneWidget, reason: '多 provider 入口按钮');
    expect(find.text('激活中'), findsOneWidget, reason: '库中配置应处于激活状态');
    // 卡片上展示真实 baseUrl / model
    expect(
      find.textContaining(expectedUrl),
      findsWidgets,
      reason: 'baseUrl 应预填真实配置而非默认值',
    );
    expect(
      find.textContaining(expectedModel),
      findsWidgets,
      reason: 'model 应预填真实配置而非默认值',
    );
    expect(find.textContaining('api.openai.com/v1'), findsNothing,
        reason: '不应出现硬编码默认 baseUrl');
    expect(find.textContaining('gpt-3.5-turbo'), findsNothing,
        reason: '不应出现硬编码默认 model');

    // 记忆策略默认「简单记忆」：模型 tab 仅有 1 个文本输入（最大消息数），
    // AI provider 字段移入「编辑」对话框，不直接铺在页面上。
    expect(tester.widgetList<TextField>(find.byType(TextField)).length, 1,
        reason: '模型 tab：provider 为卡片布局，仅记忆策略有文本输入');

    // 切到「数据」tab 应看到每日 Token 使用区块（纯展示，不依赖是否有记录）
    await tester.tap(find.text('数据'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('每日 Token 使用'), findsOneWidget);
  });
}