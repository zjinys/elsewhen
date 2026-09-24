import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nativeapi/nativeapi.dart';

import 'screens/conversation_timeline_screen.dart';
import 'bridge/rust_bridge_repository.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Configure frameless window（nativeapi：窗口由 Flutter runner 创建，直接取当前窗口配置）
  final window = WindowManager.instance.getCurrent();
  window?.titleBarStyle = TitleBarStyle.hidden;
  window?.minimumSize = const Size(800, 600);
  window?.setSize(const Size(1200, 800), false);
  window?.center();

  // Initialize Rust bridge
  final repository = RustBridgeRepository();
  await repository.initialize();

  runApp(
    ProviderScope(
      overrides: [storageRepositoryProvider.overrideWithValue(repository)],
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
        cardTheme: const CardThemeData(elevation: 2),
      ),
      home: const ConversationTimelineScreen(),
      debugShowCheckedModeBanner: false,
    );
  }
}
