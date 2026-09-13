import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../theme/app_theme.dart';
import '../models/event.dart';
import '../providers/event_provider.dart';
import '../widgets/event_card.dart';
import '../widgets/event_input.dart';

class MainScreen extends ConsumerWidget {
  const MainScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final eventsAsync = ref.watch(eventsProvider);

    return Scaffold(
      body: Column(
        children: [
          _buildHeader(context),
          Expanded(
            child: eventsAsync.when(
              data: (events) => _buildEventList(events),
              loading: () => const Center(
                child: CircularProgressIndicator(),
              ),
              error: (error, stack) => Center(
                child: Text(
                  'Error: $error',
                  style: TextStyle(color: AppTheme.error),
                ),
              ),
            ),
          ),
          _buildInputArea(ref),
        ],
      ),
    );
  }

  Widget _buildHeader(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space6),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          bottom: BorderSide(
            color: AppTheme.surface3.withValues(alpha: 0.5),
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          Icon(
            Icons.schedule,
            color: AppTheme.accentPrimary,
            size: 28,
          ),
          const SizedBox(width: AppTheme.space3),
          Text(
            'Elsewhen',
            style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                  color: AppTheme.textPrimary,
                  fontWeight: FontWeight.w600,
                ),
          ),
          const Spacer(),
          IconButton(
            icon: const Icon(Icons.settings_outlined),
            color: AppTheme.textSecondary,
            onPressed: () {
              // TODO: Open settings
            },
          ),
        ],
      ),
    );
  }

  Widget _buildEventList(List<Event> events) {
    if (events.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.event_note_outlined,
              size: 64,
              color: AppTheme.textTertiary,
            ),
            const SizedBox(height: AppTheme.space4),
            Text(
              '还没有记录任何事件',
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 16,
              ),
            ),
            const SizedBox(height: AppTheme.space2),
            Text(
              '在下方输入框开始记录',
              style: TextStyle(
                color: AppTheme.textTertiary,
                fontSize: 14,
              ),
            ),
          ],
        ),
      );
    }

    return ListView.builder(
      padding: const EdgeInsets.all(AppTheme.space4),
      itemCount: events.length,
      itemBuilder: (context, index) {
        return EventCard(event: events[index]);
      },
    );
  }

  Widget _buildInputArea(WidgetRef ref) {
    return Container(
      padding: const EdgeInsets.all(AppTheme.space4),
      decoration: BoxDecoration(
        color: AppTheme.surface1,
        border: Border(
          top: BorderSide(
            color: AppTheme.surface3.withValues(alpha: 0.5),
            width: 1,
          ),
        ),
      ),
      child: EventInput(
        onSubmit: (text) async {
          if (text.trim().isEmpty) return;

          final repo = ref.read(eventRepositoryProvider);
          await repo.createEvent(text);
          ref.invalidate(eventsProvider);
        },
      ),
    );
  }
}
