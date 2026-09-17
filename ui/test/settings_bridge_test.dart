import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/bridge/generated.dart/api.dart' as api;

/// 设置页接线验证（真实 FFI）：
/// ① 多配置 AI provider 保存/读取往返（api_key 安全设计：明文只上行、不回传，
///    用 api_key_source 标记「已配置」）
/// ② 每日 token 用量查询。
/// 环境无关：不假设空库，且用后删除测试配置，不污染共享数据目录。
/// 注意：同一 isolate 内 RustLib.init 只能初始化一次，因此所有用例共享一个 repo 实例。
void main() {
  late RustBridgeRepository repo;

  setUpAll(() async {
    repo = RustBridgeRepository();
    await repo.initialize();
  });

  test('AI provider config save/list roundtrip (api_key 不回传)', () async {
    const testName = '__test_roundtrip__';
    const upUrl = 'https://api.example.com/v1';
    const upModel = 'test-model';
    const upKey = 'sk-test-123';

    // 新建配置：id 传空串表示新建，api_key 传明文（保存时上行）
    final id = await repo.saveAiProviderConfig(api.AiProviderConfigDto(
      id: '',
      name: testName,
      providerType: 'openai-compatible',
      baseUrl: upUrl,
      model: upModel,
      apiKeySource: '',
      apiKey: upKey,
      isActive: false,
      temperature: 0.7,
      maxTokens: null,
    ));
    expect(id, isNotEmpty, reason: '保存应返回新配置 id');

    try {
      final all = await repo.listAiProviderConfigs();
      final created = all.firstWhere((p) => p.id == id);
      expect(created.baseUrl, upUrl);
      expect(created.model, upModel);
      // 安全设计：明文 key 只在保存时上行，读取回传空串
      expect(created.apiKey, isEmpty, reason: 'api_key 出于安全不回传');
      expect(created.apiKeySource, 'database',
          reason: 'apiKeySource 应标注为已配置（数据库来源）');
      print('✓ AI provider config roundtrip OK: ${created.name} @ ${created.baseUrl}');
    } finally {
      // 清理测试配置，避免污染共享测试库
      await repo.deleteAiProviderConfig(id);
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