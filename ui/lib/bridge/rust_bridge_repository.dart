import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../models/event.dart';

// Rust Bridge Repository
// This will use the generated bridge code once available
class RustBridgeRepository {
  // Singleton instance
  static final RustBridgeRepository _instance = RustBridgeRepository._internal();
  factory RustBridgeRepository() => _instance;
  RustBridgeRepository._internal();

  bool _initialized = false;

  // Initialize the bridge
  Future<void> initialize() async {
    if (_initialized) return;

    // TODO: Initialize Rust bridge
    // await RustLib.init();

    _initialized = true;
  }

  // Record a new event via Rust
  Future<Event> recordEvent(String rawText) async {
    if (!_initialized) await initialize();

    // TODO: Call Rust bridge
    // final result = await api.recordEvent(rawText: rawText);
    // return Event.fromRust(result);

    // Fallback to mock for now
    return Event(
      id: DateTime.now().millisecondsSinceEpoch.toString(),
      rawText: rawText,
      recordedAt: DateTime.now(),
      source: 'rust_bridge',
    );
  }

  // List all events from Rust
  Future<List<Event>> listEvents() async {
    if (!_initialized) await initialize();

    // TODO: Call Rust bridge
    // final results = await api.listEvents();
    // return results.map((r) => Event.fromRust(r)).toList();

    // Fallback to empty list for now
    return [];
  }

  // Trigger AI analysis
  Future<bool> triggerAnalysis() async {
    if (!_initialized) await initialize();

    // TODO: Call Rust bridge
    // return await api.triggerAnalysis();

    return false;
  }

  // Get AI provider info
  Future<String?> getAiProvider() async {
    if (!_initialized) await initialize();

    // TODO: Call Rust bridge
    // return await api.getAiProvider();

    return null;
  }
}

// Provider for Rust bridge repository
final rustBridgeRepositoryProvider = Provider<RustBridgeRepository>((ref) {
  return RustBridgeRepository();
});
