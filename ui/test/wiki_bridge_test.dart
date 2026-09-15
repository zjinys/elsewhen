import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';

void main() {
  test('Rust bridge wiki integration', () async {
    final repo = RustBridgeRepository();
    await repo.initialize();

    // 1. List wiki pages (all kinds)
    print('\n1. Listing wiki pages...');
    final pages = await repo.listWikiPages();
    print('   ✓ Found ${pages.length} wiki pages');
    expect(pages.length, greaterThan(0));

    for (final page in pages.take(10)) {
      print('   - [${page.kind}] ${page.title} (证据 ${page.evidenceCount})');
    }

    // 2. Get a single page by slug
    print('\n2. Getting page by slug: ${pages.first.slug}');
    final detail = await repo.getWikiPage(pages.first.slug);
    expect(detail, isNotNull);
    print('   ✓ Title: ${detail!.title}');
    print('   ✓ Slug: ${detail.slug}');
    print('   ✓ Summary: ${detail.summary}');
    print('   ✓ Content length: ${detail.contentMd.length} chars');
    print('   ✓ Tags: ${detail.tags}');

    // 3. Kind filter
    print('\n3. Listing pages by kind=insight...');
    final insights = await repo.listWikiPages(kind: 'insight');
    print('   ✓ Found ${insights.length} insight pages');
    expect(insights.isNotEmpty, isTrue);

    // 4. Nonexistent slug returns null
    print('\n4. Getting nonexistent slug...');
    final missing = await repo.getWikiPage('no-such-page');
    expect(missing, isNull);
    print('   ✓ Correctly returned null');

    print('\nWiki bridge test complete! 🎉');
  });
}