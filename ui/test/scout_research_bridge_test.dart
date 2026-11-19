import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

import 'support/isolated_bridge.dart';

void main() {
  setUpAll(() async {
    await createIsolatedBridge();
  });
  test('研究经真实桥接保存、冲突拒绝并在清空镜像后可读', () async {
    final document = {
      'instance_id': 'fixture',
      'product_id': '["gumroad","one"]',
      'title': 'Research fixture',
      'state': 'research',
      'question': 'Who pays?',
      'materials': [
        {
          'id': 'e1',
          'url': 'https://example.com/review',
          'body': 'I would pay for team collaboration.',
          'observed_at': '2026-11-17',
          'type': 'review',
        },
      ],
    };
    await api.saveScoutResearchDraft(
      documentJson: jsonEncode({...document, 'question': 'Recover this draft'}),
    );
    await api.clearScoutMirror();
    expect(
      (jsonDecode(
        await api.scoutResearchDraft(
          instanceId: 'fixture',
          productId: '["gumroad","one"]',
        ),
      ) as Map)['question'],
      'Recover this draft',
    );
    final saved = jsonDecode(
      await api.saveScoutResearch(documentJson: jsonEncode(document)),
    ) as Map;
    expect(saved['slug'], startsWith('scout-research-'));
    expect(jsonDecode(await api.scoutResearchDrafts()), isEmpty);
    await api.clearScoutMirror();
    final rows = jsonDecode(await api.listScoutResearch()) as List;
    expect(rows.single['question'], 'Who pays?');
    await expectLater(
      api.saveScoutResearch(documentJson: jsonEncode(document)),
      throwsA(anything),
    );
    await api.saveScoutResearch(
      documentJson: jsonEncode({...document, 'state': 'validating'}),
      expectedToken: saved['token'] as String,
    );
    expect(
      (jsonDecode(await api.listScoutResearch()) as List).single['state'],
      'validating',
    );
  });
  test('个人偏好经真实桥接同步文档保存，旧 token 拒绝覆盖', () async {
    expect(jsonDecode(await api.scoutPreferences()), isEmpty);
    final saved = jsonDecode(
      await api.saveScoutPreferences(
        documentJson: jsonEncode({
          'audience': 'Designers',
          'constraints': '2 hours/week',
        }),
      ),
    ) as Map;
    await api.clearScoutMirror();
    expect(
      (jsonDecode(await api.scoutPreferences()) as Map)['audience'],
      'Designers',
    );
    await expectLater(
      api.saveScoutPreferences(documentJson: '{}'),
      throwsA(anything),
    );
    await api.saveScoutPreferences(
      documentJson: '{}',
      expectedToken: saved['token'] as String,
    );
    expect((jsonDecode(await api.scoutPreferences()) as Map)['audience'], '');
  });
}
