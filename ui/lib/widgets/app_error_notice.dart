import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/localization.dart';
import '../l10n/notice_messages.dart';

/// Keep the ordinary flow readable and retain explicit access to diagnostics.
void showAppError(BuildContext context, String action, Object error) {
  final message = context.errorMessage(action, error);
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text(message),
      action: SnackBarAction(
        label: context.l10n.errorDetails,
        onPressed: () {
          if (!context.mounted) return;
          showAppErrorDetails(context, error);
        },
      ),
    ),
  );
}

void showAppErrorDetails(BuildContext context, Object error, {String? title}) =>
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title ?? dialogContext.l10n.errorDetails),
        content: SizedBox(
          width: 560,
          child: SingleChildScrollView(
            child: SelectableText(noticeDiagnostic(error).toString()),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Clipboard.setData(
              ClipboardData(text: noticeDiagnostic(error).toString()),
            ),
            child: Text(dialogContext.l10n.copy),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text(dialogContext.l10n.close),
          ),
        ],
      ),
    );

class AppErrorMessage extends StatelessWidget {
  const AppErrorMessage({
    super.key,
    required this.action,
    required this.error,
    this.style,
    this.message,
    this.detailsLabel,
  });
  final String action;
  final Object error;
  final TextStyle? style;
  final String? message;
  final String? detailsLabel;

  @override
  Widget build(BuildContext context) => Semantics(
    liveRegion: true,
    child: Wrap(
      spacing: 8,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        Text(
          message ?? context.errorMessage(action, error),
          style: style ?? TextStyle(color: Theme.of(context).colorScheme.error),
        ),
        TextButton(
          onPressed: () => showAppErrorDetails(context, error),
          child: Text(detailsLabel ?? context.l10n.errorDetails),
        ),
      ],
    ),
  );
}

/// Background notes retain the original receipt/diagnostic. A localized status
/// stays visible, and the full original note is available without truncation.
class ProcessingNotes extends StatelessWidget {
  const ProcessingNotes({super.key, required this.note, this.status});
  final String note;
  final String? status;
  @override
  Widget build(BuildContext context) => Wrap(
    crossAxisAlignment: WrapCrossAlignment.center,
    children: [
      if (status != null) Text(status!),
      TextButton(
        onPressed: () => showAppErrorDetails(
          context,
          note,
          title: context.l10n.processingNotes,
        ),
        child: Text(context.l10n.processingNotes),
      ),
    ],
  );
}
