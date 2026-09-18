import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';

import 'support/isolated_bridge.dart';

void main() {
  test('Rust bridge integration', () async {
    print('Testing Flutter-Rust bridge...\n');

    // Initialize against a fresh temporary database.
    print('1. Initializing bridge...');
    final repo = await createIsolatedBridge();
    print('   ✓ Bridge initialized\n');

    // List events
    print('2. Listing events...');
    final events = await repo.listEvents();
    print('   ✓ Found ${events.length} events\n');

    if (events.isNotEmpty) {
      print('3. Recent events:');
      for (var event in events.take(3)) {
        print('   - ${event.recordedAt}: ${event.rawText}');
      }
      print('');
    }

    // Record a new event
    print('4. Recording new event...');
    final newEvent = await repo.recordEvent('测试从 Flutter 记录事件');
    print('   ✓ Event recorded: ${newEvent.id}');
    print('   Content: ${newEvent.rawText}\n');

    // List again to verify
    print('5. Verifying new event...');
    final updatedEvents = await repo.listEvents();
    print('   ✓ Total events now: ${updatedEvents.length}\n');

    expect(events, isEmpty);
    expect(updatedEvents, hasLength(1));

    final submitted = await repo.submitInput(
      '统一输入桥接测试',
      idempotencyKey: 'bridge-submit-1',
    );
    final repeated = await repo.submitInput(
      '不应重复创建',
      idempotencyKey: 'bridge-submit-1',
    );
    expect(repeated.id, submitted.id);
    expect(submitted.routeStatus, 'routed');
    expect(submitted.eventId, isNotNull);
    expect(await repo.listEvents(), hasLength(2));
    await repo.recordUnifiedInput('Capture 统一输入测试');
    expect(await repo.listEvents(), hasLength(3));
    expect(await repo.triggerAnalysis(), 'no_provider');
    expect((await repo.getAnalysisJobStats()).pending, 3);
    final today = DateFormat('yyyy-MM-dd').format(DateTime.now());
    final daily = await repo.listDailyEntries(today);
    expect(daily, hasLength(3));
    expect(daily.where((entry) => entry.inputId != null), hasLength(2));
    expect(daily.where((entry) => entry.messageId != null), isEmpty);

    final urlInput = await repo.beginUrlInput('https://example.com/article');
    expect(urlInput.source, 'url_import');
    expect(urlInput.routeStatus, 'needs_confirmation');
    expect(urlInput.eventId, isNull);
    final linkedUrlInput = await repo.finishUrlInput(
      urlInput.id,
      wikiPageSlug: 'import-test-page',
    );
    expect(linkedUrlInput.routeStatus, 'routed');
    expect(linkedUrlInput.wikiPageSlug, 'import-test-page');
    expect(await repo.listEvents(), hasLength(3));
    print('Bridge test complete! 🎉');
  });
}
