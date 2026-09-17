import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';

/// Rust bridge wiki 集成测试：只读，不依赖外部 demo 数据。
/// 对活动数据目录里的知识库做真实桥接调用（list/get/kind 过滤）。
void main() {
  test('Rust bridge wiki integration', () async {
    final repo = RustBridgeRepository();
    await repo.initialize();

    // 1. List wiki pages (all kinds)
    final pages = await repo.listWikiPages();
    expect(pages.length, greaterThan(0), reason: '知识库应至少有一页');
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
        expect(slugs.contains(p.slug), isTrue,
            reason: '过滤结果应为全量子集 ($kind)');
      }
    }

    // 4. Nonexistent slug returns null
    final missing = await repo.getWikiPage('no-such-page');
    expect(missing, isNull);
    print('   ✓ Correctly returned null');

    print('\nWiki bridge test complete! 🎉');
  });
}