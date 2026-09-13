import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../theme/app_theme.dart';
import '../widgets/conversation_list.dart';
import '../widgets/message_area.dart';

class MainScreen extends ConsumerWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return Scaffold(
      backgroundColor: AppTheme.surface0,
      body: Row(
        children: [
          // Left: Conversation list
          const ConversationList(),

          // Right: Message area
          Expanded(
            child: Container(
              color: AppTheme.surface0,
              child: const MessageArea(),
            ),
          ),
        ],
      ),
    );
  }
}
