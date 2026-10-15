import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

import 'support/isolated_bridge.dart';

/// 设置页接线验证（真实 FFI）：
/// ① 多配置 AI provider 保存/读取往返（api_key 安全设计：明文只上行、不回传，
///    用 api_key_source 标记「已配置」）
/// ② 每日 token 用量查询。
/// 环境无关：不假设空库，且用后删除测试配置，不污染共享数据目录。
/// 注意：同一 isolate 内 RustLib.init 只能初始化一次，因此所有用例共享一个 repo 实例。
void main() {
  late RustBridgeRepository repo;

  setUpAll(() async {
    repo = await createIsolatedBridge();
  });

  test('AI provider config save/list roundtrip (api_key 不回传)', () async {
    const testName = '__test_roundtrip__';
    const upUrl = 'https://api.example.com/v1';
    const upModel = 'test-model';
    const upKey = 'sk-test-123';

    // 新建配置：id 传空串表示新建，api_key 传明文（保存时上行）
    final id = await repo.saveAiProviderConfig(
      api.AiProviderConfigDto(
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
      ),
    );
    expect(id, isNotEmpty, reason: '保存应返回新配置 id');

    try {
      final all = await repo.listAiProviderConfigs();
      final created = all.firstWhere((p) => p.id == id);
      expect(created.baseUrl, upUrl);
      expect(created.model, upModel);
      // 安全设计：明文 key 只在保存时上行，读取回传空串
      expect(created.apiKey, isEmpty, reason: 'api_key 出于安全不回传');
      expect(
        created.apiKeySource,
        'database',
        reason: 'apiKeySource 应标注为已配置（数据库来源）',
      );
      print(
        '✓ AI provider config roundtrip OK: ${created.name} @ ${created.baseUrl}',
      );
    } finally {
      // 清理测试配置，避免污染共享测试库
      await repo.deleteAiProviderConfig(id);
    }
  });

  test('规则触发往返（决策辅助 D1）：触发词/场景/提醒偏好', () async {
    final ruleId = await repo.addRule('__D1 触发测试__ X 项目相关要提醒');
    try {
      // 默认：不参与预筛、全场景、偏好 instant
      var rules = await repo.listRules();
      final created = rules.firstWhere((r) => r.id == ruleId);
      expect(created.triggerKeywords, isEmpty);
      // 配置触发：关键词 + 场景
      await repo.setRuleTrigger(
        ruleId,
        triggerScene: 'capture',
        triggerKeywords: ['X项目', 'deadline'],
      );
      rules = await repo.listRules();
      final configured = rules.firstWhere((r) => r.id == ruleId);
      expect(configured.triggerKeywords, ['X项目', 'deadline']);
      expect(configured.triggerScene, 'capture');
      expect(configured.hasTrigger, isTrue);

      // 提醒偏好往返：review_only → off → instant(默认)
      await repo.setRuleTriggerMode('review_only');
      expect(await repo.getRuleTriggerMode(), 'review_only');
      await repo.setRuleTriggerMode('off');
      expect(await repo.getRuleTriggerMode(), 'off');
      await repo.setRuleTriggerMode('instant');
      expect(await repo.getRuleTriggerMode(), 'instant');

      // 最近提醒读面可用（隔离库可能无命中，只验证接口与结构）
      final hits = await repo.listRecentRuleHits(limit: 5);
      expect(hits, isA<List<dynamic>>());
    } finally {
      await repo.deleteRule(ruleId);
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

  test('用户模型面板（D2）：读面结构 + 建页前置校验', () async {
    // 读面：不假设库非空，只验证接口可用与结构正确（模型面板直接消费）。
    final listed = await repo.listRelations();
    for (final r in listed) {
      expect(r.fromSlug, isNotEmpty);
      expect(r.toSlug, isNotEmpty);
      expect(r.relation, isNotEmpty);
      expect(r.confidence, inExclusiveRange(0, 6));
    }
    // add_relation 的前置校验：页面不存在时明确报错（不静默挂空引用）。
    await expectLater(
      repo.addRelation(
        fromSlug: '__no_such_person__',
        toSlug: '__no_such_topic__',
        relation: '参与',
      ),
      throwsA(anything),
    );
  });

  test('theme prefs 读写 roundtrip（含正文字号）', () async {
    await repo.updateThemePrefs(
      mode: 'light',
      preset: 'violet',
      font: 'Noto Sans SC',
      fontSize: 18,
    );
    final prefs = await repo.getThemePrefs();
    expect(prefs.mode, 'light');
    expect(prefs.preset, 'violet');
    expect(prefs.font, 'Noto Sans SC');
    expect(prefs.fontSize, 18, reason: '字号应随主题偏好持久化');

    // 覆盖回默认（模拟用户拖回落）
    await repo.updateThemePrefs(
      mode: 'dark',
      preset: 'amber',
      font: 'system',
      fontSize: 16,
    );
    final back = await repo.getThemePrefs();
    expect(back.mode, 'dark');
    expect(back.fontSize, 16);
  });

  test('theme prefs 编辑器覆盖层 roundtrip（写入 + 跟随全局清除）', () async {
    // 写入覆盖层
    await repo.updateThemePrefs(
      mode: 'dark',
      preset: 'amber',
      font: 'system',
      fontSize: 16,
      editorFont: 'Noto Serif SC',
      editorFontSize: 20,
      editorLineHeight: 2.0,
    );
    final withOverride = await repo.getThemePrefs();
    expect(withOverride.editorFont, 'Noto Serif SC');
    expect(withOverride.editorFontSize, 20);
    expect(withOverride.editorLineHeight, 2.0);

    // 「跟随全局」= 传 null → Rust 侧删除对应 meta 键
    await repo.updateThemePrefs(
      mode: 'dark',
      preset: 'amber',
      font: 'system',
      fontSize: 16,
      editorFont: null,
      editorFontSize: null,
      editorLineHeight: null,
    );
    final cleared = await repo.getThemePrefs();
    expect(cleared.editorFont, isNull, reason: '跟随全局应清掉覆盖键');
    expect(cleared.editorFontSize, isNull);
    expect(cleared.editorLineHeight, isNull);
    // 清除覆盖不影响全局层
    expect(cleared.font, 'system');
    expect(cleared.fontSize, 16);
  });

  test('listPageGuidance returns empty (not error) on unknown slug', () async {
    // D4 聚合视图桥面：未知 slug 空结果、不抛错——空库不劣化契约。
    final items = await repo.listPageGuidance('person/nonexistent-slug');
    expect(items, isEmpty);
  });

  test('analysis job stats expose durable queue statuses', () async {
    final before = await repo.getAnalysisJobStats();
    expect(before.pending, 0);
    expect(before.running, 0);
    expect(before.retry, 0);
    expect(before.succeeded, 0);
    expect(before.failed, 0);

    await repo.recordEvent('需要进入分析队列的隔离测试事件');
    final after = await repo.getAnalysisJobStats();
    expect(after.pending, 1);
    expect(after.running, 0);
    expect(after.retry, 0);
    expect(after.succeeded, 0);
    expect(after.failed, 0);
  });
}
