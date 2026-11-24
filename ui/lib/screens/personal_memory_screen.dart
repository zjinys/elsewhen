import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../l10n/localization.dart';
import '../widgets/app_error_notice.dart';

part 'personal_memory_parts/detail.dart';
part 'personal_memory_parts/review.dart';
part 'personal_memory_parts/pickers.dart';

Map<String, dynamic> _memoryMap(dynamic value) =>
    Map<String, dynamic>.from(value as Map);
List<Map<String, dynamic>> _memoryRows(dynamic value) =>
    (value as List? ?? const []).map(_memoryMap).toList();

String _memoryLabel(BuildContext context, String value) => switch (value) {
  'main' => context.l10n.memoryMain,
  'wiki' => context.l10n.memoryWiki,
  'goal' => context.l10n.memoryGoal,
  'pending' => context.l10n.memoryPending,
  'active' => context.l10n.memoryActive,
  'conflicted' => context.l10n.memoryConflicted,
  'superseded' => context.l10n.memorySuperseded,
  'dismissed' => context.l10n.memoryDismissed,
  'deleted' => context.l10n.memoryDeleted,
  'semantic' => context.l10n.memorySemantic,
  'procedural' => context.l10n.memoryProcedural,
  'reference' => context.l10n.memoryReference,
  'conversation' => context.l10n.memoryConversation,
  'personal' => context.l10n.memoryPersonal,
  'current' => context.l10n.memoryCurrent,
  'stale' => context.l10n.memoryStale,
  'unavailable' => context.l10n.memoryUnavailable,
  'unassessed' => context.l10n.memoryUnassessed,
  'supported' => context.l10n.memorySupported,
  'contradicted' => context.l10n.memoryContradicted,
  'insufficient' => context.l10n.memoryInsufficient,
  'low' => context.l10n.memoryLow,
  'normal' => context.l10n.memoryNormal,
  'high' => context.l10n.memoryHigh,
  'confirm' => context.l10n.memoryConfirm,
  'dismiss' => context.l10n.memoryDismiss,
  'delete' => context.l10n.delete,
  'restore' => context.l10n.memoryRestore,
  'conflict' => context.l10n.memoryConflict,
  'supersede' => context.l10n.memorySupersede,
  'pin' => context.l10n.memoryPin,
  'unpin' => context.l10n.memoryUnpin,
  'exclude' => context.l10n.memoryExclude,
  'include' => context.l10n.memoryInclude,
  'link_goal' => context.l10n.memoryLinkGoal,
  'unlink_goal' => context.l10n.memoryUnlinkGoal,
  'set_salience' => context.l10n.memorySalience,
  _ => value,
};

class PersonalMemoryScreen extends ConsumerStatefulWidget {
  final bool embedded;

  const PersonalMemoryScreen({super.key, this.embedded = false});

  @override
  ConsumerState<PersonalMemoryScreen> createState() =>
      _PersonalMemoryScreenState();
}

class _PersonalMemoryScreenState extends ConsumerState<PersonalMemoryScreen> {
  static const _pageSize = 30;
  final Map<String, dynamic> _filter = {};
  List<Map<String, dynamic>> _items = [];
  Map<String, dynamic>? _detail;
  String? _selectedId;
  Object? _listError, _detailError;
  int _offset = 0, _total = 0, _listRequest = 0, _detailRequest = 0;
  bool _loading = true, _detailLoading = false, _busy = false, _working = false;

  RustBridgeRepository get _repo =>
      ref.read(storageRepositoryProvider) as RustBridgeRepository;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    super.dispose();
  }

  Future<void> _load() async {
    final request = ++_listRequest;
    ++_detailRequest;
    setState(() {
      _loading = true;
      _listError = null;
      _detail = null;
      _selectedId = null;
      _detailError = null;
      _detailLoading = false;
    });
    try {
      final page = _memoryMap(
        jsonDecode(
          await _repo.listPersonalMemories(
            jsonEncode(_filter),
            offset: _offset,
            limit: _pageSize,
          ),
        ),
      );
      if (!mounted || request != _listRequest) return;
      final total = page['total'] as int;
      if (total > 0 && _offset >= total) {
        _offset = ((total - 1) ~/ _pageSize) * _pageSize;
        await _load();
        return;
      }
      if (total == 0) _offset = 0;
      final items = _memoryRows(page['items']);
      setState(() {
        _items = items;
        _total = total;
      });
    } catch (error) {
      if (mounted && request == _listRequest) {
        setState(() => _listError = error);
      }
    } finally {
      if (mounted && request == _listRequest) setState(() => _loading = false);
    }
  }

  Future<void> _select(String id) async {
    final request = ++_detailRequest;
    setState(() {
      _selectedId = id;
      _detail = null;
      _detailLoading = true;
      _detailError = null;
    });
    try {
      final detail = _memoryMap(
        jsonDecode(await _repo.getPersonalMemoryDetail(id)),
      );
      if (mounted && request == _detailRequest) {
        setState(() => _detail = detail);
      }
    } catch (error) {
      if (mounted && request == _detailRequest) {
        setState(() => _detailError = error);
      }
    } finally {
      if (mounted && request == _detailRequest) {
        setState(() => _detailLoading = false);
      }
    }
  }

  Widget _list() {
    if (_loading) return const Center(child: CircularProgressIndicator());
    if (_listError != null) return _error(_listError!, _load);
    return Column(
      children: [
        Expanded(
          child: _items.isEmpty
              ? Center(
                  child: Padding(
                    padding: const EdgeInsets.all(24),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [Text(context.l10n.memoryNoSaved)],
                    ),
                  ),
                )
              : ListView.separated(
                  padding: const EdgeInsets.all(16),
                  itemCount: _items.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 10),
                  itemBuilder: (context, index) {
                    final item = _items[index];
                    return Card(
                      margin: EdgeInsets.zero,
                      child: ListTile(
                        key: ValueKey('memory-row-${item['id']}'),
                        selected: _selectedId == item['id'],
                        onTap: _busy
                            ? null
                            : () => _select(item['id'] as String),
                        contentPadding: const EdgeInsets.fromLTRB(
                          16,
                          10,
                          12,
                          10,
                        ),
                        title: Text(
                          item['title'] as String,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle: Padding(
                          padding: const EdgeInsets.only(top: 6),
                          child: Text(
                            item['content'] as String? ??
                                context.l10n.memoryNoContent,
                            maxLines: 3,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        trailing: Icon(
                          Icons.chevron_right,
                          color: Theme.of(context).colorScheme.outline,
                        ),
                      ),
                    );
                  },
                ),
        ),
        const Divider(height: 1),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  '${_items.isEmpty ? 0 : _offset + 1}–${_offset + _items.length} / $_total ${context.l10n.memoryItems}',
                ),
              ),
              IconButton(
                tooltip: context.l10n.previousPage,
                onPressed: _busy || _offset == 0
                    ? null
                    : () {
                        _offset -= _pageSize;
                        _load();
                      },
                icon: const Icon(Icons.chevron_left),
              ),
              IconButton(
                tooltip: context.l10n.nextPage,
                onPressed: _busy || _offset + _pageSize >= _total
                    ? null
                    : () {
                        _offset += _pageSize;
                        _load();
                      },
                icon: const Icon(Icons.chevron_right),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _error(Object error, VoidCallback retry) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          AppErrorMessage(action: context.l10n.memoryManager, error: error),
          TextButton(onPressed: retry, child: Text(context.l10n.retry)),
        ],
      ),
    ),
  );

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, viewport) {
      return Scaffold(
        appBar: widget.embedded
            ? null
            : AppBar(
                title: Text(context.l10n.memoryManager),
                actions: [
                  IconButton(
                    tooltip: context.l10n.refresh,
                    onPressed: _busy
                        ? null
                        : () {
                            _load();
                          },
                    icon: const Icon(Icons.refresh),
                  ),
                ],
              ),
        body: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (_working) const LinearProgressIndicator(minHeight: 2),
            if (_selectedId == null)
              Padding(
                padding: const EdgeInsets.all(16),
                child: TextField(
                  decoration: InputDecoration(prefixIcon: const Icon(Icons.search)),
                  onChanged: (value) { _filter['query'] = value.trim(); _offset = 0; _load(); },
                ),
              ),
            Expanded(
              child: LayoutBuilder(
                builder: (context, constraints) {
                  final wide = constraints.maxWidth >= 860;
                  final detailOpen = _selectedId != null;
                  if (wide) {
                    return detailOpen ? _detailPane(true) : _list();
                  }
                  return PopScope(
                    canPop: !detailOpen || _busy,
                    onPopInvokedWithResult: (didPop, result) {
                      if (!didPop && !_busy) _closeDetail();
                    },
                    child: detailOpen ? _detailPane(true) : _list(),
                  );
                },
              ),
            ),
          ],
        ),
      );
    },
  );

  void _closeDetail() {
    ++_detailRequest;
    setState(() {
      _selectedId = null;
      _detail = null;
      _detailLoading = false;
    });
  }
}
