import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

/// Rust bridge wiki 集成测试：在隔离库里自种数据后验证 list/get/kind 过滤。
void main() {
  test('Rust bridge wiki integration', () async {
    final repo = await createIsolatedBridge();
    final seeded = await repo.saveTextPage(
      text: '用于验证真实 Rust bridge 的隔离测试页面。',
      title: 'Bridge 隔离测试',
      tags: const ['test'],
    );

    // 1. List wiki pages (all kinds)
    final pages = await repo.listWikiPages();
    expect(pages, hasLength(1), reason: '隔离库应只包含自种页面');
    expect(pages.single.slug, seeded.slug);
    print('   ✓ Found ${pages.length} wiki pages');

    for (final page in pages.take(10)) {
      print('   - [${page.kind}] ${page.title} (证据 ${page.evidenceCount})');
    }

    // 2. Get a single page by slug
    final detail = await repo.getWikiPage(pages.first.slug);
    expect(detail, isNotNull);
    print('   ✓ Title: ${detail!.title}');
    print('   ✓ Slug: ${detail.slug}');
    print('   ✓ Summary: ${detail.summary}');
    print('   ✓ Content length: ${detail.contentMd.length} chars');
    print('   ✓ Tags: ${detail.tags}');

    // 3. Kind filter：按全部页面里出现的每一种 kind 过滤，应为全量的子集
    final kinds = pages.map((p) => p.kind).toSet();
    expect(kinds, isNotEmpty);
    for (final kind in kinds) {
      final filtered = await repo.listWikiPages(kind: kind);
      expect(filtered, isNotEmpty, reason: 'kind=$kind 过滤应有结果');
      final slugs = pages.map((p) => p.slug).toSet();
      for (final p in filtered) {
        expect(slugs.contains(p.slug), isTrue, reason: '过滤结果应为全量子集 ($kind)');
      }
    }

    // 4. Nonexistent slug returns null
    final missing = await repo.getWikiPage('no-such-page');
    expect(missing, isNull);
    print('   ✓ Correctly returned null');

    print('\nWiki bridge test complete! 🎉');
  });
}
