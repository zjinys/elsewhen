import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';
import 'support/isolated_bridge.dart';

void main() {
  setUpAll(() async { await createIsolatedBridge(); });
  test('prompt draft activation reset and history roundtrip over real bridge', () async {
    final before = (await api.listPromptDefinitions(locale: 'zh-Hans')).singleWhere((p) => p.key == 'chat.personal.policy');
    final id = await api.savePromptRevision(key: before.key, locale: 'zh-Hans', body: '测试角色：按具体事实简短回答。', parentRevisionId: before.currentRevisionId, note: '桥接回归');
    expect((await api.listPromptDefinitions(locale: 'zh-Hans')).singleWhere((p) => p.key == before.key).currentRevisionId, before.currentRevisionId);
    await api.activatePromptRevision(key: before.key, locale: 'zh-Hans', revisionId: id, expectedGeneration: before.generation);
    final active = (await api.listPromptDefinitions(locale: 'zh-Hans')).singleWhere((p) => p.key == before.key);
    expect(active.currentRevisionId, id);
    expect(active.currentBody, '测试角色：按具体事实简短回答。');
    expect((await api.listPromptRevisions(key: before.key, locale: 'zh-Hans', offset: 0)).any((r) => r.id == id), isTrue);
    await api.activatePromptRevision(key: before.key, locale: 'zh-Hans', revisionId: null, expectedGeneration: active.generation);
    expect((await api.listPromptDefinitions(locale: 'zh-Hans')).singleWhere((p) => p.key == before.key).currentRevisionId, before.currentRevisionId);
  });
}
