import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';

/// 设置页接线验证（真实 FFI）：get/update AI provider 配置往返 + 每日 token 用量查询。
/// 环境无关：不假设空库，且用后恢复原配置，不破坏同一数据目录下的其他测试。
/// 注意：同一 isolate 内 RustLib.init 只能初始化一次，因此所有用例共享一个 repo 实例。
void main() {
  late RustBridgeRepository repo;

  setUpAll(() async {
    repo = RustBridgeRepository();
    await repo.initialize();
  });

  test('AI provider config roundtrip', () async {
    final before = await repo.getAiProviderConfig();

    const upUrl = 'https://api.example.com/v1';
    const upModel = 'test-model';
    const upKey = 'sk-test-123';
    try {
      await repo.updateAiProviderConfig(
        baseUrl: upUrl,
        model: upModel,
        apiKey: upKey,
      );

      final after = await repo.getAiProviderConfig();
      expect(after, isNotNull);
      expect(after!.baseUrl, upUrl);
      expect(after.model, upModel);
      expect(after.apiKey, upKey);
      print('✓ AI provider config roundtrip OK: ${after.model} @ ${after.baseUrl}');
    } finally {
      // 恢复原有配置，避免污染共享测试库
      if (before != null) {
        await repo.updateAiProviderConfig(
          baseUrl: before.baseUrl,
          model: before.model,
          apiKey: before.apiKey,
        );
      }
    }
  });

  test('daily token usage returns aggregated list', () async {
    final daily = await repo.getDailyTokenUsage(7);

    // 不假设库里有记录：只验证接口可用、结构正确（日期格式 + 数字字段非负）
    expect(daily, isA<List<dynamic>>());
    for (final day in daily) {
      expect(day.date.length, 10, reason: '日期应为 YYYY-MM-DD');
      expect(day.totalTokens, greaterThanOrEqualTo(0));
      expect(day.callCount, greaterThanOrEqualTo(0));
    }
  });
}