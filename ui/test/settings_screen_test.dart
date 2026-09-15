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

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          storageRepositoryProvider.overrideWithValue(repo),
        ],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );

    // 等 post-frame 的 _loadFromBridge 跑完真实 FFI
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 400)),
    );
    await tester.pump();

    // 模型 tab（默认）：表单前两个字段应被真实配置覆写，而非硬编码默认值（openai.com / gpt-3.5-turbo）
    final fields = tester.widgetList<TextField>(find.byType(TextField)).toList();
    expect(fields.length, 6, reason: '模型 tab：AI provider 5 项 + memory 1 项');
    expect(fields[0].controller!.text, isNot('https://api.openai.com/v1'),
        reason: 'baseUrl 应预填真实配置而非默认值');
    expect(fields[1].controller!.text, isNot('gpt-3.5-turbo'),
        reason: 'model 应预填真实配置而非默认值');
    expect(fields[2].controller!.text, isNotEmpty,
        reason: 'apiKey 应预填真实配置');

    // 设置页按 tab 组织：切到「数据」tab 应看到每日 Token 使用区块（纯展示，不依赖是否有记录）
    await tester.tap(find.text('数据'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('每日 Token 使用'), findsOneWidget);
  });
}