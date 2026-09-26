import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../theme/app_theme.dart';
import '../models/event.dart';

class EventCard extends StatelessWidget {
  final Event event;

  const EventCard({super.key, required this.event});

  @override
  Widget build(BuildContext context) {
    return Container(
      margin: const EdgeInsets.only(bottom: AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(
          color: AppTheme.surface3.withValues(alpha: 0.3),
          width: 1,
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.all(AppTheme.space4),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _buildHeader(),
            const SizedBox(height: AppTheme.space3),
            _buildContent(),
            if (event.analysis != null) ...[
              const SizedBox(height: AppTheme.space3),
              _buildAnalysis(),
            ],
          ],
        ),
      ),
    );
  }

  Widget _buildHeader() {
    final timeFormat = DateFormat('HH:mm');
    final dateFormat = DateFormat('yyyy-MM-dd');

    return Row(
      children: [
        Container(
          padding: const EdgeInsets.symmetric(
            horizontal: AppTheme.space2,
            vertical: AppTheme.space1,
          ),
          decoration: BoxDecoration(
            color: AppTheme.surface3,
            borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(Icons.access_time, size: 12, color: AppTheme.textTertiary),
              const SizedBox(width: 4),
              Text(
                timeFormat.format(event.recordedAt),
                style: TextStyle(
                  color: AppTheme.textTertiary,
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(width: AppTheme.space2),
        Text(
          dateFormat.format(event.recordedAt),
          style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
        ),
        const Spacer(),
        _buildSourceBadge(),
      ],
    );
  }

  Widget _buildSourceBadge() {
    IconData icon;
    String label;

    switch (event.source) {
      case 'flutter_gui':
        icon = Icons.phone_android;
        label = 'GUI';
        break;
      case 'cli':
        icon = Icons.terminal;
        label = 'CLI';
        break;
      case 'hotkey':
        icon = Icons.keyboard;
        label = 'Hotkey';
        break;
      default:
        icon = Icons.device_unknown;
        label = event.source;
    }

    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space2,
        vertical: AppTheme.space1,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface3.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 11, color: AppTheme.textTertiary),
          const SizedBox(width: 4),
          Text(
            label,
            style: TextStyle(
              color: AppTheme.textTertiary,
              fontSize: 10,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildContent() {
    return Text(
      event.rawText,
      style: TextStyle(color: AppTheme.textPrimary, fontSize: 15, height: 1.6),
    );
  }

  Widget _buildAnalysis() {
    final analysis = event.analysis!;

    return Container(
      padding: const EdgeInsets.all(AppTheme.space3),
      decoration: BoxDecoration(
        color: AppTheme.surface3.withValues(alpha: 0.3),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        border: Border.all(
          color: AppTheme.accentMuted.withValues(alpha: 0.2),
          width: 1,
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.auto_awesome, size: 14, color: AppTheme.accentPrimary),
              const SizedBox(width: AppTheme.space1),
              Text(
                'AI 分析',
                style: TextStyle(
                  color: AppTheme.accentPrimary,
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
          const SizedBox(height: AppTheme.space2),
          Text(
            analysis.summary,
            style: TextStyle(
              color: AppTheme.textSecondary,
              fontSize: 13,
              height: 1.5,
            ),
          ),
          if (analysis.tags != null && analysis.tags!.isNotEmpty) ...[
            const SizedBox(height: AppTheme.space2),
            Wrap(
              spacing: AppTheme.space1,
              runSpacing: AppTheme.space1,
              children: analysis.tags!.map((tag) {
                return Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppTheme.space2,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: AppTheme.accentMuted.withValues(alpha: 0.2),
                    borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
                  ),
                  child: Text(
                    tag,
                    style: TextStyle(
                      color: AppTheme.accentPrimary,
                      fontSize: 11,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                );
              }).toList(),
            ),
          ],
        ],
      ),
    );
  }
}
