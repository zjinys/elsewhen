import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/generated.dart/api/scout.dart' as api;
import 'package:elsewhen_ui/providers/scout_provider.dart';

ScoutFindingItem item(Map<String, dynamic> payload, {String key = 'k'}) =>
    ScoutFindingItem(
      id: 1,
      sourceId: 's',
      kind: 'new',
      itemKey: key,
      payload: payload,
      detectedAt: '2026-11-16T00:00:00Z',
      readAt: null,
    );

api.ScoutFindingDto dto({
  String payloadJson = '{}',
  String itemKey = 'key1',
  String kind = 'new',
  String? deltaJson,
}) =>
    api.ScoutFindingDto(
      id: 2,
      sourceId: 's',
      snapshotId: 1,
      kind: kind,
      itemKey: itemKey,
      payloadJson: payloadJson,
      deltaJson: deltaJson,
      detectedAt: '2026-11-16T00:00:00Z',
    );

void main() {
  mainSignals();
  group('ScoutFindingItem.title', () {
    test('跨源兜底链 name/title/full_name', () {
      expect(item({'name': 'A'}).title, 'A');
      expect(item({'title': 'B'}).title, 'B');
      expect(item({'full_name': 'o/r'}).title, 'o/r');
    });

    test('HTML 实体被解码', () {
      expect(item({'title': 'MAP &amp; GO!'}).title, 'MAP & GO!');
    });

    test('无名字段时落回 itemKey', () {
      expect(item({'id': 'x'}, key: 'o/r').title, 'o/r');
    });
  });

  group('ScoutFindingItem.subtitle', () {
    test('剥 HTML 标签并解实体', () {
      expect(
        item({'summary': '&lt;p&gt; Open recorder &lt;/p&gt;'}).subtitle,
        'Open recorder',
      );
      expect(
        item({'summary': 'A &amp; B <b>bold</b>'}).subtitle,
        'A & B bold',
      );
    });

    test('去 RSS 尾部导航噪音', () {
      expect(
        item({'summary': 'Turn Webflow app into a project | Link'}).subtitle,
        'Turn Webflow app into a project',
      );
    });

    test('超长截断到 140', () {
      final long = 'x' * 200;
      final got = item({'description': long}).subtitle!;
      expect(got.length, 141);
      expect(got.endsWith('…'), isTrue);
    });

    test('空候选返回 null', () {
      expect(item({'id': '1'}).subtitle, isNull);
    });
  });

  group('ScoutFindingItem.link', () {
    test('兜底链 url/html_url/permalink/link', () {
      expect(item({'url': 'https://a'}).link, 'https://a');
      expect(item({'html_url': 'https://b'}).link, 'https://b');
      expect(item({'permalink': 'https://c'}).link, 'https://c');
      expect(item({'link': 'https://d'}).link, 'https://d');
      expect(item({}).link, isNull);
    });
  });

  group('ScoutFindingItem.fromDto', () {
    test('payloadJson 解码失败不阻塞，title 落回 itemKey', () {
      final got = ScoutFindingItem.fromDto(dto(payloadJson: '{not json'));
      expect(got.payload, isEmpty);
      expect(got.title, 'key1');
    });

    test('gone 且 payload 有 title 时标题可读', () {
      final got = ScoutFindingItem.fromDto(dto(
        kind: 'gone',
        itemKey: 'tag:www.producthunt.com,2005:Post/1313374',
        payloadJson:
            '{"id":"tag:www.producthunt.com,2005:Post/1313374","title":"Kairos"}',
      ));
      expect(got.title, 'Kairos');
    });
  });
}

// ── 关键指标 / delta / 新增字段 ────────────────────────────────────

void mainSignals() {

ScoutFindingItem changed(
  Map<String, dynamic> payload,
  Map<String, dynamic> delta,
) => ScoutFindingItem(
  id: 1,
  sourceId: 's',
  kind: 'changed',
  itemKey: 'k',
  payload: payload,
  delta: delta,
  detectedAt: '2026-11-16T00:00:00Z',
  readAt: null,
);

  // ── 关键指标 / delta / 新增字段 ────────────────────────────────

  test('github signals expose stars/forks/issues/license', () {
    final it = item({
      'stargazers_count': 57225,
      'forks_count': 1796,
      'open_issues_count': 1355,
      'license_spdx_id': 'Apache-2.0',
      'language': 'Rust',
    });
    final labels = it.signals.map((s) => s.label).toList();
    expect(labels, ['star', 'fork', 'issue', '许可', '语言']);
    expect(it.signals.first.value, '57.2k');
  });

  test('diff is carried onto the matching signal', () {
    final it = changed(
      {'stargazers_count': 57225},
      {
        'stargazers_count': {'from': 57191, 'to': 57225, 'diff': 34},
      },
    );
    final sig = it.signals.first;
    expect(sig.diff, 34);
    expect(sig.hasDiff, isTrue);
  });

  test('gumroad price renders with currency, zero is 免费', () {
    final paid = item({'price_cents': 24900, 'currency_code': 'usd'});
    expect(paid.signals.first.value, 'USD 249.00');
    final free = item({'price_cents': 0, 'currency_code': 'usd'});
    expect(free.signals.first.value, '免费');
  });

  test('pay-what-you-want gets its own signal', () {
    final it = item({'price_cents': 500, 'is_pay_what_you_want': true});
    expect(it.signals.map((s) => s.value), contains('随意付'));
  });

  test('app store rank and genre are surfaced', () {
    final it = item({'rank': 25, 'genres_0_name': 'News'});
    expect(it.signals.first.value, '25 名');
    expect(it.signals.map((s) => s.value), contains('News'));
  });

  test('archived repo is flagged', () {
    final it = item({'archived': true});
    expect(it.signals.map((s) => s.value), contains('已归档'));
  });

  test('topics are capped at four', () {
    final it = item({
      'topics': ['a', 'b', 'c', 'd', 'e', 'f'],
    });
    expect(it.topics, ['a', 'b', 'c', 'd']);
  });

  test('since falls back across created_at/releaseDate/published', () {
    expect(item({'created_at': '2019-09-24T21:41:56Z'}).since, '2019-09');
    expect(item({'releaseDate': '2009-10-09'}).since, '2009-10');
    expect(item({'published': 'bad'}).since, 'bad');
  });

  test('delta_json on dto is parsed into the map', () {
    final d = dto(
      payloadJson: '{"stargazers_count":10}',
      kind: 'changed',
      deltaJson: '{"stargazers_count":{"from":8,"to":10,"diff":2}}',
    );
    final it = ScoutFindingItem.fromDto(d);
    expect(it.delta['stargazers_count']['diff'], 2);
  });

  test('malformed delta_json degrades to empty, keeps payload', () {
    final d = dto(payloadJson: '{"name":"x"}', deltaJson: 'not json');
    final it = ScoutFindingItem.fromDto(d);
    expect(it.delta, isEmpty);
    expect(it.title, 'x');
  });

  test('missing delta_json is empty', () {
    expect(ScoutFindingItem.fromDto(dto()).delta, isEmpty);
  });
}
