import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;

import '../sync/sync_dataset_switch_api.dart';

/// 数据集切换引导：原生 catalog（sync-workspace.db）里出现 ready 且未选中的
/// 数据集时，提示用户「进入数据集」。选择只写 catalog 的 selected_id，
/// 当前进程 pin 着的主库连接不会移动——必须完全退出并重新打开应用才生效。
///
/// 挂两处：加入流程完成卡（joiner pairing 完成，最直观的时机）与同步设置页
/// 顶部（重启后配对会话已清空，不依赖 pairing 状态也能发现待切换数据集）。
class DatasetSwitchCard extends StatefulWidget {
  const DatasetSwitchCard({
    super.key,
    this.api = const FrbSyncDatasetSwitchApi(),
    this.onSwitched,
    this.catalogChanges,
  });

  final SyncDatasetSwitchApi api;
  final void Function(String datasetName)? onSwitched;

  /// 目录可能被同屏的其它动作改写（加入控制器删除暂存项、完成加入等），
  /// 它们会 notifyListeners；监听后本卡立即重查，删除的暂存项不用重进
  /// 设置页才消失。
  final Listenable? catalogChanges;

  @override
  State<DatasetSwitchCard> createState() => _DatasetSwitchCardState();
}

class _DatasetSwitchCardState extends State<DatasetSwitchCard> {
  final _pending = <api.SyncDatasetEntryDto>[];
  bool _checking = false;
  bool _switching = false;
  String? _switchedName;
  String? _error;

  @override
  void initState() {
    super.initState();
    widget.catalogChanges?.addListener(_onCatalogChanged);
    _check();
  }

  @override
  void didUpdateWidget(covariant DatasetSwitchCard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.catalogChanges != oldWidget.catalogChanges) {
      oldWidget.catalogChanges?.removeListener(_onCatalogChanged);
      widget.catalogChanges?.addListener(_onCatalogChanged);
    }
  }

  @override
  void dispose() {
    widget.catalogChanges?.removeListener(_onCatalogChanged);
    super.dispose();
  }

  void _onCatalogChanged() => _check();

  /// ready 条目暂缺（快照应用与 catalog 发布有先后）不钉住失败结果：
  /// 配对流程的 2s 轮询/状态头刷新会带着本卡重建重查。
  Future<void> _check() async {
    if (_checking || _switching || _switchedName != null) return;
    _checking = true;
    try {
      final ready = await widget.api.listUnselected();
      if (!mounted || _switchedName != null) return;
      setState(() {
        _pending
          ..clear()
          ..addAll(ready);
      });
    } catch (error) {
      if (!mounted || _switchedName != null) return;
      setState(() => _error = '查询数据集失败：$error');
    } finally {
      _checking = false;
    }
  }

  Future<void> _select(api.SyncDatasetEntryDto entry) async {
    if (_switching || _switchedName != null) return;
    setState(() {
      _switching = true;
      _error = null;
    });
    try {
      await widget.api.select(entry.entryId);
      if (!mounted) return;
      setState(() {
        _switching = false;
        _pending.removeWhere((e) => e.entryId == entry.entryId);
        _switchedName = entry.name;
      });
      widget.onSwitched?.call(entry.name);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _switching = false;
        _error = '切换失败（需先解锁本机同步密钥）：$error';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (_switchedName != null) {
      return Row(
        children: [
          Icon(Icons.task_alt, size: 18, color: Colors.green.shade700),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              '已切换到「$_switchedName」。请完全退出并重新打开应用，即可看到同步的数据。',
              style: theme.textTheme.bodySmall,
            ),
          ),
        ],
      );
    }
    if (_pending.isEmpty && _error == null) {
      return const SizedBox.shrink();
    }
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest.withValues(alpha: 0.4),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: theme.colorScheme.outlineVariant),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                Icons.folder_open,
                size: 18,
                color: theme.colorScheme.primary,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  _pending.length == 1
                      ? '已加入数据集「${_pending.first.name}」，其数据尚未在此窗口生效。'
                      : '发现 ${_pending.length} 个待进入的数据集',
                  style: theme.textTheme.bodySmall?.copyWith(
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          for (final entry in _pending)
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      _pending.length == 1 ? '切换后重启应用即可看到全部同步数据。' : entry.name,
                      style: theme.textTheme.bodySmall,
                    ),
                  ),
                  FilledButton.tonalIcon(
                    onPressed: _switching ? null : () => _select(entry),
                    icon: const Icon(Icons.folder_open, size: 16),
                    label: const Text('进入数据集'),
                  ),
                ],
              ),
            ),
          if (_error != null)
            Text(
              _error!,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
        ],
      ),
    );
  }
}
