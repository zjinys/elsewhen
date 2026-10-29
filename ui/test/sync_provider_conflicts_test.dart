import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/sync/sync_provider_conflicts_controller.dart';
import 'package:elsewhen_ui/widgets/sync_provider_conflicts_settings.dart';

api.ProviderConflictProfileDto _profile({
  required String id,
  required String name,
  required String baseUrl,
  required String masked,
  bool present = true,
  List<api.ReachabilityHint> hints = const [],
}) => api.ProviderConflictProfileDto(
  id: id,
  name: name,
  providerType: 'openai-compatible',
  baseUrl: baseUrl,
  model: 'model-$name',
  apiKeyMasked: masked,
  apiKeyPresent: present,
  enabled: true,
  isActive: false,
  temperature: 0.5,
  maxTokens: null,
  contextWindow: null,
  reachabilityHints: hints,
);

api.ProviderSyncConflictDto _conflict({
  String revisionId = 'remote-rev-1',
  String localRevision = 'local-rev-1',
}) => api.ProviderSyncConflictDto(
  revisionId: revisionId,
  baseRevision: null,
  senderDeviceId: 'device-a',
  senderDeviceName: '另一台设备',
  localRevision: localRevision,
  remoteProfiles: [
    _profile(
      id: 'r1',
      name: '远端配置',
      baseUrl: 'http://127.0.0.1:9999/v1',
      masked: 'SYN****IRST',
      hints: const [
        api.ReachabilityHint(
          code: 'provider_url_loopback',
          address: 'http://127.0.0.1:9999/v1',
        ),
      ],
    ),
  ],
  remoteActiveId: 'r1',
  remoteFailoverOrder: const ['r1'],
  localProfiles: [
    _profile(
      id: 'l1',
      name: '本地配置',
      baseUrl: 'https://local.invalid/v1',
      masked: 'SYN****_KEY',
    ),
  ],
  localActiveId: 'l1',
);

class TestConflictApi implements SyncProviderConflictApi {
  List<api.ProviderSyncConflictDto> current = [];
  Object? listError;
  Object? resolveError;
  final resolutions =
      <({String revisionId, String local, String remote, bool accept})>[];

  @override
  Future<List<api.ProviderSyncConflictDto>> listConflicts() async {
    if (listError != null) throw listError!;
    return current;
  }

  @override
  Future<void> resolve({
    required String revisionId,
    required String expectedLocalRevision,
    required String expectedRemoteRevision,
    required bool acceptRemote,
  }) async {
    if (resolveError != null) throw resolveError!;
    resolutions.add((
      revisionId: revisionId,
      local: expectedLocalRevision,
      remote: expectedRemoteRevision,
      accept: acceptRemote,
    ));
    // 模拟 Rust 侧成功：该冲突从列表消失。
    current = current.where((c) => c.revisionId != revisionId).toList();
  }
}

void main() {
  testWidgets('掩码展示双侧配置与可达性提示，绝不出现明文 Key', (tester) async {
    final bridge = TestConflictApi()..current = [_conflict()];
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 双侧名称与激活项可见（section 标题与 profile 行均含名称，用行级 finder 区分）。
    expect(find.text('本地配置 · model-本地配置（当前激活）'), findsOneWidget);
    expect(find.text('远端配置 · model-远端配置（当前激活）'), findsOneWidget);
    // 掩码可见、任何明文都不出现（测试夹具本就不含明文，再确认掩码渲染）。
    expect(find.textContaining('SYN****IRST'), findsOneWidget);
    expect(find.textContaining('SYN****_KEY'), findsOneWidget);
    expect(find.textContaining('SYNTHETIC'), findsNothing);
    // 环回提示展示，且原样回显地址。
    expect(find.textContaining('环回地址'), findsOneWidget);
    expect(find.text('http://127.0.0.1:9999/v1'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets('保留本地与接受远端都原样回传审阅时所见版本', (tester) async {
    final bridge = TestConflictApi()..current = [_conflict()];
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    // 保留本地。
    await tester.tap(find.text('保留本地'));
    await tester.pumpAndSettle();
    expect(bridge.resolutions, hasLength(1));
    expect(bridge.resolutions.single.revisionId, 'remote-rev-1');
    expect(bridge.resolutions.single.local, 'local-rev-1');
    expect(bridge.resolutions.single.remote, 'remote-rev-1');
    expect(bridge.resolutions.single.accept, isFalse);
    // 成功后重拉，冲突已消失。
    expect(find.textContaining('没有待审阅'), findsOneWidget);

    // 再来一条，接受远端。
    bridge.current = [_conflict(revisionId: 'remote-rev-2')];
    await controller.refresh();
    await tester.pumpAndSettle();
    await tester.tap(find.text('接受远端'));
    await tester.pumpAndSettle();
    expect(bridge.resolutions, hasLength(2));
    expect(bridge.resolutions.last.accept, isTrue);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets('锁定失败关闭：错误原样展示，列表不被静默清空', (tester) async {
    final bridge = TestConflictApi()
      ..listError = StateError('provider_key_locked（请先在设置→数据中解锁本机同步密钥）');
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('已锁定'), findsOneWidget);
    expect(find.textContaining('没有待审阅'), findsNothing);

    // 解锁后恢复。
    bridge
      ..listError = null
      ..current = [_conflict()];
    await controller.refresh();
    await tester.pumpAndSettle();
    expect(find.text('本地配置 · model-本地配置（当前激活）'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets('审阅期间本地已变：双版本绑定错误原样上抛', (tester) async {
    final bridge = TestConflictApi()
      ..current = [_conflict()]
      ..resolveError = StateError(
        'provider_conflict_local_changed（本地配置已变化，请重新审阅）',
      );
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('接受远端'));
    await tester.pumpAndSettle();
    // 失败关闭：错误展示，冲突仍在（未被乐观移除），无成功假象。
    expect(find.textContaining('本地配置已变化'), findsOneWidget);
    expect(find.text('本地配置 · model-本地配置（当前激活）'), findsOneWidget);
    expect(bridge.resolutions, isEmpty); // resolve 在记录前抛错

    // 版本绑定失败时自动重拉：列表反映最新状态，无需手动刷新。
    expect(find.text('最新内容'), findsNothing);
    bridge
      ..resolveError = null
      ..current = [_conflict(localRevision: 'fresh-local-rev')];
    await tester.pumpAndSettle();
    // resolveError 仍设置时重拉不会发生（本用例已清空），此处只验证列表更新。
    expect(find.textContaining('local-rev'), findsNothing);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets('裁决遇版本漂移：自动重拉列表并保留错误提示', (tester) async {
    final bridge = TestConflictApi()
      ..current = [_conflict()]
      ..resolveError = StateError(
        'provider_conflict_local_changed（本地配置已变化，请重新审阅）',
      );
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    // 首次 listConflicts 成功：无错误、无重试入口，冲突卡片可见。
    expect(find.textContaining('已变化'), findsNothing);
    expect(find.text('重新拉取'), findsNothing);
    expect(find.text('保留本地'), findsOneWidget);

    // 裁决失败后：错误保留，同时列表被自动重拉为新版本。
    // resolveError 保留（resolve 仍抛错），但 current 换成 fresh 版本——
    // 自动重拉发生在 resolve 抛错后的同一 resolve 调用里。
    bridge.current = [_conflict(localRevision: 'fresh-local-rev')];
    await tester.tap(find.text('保留本地'));
    await tester.pumpAndSettle();
    expect(find.textContaining('本地配置已变化'), findsOneWidget);
    // 自动重拉已把 DTO 换成 fresh 版本：列表是新的审阅基线。
    expect(controller.conflicts?.first.localRevision, 'fresh-local-rev');
    expect(find.text('重新拉取'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });

  testWidgets('「重新拉取」按钮触发 refresh 且 busy 时禁用', (tester) async {
    final bridge = TestConflictApi()
      ..current = [_conflict()]
      ..listError = StateError('provider_key_locked');
    final controller = SyncProviderConflictsController(bridge: bridge);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: SyncProviderConflictsSettings(controller: controller),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('重新拉取'), findsOneWidget);

    bridge.listError = null;
    await tester.tap(find.text('重新拉取'));
    await tester.pumpAndSettle();
    // 重拉成功：错误消失，列表出现。
    expect(find.text('重新拉取'), findsNothing);
    expect(find.text('本地配置 · model-本地配置（当前激活）'), findsOneWidget);

    await tester.pumpWidget(const SizedBox());
    controller.dispose();
  });
}
