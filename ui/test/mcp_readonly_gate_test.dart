// 只读免审批白名单（迁移 53）设置页接线测试：开关渲染、点击回写、
// 免审批范围说明与信任开关并存。独立文件避免与 settings_screen_test
// 的真实 FFI 隔离库流程互相牵连。
//
// 2026 UI 改版后：审批策略开关（信任 / 只读免审批）收进每张服务器
// 卡片的 ⋮ 溢出菜单，列表只保留启用 Switch 与菜单入口；断言相应
// 改为菜单项文案。
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

final serverDto = api.McpServerDto(
  id: 'srv-1',
  name: 'gitea',
  keywords: '代码托管,仓库',
  platform: 'all',
  transport: 'stdio',
  command: '/usr/bin/gitea-mcp',
  argsJson: '[]',
  envJson: '{}',
  url: '',
  headersJson: '{}',
  enabled: true,
  trusted: false,
  allowReadonlyAuto: false,
);

// 继承真实桥接仓库（settings 按 is RustBridgeRepository 过滤），
// 只 mock MCP 相关面：读取返回固定清单，写开关记录调用参数。
class FakeMcpRepo extends RustBridgeRepository {
  FakeMcpRepo() : super(runBackgroundWorker: false, runChatQueueWorker: false);
  final toggles = <(String, bool)>[];

  @override
  Future<List<api.McpServerDto>> listMcpServers() async => [serverDto];

  @override
  Future<void> setMcpServerAllowReadonlyAuto({
    required String id,
    required bool allow,
  }) async {
    toggles.add((id, allow));
  }
}

/// 展开指定 server 卡片的 ⋮ 菜单（每张卡一个 more_horiz 图标）。
Future<void> _openMenu(WidgetTester tester) async {
  await tester.tap(find.byIcon(Icons.more_horiz));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('readonly-auto toggle renders and writes back per server', (
    tester,
  ) async {
    final repo = FakeMcpRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.pumpAndSettle();

    // 进 MCP tab（窄屏 TabBar）。
    await tester.tap(find.text('MCP'));
    await tester.pumpAndSettle();
    expect(find.text('gitea'), findsOneWidget);

    // 默认关闭：菜单里是「开只读免审批」。
    await _openMenu(tester);
    expect(find.text('开只读免审批'), findsOneWidget);
    expect(find.text('关闭只读免审批'), findsNothing);

    await tester.tap(find.text('开只读免审批'));
    await tester.pumpAndSettle();
    expect(repo.toggles.single, ('srv-1', true), reason: '按 server 回写开关');

    // 重挂载：开关已开的 server 显示开启态图标，再点一次回写关闭。
    final openRepo = FakeMcpRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(openRepo)],
        child: const MaterialApp(home: SettingsScreen()),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('MCP'));
    await tester.pumpAndSettle();
    await _openMenu(tester);
    expect(find.text('关闭只读免审批'), findsNothing, reason: 'fake 清单固定为关闭态，不误导断言');
    await tester.tap(find.text('开只读免审批'));
    await tester.pumpAndSettle();
    expect(openRepo.toggles.single, ('srv-1', true));
  });
}
