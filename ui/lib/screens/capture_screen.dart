import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../theme/app_theme.dart';
import '../providers/event_provider.dart';

class CaptureScreen extends ConsumerStatefulWidget {
  const CaptureScreen({super.key});

  @override
  ConsumerState<CaptureScreen> createState() => _CaptureScreenState();
}

class _CaptureScreenState extends ConsumerState<CaptureScreen> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  bool _isSubmitting = false;

  @override
  void initState() {
    super.initState();
    // Auto-focus when window opens
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _focusNode.requestFocus();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _submitEvent() async {
    if (_isSubmitting || _controller.text.trim().isEmpty) return;

    setState(() => _isSubmitting = true);

    try {
      final repo = ref.read(eventRepositoryProvider);
      await repo.createEvent(_controller.text);
      _controller.clear();

      if (mounted) {
        // Show brief success feedback
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: const Text('事件已记录'),
            duration: const Duration(seconds: 1),
            backgroundColor: AppTheme.success,
          ),
        );
      }
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }

  void _closeWindow() {
    // TODO: Properly hide window instead of closing app
    SystemNavigator.pop();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.surface1,
      body: KeyboardListener(
        focusNode: FocusNode(),
        onKeyEvent: (event) {
          if (event is KeyDownEvent) {
            if (event.logicalKey == LogicalKeyboardKey.escape) {
              _closeWindow();
            } else if (event.logicalKey == LogicalKeyboardKey.enter &&
                !HardwareKeyboard.instance.isShiftPressed) {
              _submitEvent();
            }
          }
        },
        child: Container(
          padding: const EdgeInsets.all(AppTheme.space4),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _buildHeader(),
              const SizedBox(height: AppTheme.space3),
              _buildInput(),
              const SizedBox(height: AppTheme.space3),
              _buildActions(),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildHeader() {
    return Row(
      children: [
        Container(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: AppTheme.surface2,
            borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          ),
          child: Icon(
            Icons.edit_note,
            color: AppTheme.accentPrimary,
            size: 18,
          ),
        ),
        const SizedBox(width: AppTheme.space2),
        Text(
          '快速记录',
          style: Theme.of(context).textTheme.titleMedium?.copyWith(
                color: AppTheme.textPrimary,
                fontWeight: FontWeight.w600,
              ),
        ),
        const Spacer(),
        IconButton(
          icon: const Icon(Icons.close, size: 18),
          color: AppTheme.textTertiary,
          onPressed: _closeWindow,
          padding: EdgeInsets.zero,
          constraints: const BoxConstraints(
            minWidth: 32,
            minHeight: 32,
          ),
        ),
      ],
    );
  }

  Widget _buildInput() {
    return Container(
      constraints: const BoxConstraints(
        minHeight: 80,
        maxHeight: 200,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
      ),
      child: TextField(
        controller: _controller,
        focusNode: _focusNode,
        maxLines: null,
        style: const TextStyle(
          color: AppTheme.textPrimary,
          fontSize: 15,
          height: 1.6,
        ),
        decoration: InputDecoration(
          hintText: '记录此刻发生的事情...',
          hintStyle: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 15,
          ),
          border: InputBorder.none,
          contentPadding: const EdgeInsets.all(AppTheme.space4),
        ),
      ),
    );
  }

  Widget _buildActions() {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Text(
          'Enter 提交 • Esc 关闭',
          style: TextStyle(
            color: AppTheme.textTertiary,
            fontSize: 12,
          ),
        ),
        ElevatedButton(
          onPressed: _isSubmitting ? null : _submitEvent,
          style: ElevatedButton.styleFrom(
            backgroundColor: AppTheme.accentPrimary,
            foregroundColor: AppTheme.surface0,
            elevation: 0,
            padding: const EdgeInsets.symmetric(
              horizontal: AppTheme.space6,
              vertical: AppTheme.space3,
            ),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(AppTheme.radiusFull),
            ),
          ),
          child: _isSubmitting
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    valueColor: AlwaysStoppedAnimation<Color>(
                      AppTheme.surface0,
                    ),
                  ),
                )
              : const Text(
                  '记录',
                  style: TextStyle(
                    fontWeight: FontWeight.w600,
                    fontSize: 14,
                  ),
                ),
        ),
      ],
    );
  }
}
