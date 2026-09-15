import 'ui/lib/data/storage_repository.dart';
import 'ui/lib/data/mock_storage_repository.dart';
import 'ui/lib/bridge/rust_bridge_repository.dart';

void main() async {
  print('Testing Storage Adapter Architecture\n');
  
  // Test 1: Mock implementation
  print('=== Test 1: MockStorageRepository ===');
  final mockRepo = MockStorageRepository();
  await mockRepo.initialize();
  
  final mockEvent = await mockRepo.recordEvent('Test event from mock');
  print('✓ Recorded event: ${mockEvent.id}');
  
  final mockEvents = await mockRepo.listEvents();
  print('✓ Listed ${mockEvents.length} events');
  
  final mockProvider = await mockRepo.getAiProvider();
  print('✓ AI Provider: $mockProvider');
  
  print('\n=== Test 2: RustBridgeRepository ===');
  final rustRepo = RustBridgeRepository();
  await rustRepo.initialize();
  
  final rustEvent = await rustRepo.recordEvent('Test event from Rust bridge');
  print('✓ Recorded event: ${rustEvent.id}');
  
  final rustEvents = await rustRepo.listEvents();
  print('✓ Listed ${rustEvents.length} events');
  
  final rustProvider = await rustRepo.getAiProvider();
  print('✓ AI Provider: ${rustProvider ?? "not configured"}');
  
  print('\n=== Test 3: Polymorphism ===');
  Future<void> testRepo(StorageRepository repo, String name) async {
    await repo.initialize();
    final event = await repo.recordEvent('Polymorphic test');
    print('✓ $name: recorded event ${event.id}');
  }
  
  await testRepo(MockStorageRepository(), 'Mock');
  await testRepo(RustBridgeRepository(), 'Rust');
  
  print('\n✓ All adapter tests passed!');
}
