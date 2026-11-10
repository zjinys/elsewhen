import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../l10n/localization.dart';
import '../l10n/notice_messages.dart';
import '../sync/sync_dataset_cleanup_api.dart';
import 'app_error_notice.dart';

class DatasetCleanupCard extends StatefulWidget {
  const DatasetCleanupCard({super.key, required this.api, this.onChanged});
  final SyncDatasetCleanupApi api;
  final VoidCallback? onChanged;

  @override
  State<DatasetCleanupCard> createState() => _DatasetCleanupCardState();
}

class _DatasetCleanupCardState extends State<DatasetCleanupCard> {
  List<api.SyncDatasetCleanupDto> _entries = [];
  bool _busy = false;
  bool _confirming = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  Future<void> _refresh() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final entries = await widget.api.list();
      if (mounted) setState(() => _entries = entries);
    } catch (e) {
      if (mounted) {
        setState(() => _error = appFailure(e, key: 'datasetCleanupFailed'));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _remove(api.SyncDatasetCleanupDto preview) async {
    if (_busy) return;
    // Lock this card while the confirmation is open; a second dialog must not
    // reuse the same preview or race the native key deletion.
    setState(() {
      _busy = true;
      _confirming = true;
    });
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.datasetCleanupConfirmTitle),
        content: SingleChildScrollView(
          child: Text(
            '${preview.name}\n${preview.entryId}\n\n'
            '${preview.pending ? context.l10n.datasetCleanupPending : context.l10n.datasetCleanupSize(preview.bytes.toString())}\n\n'
            '${context.l10n.datasetCleanupWarning}',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l10n.datasetCleanupDelete),
          ),
        ],
      ),
    );
    if (!mounted) return;
    setState(() => _confirming = false);
    if (confirmed != true) {
      setState(() => _busy = false);
      return;
    }
    setState(() => _error = null);
    try {
      await widget.api.remove(preview);
    } catch (e) {
      if (mounted) {
        setState(() => _error = appFailure(e, key: 'datasetCleanupFailed'));
      }
    } finally {
      // Reload even on partial failure: pending cleanup remains visible and
      // retryable after the database was removed but the key store failed.
      if (mounted) {
        widget.onChanged?.call();
        setState(() => _busy = false);
        final error = _error;
        await _refresh();
        if (mounted && error != null) setState(() => _error = error);
      }
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Row(
        children: [
          Expanded(
            child: Text(
              context.l10n.datasetCleanupTitle,
              style: Theme.of(context).textTheme.titleSmall,
            ),
          ),
          IconButton(
            onPressed: _busy ? null : _refresh,
            tooltip: context.l10n.datasetCleanupRefresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      Text(context.l10n.datasetCleanupHelp),
      if (!_busy && _entries.isEmpty && _error == null)
        Text(context.l10n.datasetCleanupEmpty),
      for (final entry in _entries)
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  '${entry.name}\n${entry.pending ? context.l10n.datasetCleanupPending : context.l10n.datasetCleanupSize(entry.bytes.toString())}',
                ),
              ),
              TextButton(
                onPressed: _busy ? null : () => _remove(entry),
                child: Text(
                  entry.pending
                      ? context.l10n.datasetCleanupRetry
                      : context.l10n.datasetCleanupReview,
                ),
              ),
            ],
          ),
        ),
      if (_busy && !_confirming) const LinearProgressIndicator(),
      if (_error != null)
        AppErrorMessage(
          action: context.l10n.datasetCleanupTitle,
          error: _error!,
        ),
    ],
  );
}
