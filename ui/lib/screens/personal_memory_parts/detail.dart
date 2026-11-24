part of '../personal_memory_screen.dart';

// ignore_for_file: invalid_use_of_protected_member

extension _MemoryDetail on _PersonalMemoryScreenState {
  Widget _detailPane(bool compact) {
    final detail = _detail;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (compact)
          Align(
            alignment: Alignment.centerLeft,
            child: TextButton.icon(
              onPressed: _busy ? null : _closeDetail,
              icon: const Icon(Icons.arrow_back),
              label: Text(context.l10n.memoryBackList),
            ),
          ),
        Expanded(
          child: _detailLoading
              ? const Center(child: CircularProgressIndicator())
              : _detailError != null
              ? _error(_detailError!, () => _select(_selectedId!))
              : detail == null
              ? Center(child: Text(context.l10n.memorySelect))
              : _detailContent(detail),
        ),
      ],
    );
  }

  Widget _detailContent(Map<String, dynamic> detail) {
    if (detail['kind'] == 'profile') return _profileContent(detail);
    final claim = _memoryMap(detail['claim']);
    final inactive = ['deleted', 'dismissed'].contains(claim['state']);
    final enabled = !_busy && detail['can_review'] == true;
    return SingleChildScrollView(
      key: ValueKey('memory-detail-${claim['id']}'),
      padding: const EdgeInsets.all(24),
      child: SelectionArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              detail['title'] as String,
              style: Theme.of(context).textTheme.headlineSmall,
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              children: [
                for (final value in [
                  claim['state'],
                  detail['validity'],
                ])
                  Chip(label: Text(_memoryLabel(context, value as String))),
                if (claim['excluded'] == true)
                  Chip(label: Text(context.l10n.memoryExcluded)),
                if (claim['pinned'] == true)
                  Chip(label: Text(context.l10n.memoryPin)),
              ],
            ),
            const SizedBox(height: 12),
            Text(_memoryLabel(context, claim['scope'] as String)),
            if (claim['valid_from'] != null) Text('≥ ${claim['valid_from']}'),
            if (claim['valid_until'] != null) Text('< ${claim['valid_until']}'),
            if (detail['can_review'] != true)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Text(context.l10n.memoryOwnerUnavailable),
              ),
            const SizedBox(height: 16),
            Text(detail['content'] as String? ?? context.l10n.memoryNoContent),
            if (claim['content'] != null)
              OutlinedButton.icon(
                onPressed: _busy
                    ? null
                    : () => _editMemory(
                        claim,
                        detail['content'] as String? ?? '',
                      ),
                icon: const Icon(Icons.edit_outlined),
                label: Text(context.l10n.edit),
              ),
            if (detail['content'] != null && detail['complete'] != true)
              Text(context.l10n.memoryTruncated),
            const SizedBox(height: 20),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (inactive) ...[
                  OutlinedButton(
                    onPressed: enabled
                        ? () => _change({'operation': 'restore'})
                        : null,
                    child: Text(context.l10n.memoryRestore),
                  ),
                  if (claim['state'] == 'dismissed')
                    OutlinedButton(
                      onPressed: enabled
                          ? () => _change({'operation': 'delete'})
                          : null,
                      child: Text(context.l10n.delete),
                    ),
                ] else ...[
                  OutlinedButton(
                    onPressed: enabled ? () => _confirmMemory(claim) : null,
                    child: Text(context.l10n.memoryConfirm),
                  ),
                  OutlinedButton(
                    onPressed: enabled
                        ? () => _change({
                            'operation': claim['pinned'] == true
                                ? 'unpin'
                                : 'pin',
                          })
                        : null,
                    child: Text(
                      claim['pinned'] == true
                          ? context.l10n.memoryUnpin
                          : context.l10n.memoryPin,
                    ),
                  ),
                  OutlinedButton(
                    onPressed: enabled
                        ? () => _change({
                            'operation': claim['excluded'] == true
                                ? 'include'
                                : 'exclude',
                          })
                        : null,
                    child: Text(
                      claim['excluded'] == true
                          ? context.l10n.memoryInclude
                          : context.l10n.memoryExclude,
                    ),
                  ),
                  PopupMenuButton<String>(
                    enabled: enabled,
                    tooltip: context.l10n.memorySalience,
                    onSelected: (value) => _change({
                      'operation': 'set_salience',
                      'salience': value,
                    }),
                    itemBuilder: (_) => [
                      for (final value in ['low', 'normal', 'high'])
                        CheckedPopupMenuItem(
                          value: value,
                          checked: claim['salience'] == value,
                          enabled: claim['salience'] != value,
                          child: Text(_memoryLabel(context, value)),
                        ),
                    ],
                    child: Padding(
                      padding: const EdgeInsets.all(12),
                      child: Text(
                        '${context.l10n.memorySalience}: ${_memoryLabel(context, claim['salience'] as String)}',
                      ),
                    ),
                  ),
                  PopupMenuButton<String>(
                    enabled: enabled,
                    tooltip: context.l10n.memoryMore,
                    onSelected: (value) async {
                      if (value == 'supersede') {
                        await _replaceMemory(claim);
                      } else {
                        await _change({'operation': value});
                      }
                    },
                    itemBuilder: (_) => [
                      for (final value in [
                        'conflict',
                        'supersede',
                        'dismiss',
                        'delete',
                      ])
                        PopupMenuItem(
                          value: value,
                          child: Text(_memoryLabel(context, value)),
                        ),
                    ],
                    icon: const Icon(Icons.more_horiz),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _editMemory(Map<String, dynamic> claim, String initial) async {
    var edited = initial;
    try {
      final saved = await showDialog<bool>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(context.l10n.edit),
          content: TextFormField(
            initialValue: initial,
            onChanged: (value) => edited = value,
            maxLines: 8,
            maxLength: 6000,
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, false),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, true),
              child: Text(context.l10n.save),
            ),
          ],
        ),
      );
      if (saved != true || !mounted) return;
      setState(() => _busy = true);
      await _repo.editPersonalMemory(
        claim['id'] as String,
        claim['version'] as int,
        edited,
      );
      await _load();
      await _select(claim['id'] as String);
    } catch (error) {
      if (mounted) showAppError(context, context.l10n.memoryManager, error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Widget _profileContent(Map<String, dynamic> detail) => SingleChildScrollView(
    padding: const EdgeInsets.all(24),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          detail['title'] as String,
          style: Theme.of(context).textTheme.headlineSmall,
        ),
        const SizedBox(height: 20),
        SelectableText(
          (detail['content'] as String?)?.trim().isNotEmpty == true
              ? detail['content'] as String
              : context.l10n.memoryNoContent,
          style: Theme.of(context).textTheme.bodyLarge,
        ),
        const SizedBox(height: 24),
        FilledButton.icon(
          onPressed: _busy ? null : _editProfile,
          icon: const Icon(Icons.edit_outlined),
          label: Text(context.l10n.edit),
        ),
      ],
    ),
  );

  Future<void> _editProfile() async {
    final profile = await _repo.getPersonalMemoryProfile();
    if (!mounted) return;
    final fields = <String, TextEditingController>{
      '技能': TextEditingController(text: profile.skills),
      '资源': TextEditingController(text: profile.resources),
      '限制': TextEditingController(text: profile.limits),
      '偏好': TextEditingController(text: profile.preferences),
    };
    try {
      final saved = await showDialog<bool>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(context.l10n.edit),
          content: SizedBox(
            width: 560,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final entry in fields.entries)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 12),
                      child: TextField(
                        controller: entry.value,
                        maxLines: 4,
                        maxLength: 1500,
                        decoration: InputDecoration(labelText: entry.key),
                      ),
                    ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, false),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogContext, true),
              child: Text(context.l10n.save),
            ),
          ],
        ),
      );
      if (saved != true || !mounted) return;
      setState(() {
        _busy = true;
        _working = true;
      });
      await _repo.savePersonalMemoryProfile(
        api.PlanningProfileDto(
          skills: fields['技能']!.text.trim(),
          resources: fields['资源']!.text.trim(),
          limits: fields['限制']!.text.trim(),
          preferences: fields['偏好']!.text.trim(),
          updatedAt: profile.updatedAt,
        ),
      );
      await _load();
      await _select('profile/me');
    } catch (error) {
      if (mounted) showAppError(context, context.l10n.memoryManager, error);
    } finally {
      for (final controller in fields.values) {
        controller.dispose();
      }
      if (mounted) {
        setState(() {
          _busy = false;
          _working = false;
        });
      }
    }
  }
}
