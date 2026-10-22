part of '../settings_screen.dart';

//! 设置 → 习惯提醒：集中查看/维护所有习惯页的定期问候。

extension _HabitsTab on _SettingsScreenState {
  Widget _buildHabitsTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: '习惯提醒',
            icon: Icons.notifications_active_outlined,
            child: const HabitNudgeSettingsContent(),
          ),
        ],
      ),
    );
  }
}
