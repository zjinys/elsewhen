import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../theme/app_theme.dart';

/// 情感回访横幅（决策辅助 D6）：「被记得」的轻量体验。
///
/// 红线：
/// - 只在主界面挂载（打开应用）时检查一次；绝不注册系统级推送、
///   不做任何后台唤醒或通知通道；
/// - 展示内容是一周前后的「被记住的事」原文摘录与当时的感受，只呈现，
///   不评价不追问（陪伴 ≠ 分析报告）；
/// - 频率与开关由 companion_settings 把握（展示即记账，间隔内不再打扰）；
/// - 点「知道了」关闭后当次会话不再出现；读取失败静默收缩（fail-safe）。
class CompanionRevisitHint extends ConsumerStatefulWidget {
  const CompanionRevisitHint({super.key});

  @override
  ConsumerState<CompanionRevisitHint> createState() =>
      _CompanionRevisitHintState();
}

class _CompanionRevisitHintState extends ConsumerState<CompanionRevisitHint> {
  api.CompanionRevisitDto? _item;
  bool _dismissed = false;
  bool _checked = false;

  @override
  void initState() {
    super.initState();
    // 桌面端也走 main screen 挂载路径；移到下一帧避免 build 阻塞。
    WidgetsBinding.instance.addPostFrameCallback((_) => _check());
  }

  Future<void> _check() async {
    // 回访只在用户设备本地随时钟发生；测试/CI（无配置）失败静默收缩。
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final item = await repo.checkCompanionRevisit();
      if (!mounted) return;
      setState(() {
        _item = item;
        _checked = true;
      });
    } catch (e) {
      debugPrint('checkCompanionRevisit failed: $e');
      if (!mounted) return;
      setState(() => _checked = true);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_dismissed || _checked && _item == null) {
      return const SizedBox.shrink();
    }
    final item = _item;
    if (item == null) return const SizedBox.shrink();

    final recordedAt = DateTime.tryParse(item.recordedAt)?.toLocal();
    final when = recordedAt == null
        ? ''
        : '${DateFormat('M月d日').format(recordedAt)} ';
    final feeling = item.feeling == null ? '' : '，${item.feeling}';
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(
        AppTheme.space4,
        AppTheme.space3,
        AppTheme.space4,
        0,
      ),
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space4,
        vertical: AppTheme.space2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Row(
        children: [
          Icon(
            Icons.auto_stories_outlined,
            size: 18,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: AppTheme.space3),
          Expanded(
            child: Text(
              '$when你记录过：「${item.excerpt}」$feeling——还记得这件事。',
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: AppTheme.textSecondary, fontSize: 12.5),
            ),
          ),
          IconButton(
            icon: const Icon(Icons.close, size: 16),
            color: AppTheme.textTertiary,
            tooltip: '知道了',
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
            onPressed: () => setState(() => _dismissed = true),
          ),
        ],
      ),
    );
  }
}
