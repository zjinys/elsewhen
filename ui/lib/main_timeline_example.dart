import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:window_manager/window_manager.dart';
import 'screens/conversation_timeline_screen.dart';
import 'bridge/rust_bridge_repository.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Configure frameless window
  await windowManager.ensureInitialized();
  await windowManager.setTitleBarStyle(TitleBarStyle.hidden);
  await windowManager.setMinimumSize(const Size(800, 600));
  await windowManager.setSize(const Size(1200, 800));
  await windowManager.center();

  // Initialize Rust bridge
  final repository = RustBridgeRepository();
  await repository.initialize();

  runApp(
    ProviderScope(
      overrides: [
        storageRepositoryProvider.overrideWithValue(repository),
      ],
      child: const MyApp(),
    ),
  );
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Elsewhen - 时间线对话',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.deepPurple),
        useMaterial3: true,
        cardTheme: const CardThemeData(
          elevation: 2,
        ),
      ),
      home: const ConversationTimelineScreen(),
      debugShowCheckedModeBanner: false,
    );
  }
}
