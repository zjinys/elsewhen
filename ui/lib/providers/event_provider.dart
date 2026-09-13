import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/event.dart';

// Mock repository for now - will be replaced with Rust bridge
class EventRepository {
  final List<Event> _events = [];

  Future<List<Event>> getEvents() async {
    // TODO: Call Rust backend via FFI
    await Future.delayed(const Duration(milliseconds: 100));
    return List.from(_events.reversed);
  }

  Future<Event> createEvent(String rawText) async {
    // TODO: Call Rust backend via FFI
    final event = Event(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      rawText: rawText,
      recordedAt: DateTime.now(),
      source: 'flutter_gui',
    );

    _events.add(event);
    await Future.delayed(const Duration(milliseconds: 50));
    return event;
  }

  Future<void> deleteEvent(String id) async {
    // TODO: Call Rust backend via FFI
    _events.removeWhere((e) => e.id == id);
    await Future.delayed(const Duration(milliseconds: 50));
  }
}

// Providers
final eventRepositoryProvider = Provider<EventRepository>((ref) {
  return EventRepository();
});

final eventsProvider = FutureProvider<List<Event>>((ref) async {
  final repo = ref.watch(eventRepositoryProvider);
  return repo.getEvents();
});

final eventInputProvider = StateProvider<String>((ref) => '');
