import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/widgets/sync_key_health_settings.dart';

class FakeHealthApi implements SyncKeyHealthApi {
  FakeHealthApi(this.result);
  SyncKeyStoreHealth result;
  Object? error;
  final calls = <String>[];

  @override
  Future<SyncKeyStoreHealth> probe(String installationId) async {
    calls.add(installationId);
    // 模拟真实异步探针：让 tap 后的 result/error 赋值先落到 fake 上。
    await Future<void>.delayed(Duration.zero);
    if (error != null) throw error!;
    return result;
  }
}

Widget _host(FakeHealthApi api, String installationId) => MaterialApp(
  home: Scaffold(
    body: SyncKeyHealthSettings(api: api, installationId: installationId),
  ),
);

void main() {
  testWidgets('健康状态展示安全存储托管边界', (tester) async {
    final api = FakeHealthApi(SyncKeyStoreHealth.ok);
    await tester.pumpWidget(_host(api, 'install-a'));
    await tester.pumpAndSettle();
    expect(find.textContaining('系统安全存储托管'), findsOneWidget);
    expect(find.textContaining('不进云备份'), findsOneWidget);
    expect(api.calls, ['install-a']);
  });

  testWidgets('损坏/缺失/不可用各自给处置指引且不覆盖原记录', (tester) async {
    final api = FakeHealthApi(SyncKeyStoreHealth.corrupt);
    await tester.pumpWidget(_host(api, 'install-a'));
    await tester.pumpAndSettle();
    expect(find.textContaining('记录损坏'), findsOneWidget);
    expect(find.textContaining('不会覆盖原记录'), findsOneWidget);

    await tester.tap(find.text('检查'));
    api.result = SyncKeyStoreHealth.missing;
    await tester.pumpAndSettle();
    expect(find.textContaining('不会生成替代密钥'), findsOneWidget);

    await tester.tap(find.text('检查'));
    api.error = Exception('platform boom');
    await tester.pumpAndSettle();
    expect(find.textContaining('系统安全存储不可用'), findsOneWidget);
    expect(
      find.textContaining('platform boom'),
      findsNothing,
      reason: '平台诊断信息不得外泄',
    );
  });

  testWidgets('探针抛错不崩溃、按不可用展示', (tester) async {
    final api = FakeHealthApi(SyncKeyStoreHealth.ok);
    api.error = StateError('boom');
    await tester.pumpWidget(_host(api, 'install-a'));
    await tester.pumpAndSettle();
    expect(find.textContaining('不可用'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
