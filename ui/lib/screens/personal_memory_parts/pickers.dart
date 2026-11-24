part of '../personal_memory_screen.dart';

class _MemoryGoalDialog extends StatefulWidget {
  const _MemoryGoalDialog({required this.repo});
  final RustBridgeRepository repo;

  @override
  State<_MemoryGoalDialog> createState() => _MemoryGoalDialogState();
}

class _MemoryGoalDialogState extends State<_MemoryGoalDialog> {
  late Future<List<api.GoalDto>> _goals;
  final _reason = TextEditingController();
  String? _selected;

  @override
  void initState() {
    super.initState();
    _goals = widget.repo.listMemoryLinkGoals();
  }

  @override
  void dispose() {
    _reason.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(context.l10n.memoryLinkGoal),
    content: SizedBox(
      width: 480,
      child: FutureBuilder<List<api.GoalDto>>(
        future: _goals,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                AppErrorMessage(
                  action: context.l10n.memoryLinkGoal,
                  error: snapshot.error!,
                ),
                TextButton(
                  onPressed: () => setState(
                    () => _goals = widget.repo.listMemoryLinkGoals(),
                  ),
                  child: Text(context.l10n.retry),
                ),
              ],
            );
          }
          if (!snapshot.hasData) {
            return const Center(
              heightFactor: 2,
              child: CircularProgressIndicator(),
            );
          }
          final goals = snapshot.data!;
          if (goals.isEmpty) return Text(context.l10n.memoryNoGoals);
          return SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                DropdownButtonFormField<String>(
                  isExpanded: true,
                  initialValue: _selected,
                  decoration: InputDecoration(
                    labelText: context.l10n.memoryLinkGoal,
                  ),
                  items: [
                    for (final goal in goals)
                      DropdownMenuItem(
                        value: goal.id,
                        child: Text(
                          goal.content,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                  ],
                  onChanged: (value) => setState(() => _selected = value),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: _reason,
                  minLines: 2,
                  maxLines: 4,
                  maxLength: 600,
                  decoration: InputDecoration(
                    labelText: context.l10n.memoryGoalReason,
                  ),
                  onChanged: (_) => setState(() {}),
                ),
              ],
            ),
          );
        },
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(context.l10n.cancel),
      ),
      FilledButton(
        onPressed: _selected == null || _reason.text.trim().isEmpty
            ? null
            : () => Navigator.pop(context, <String, dynamic>{
                'operation': 'link_goal',
                'goal_id': _selected,
                'reason': _reason.text.trim(),
              }),
        child: Text(context.l10n.confirm),
      ),
    ],
  );
}

class _MemoryReplacementDialog extends StatefulWidget {
  const _MemoryReplacementDialog({required this.repo, required this.claim});
  final RustBridgeRepository repo;
  final Map<String, dynamic> claim;

  @override
  State<_MemoryReplacementDialog> createState() =>
      _MemoryReplacementDialogState();
}

class _MemoryReplacementDialogState extends State<_MemoryReplacementDialog> {
  final _search = TextEditingController();
  Timer? _debounce;
  List<Map<String, dynamic>> _items = [];
  int _offset = 0, _total = 0, _request = 0;
  bool _loading = true;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _search.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final request = ++_request;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final page = _memoryMap(
        jsonDecode(
          await widget.repo.listPersonalMemories(
            jsonEncode({
              'query': _search.text.trim(),
              for (final key in [
                'conversation_id',
                'subject',
                'predicate',
                'kind',
              ])
                key: widget.claim[key],
            }),
            offset: _offset,
            limit: 20,
          ),
        ),
      );
      if (!mounted || request != _request) return;
      setState(() {
        _total = page['total'] as int;
        _items = _memoryRows(page['items'])
            .where(
              (item) =>
                  item['id'] != widget.claim['id'] &&
                  !['deleted', 'dismissed'].contains(item['state']) &&
                  item['validity'] == 'current',
            )
            .toList();
      });
    } catch (error) {
      if (mounted && request == _request) setState(() => _error = error);
    } finally {
      if (mounted && request == _request) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(context.l10n.memorySupersede),
    content: SizedBox(
      width: 520,
      height: 420,
      child: Column(
        children: [
          Text(context.l10n.memoryChooseReplacement),
          const SizedBox(height: 12),
          TextField(
            controller: _search,
            maxLength: 300,
            decoration: InputDecoration(
              hintText: context.l10n.memorySearchHint,
              counterText: '',
            ),
            onChanged: (_) {
              _debounce?.cancel();
              _debounce = Timer(const Duration(milliseconds: 300), () {
                _offset = 0;
                _load();
              });
            },
          ),
          Expanded(
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : _error != null
                ? Center(
                    child: AppErrorMessage(
                      action: context.l10n.memoryManager,
                      error: _error!,
                    ),
                  )
                : _items.isEmpty
                ? Center(child: Text(context.l10n.memoryNoReplacement))
                : ListView(
                    children: [
                      for (final item in _items)
                        ListTile(
                          title: Text(item['title'] as String),
                          subtitle: Text(
                            '${context.l10n.memoryVersion} ${item['version']}',
                          ),
                          onTap: () =>
                              Navigator.pop(context, item['id'] as String),
                        ),
                    ],
                  ),
          ),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              IconButton(
                tooltip: context.l10n.previousPage,
                onPressed: _loading || _offset == 0
                    ? null
                    : () {
                        _offset -= 20;
                        _load();
                      },
                icon: const Icon(Icons.chevron_left),
              ),
              IconButton(
                tooltip: context.l10n.nextPage,
                onPressed: _loading || _offset + 20 >= _total
                    ? null
                    : () {
                        _offset += 20;
                        _load();
                      },
                icon: const Icon(Icons.chevron_right),
              ),
            ],
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(context.l10n.cancel),
      ),
    ],
  );
}
