import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';

void main() {
  test('Rust bridge integration', () async {
    print('Testing Flutter-Rust bridge...\n');

    final repo = RustBridgeRepository();

    // Initialize
    print('1. Initializing bridge...');
    await repo.initialize();
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

    expect(updatedEvents.length, greaterThan(events.length));
    print('Bridge test complete! 🎉');
  });
}
