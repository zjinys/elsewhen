// 只读免审批白名单（迁移 53）设置页接线测试：开关渲染、点击回写、
// 免审批范围说明与信任开关并存。独立文件避免与 settings_screen_test
// 的真实 FFI 隔离库流程互相牵连。
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:elsewhen_ui/screens/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

final serverDto = api.McpServerDto(
  id: 'srv-1',
  name: 'gitea',
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

    // 默认关闭：锁形图标 + 提示说明可打开。
    final lock = find.byTooltip(
      '只读免审批（点击后仅标了 readOnlyHint 的工具免逐次审批）',
    );
    expect(lock, findsOneWidget);

    await tester.tap(lock);
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
    final unlock = find.byTooltip(
      '只读免审批已开（仅豁免标了 readOnlyHint 的工具，点击关闭）',
    );
    expect(unlock, findsNothing, reason: 'fake 清单固定为关闭态，不误导断言');
    await tester.tap(find.byTooltip(
      '只读免审批（点击后仅标了 readOnlyHint 的工具免逐次审批）',
    ));
    await tester.pumpAndSettle();
    expect(openRepo.toggles.single, ('srv-1', true));
  });
}
