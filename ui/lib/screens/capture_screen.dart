import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import '../theme/app_theme.dart';
import '../providers/conversation_provider.dart';
import '../providers/app_provider.dart';

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

  Future<void> _submitCapture() async {
    if (_isSubmitting || _controller.text.trim().isEmpty) return;

    setState(() => _isSubmitting = true);

    try {
      // Get or create a conversation
      final conversationId = ref.read(selectedConversationIdProvider);
      final repo = ref.read(conversationRepositoryProvider);

      String targetConvId = conversationId ?? '1'; // Use first conversation as default

      await repo.sendMessage(targetConvId, _controller.text);
      ref.invalidate(messagesProvider);
      ref.invalidate(conversationsProvider);

      if (mounted) {
        _controller.clear();

        // Switch back to main application mode
        await Future.delayed(const Duration(milliseconds: 200));
        final windowService = ref.read(windowServiceProvider);
        await windowService.switchToMainMode();
      }
    } finally {
      if (mounted) {
        setState(() => _isSubmitting = false);
      }
    }
  }

  void _closeWindow() async {
    // Switch back to main mode
    final windowService = ref.read(windowServiceProvider);
    await windowService.switchToMainMode();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.transparent,
      body: KeyboardListener(
        focusNode: FocusNode(),
        onKeyEvent: (event) {
          if (event is KeyDownEvent) {
            if (event.logicalKey == LogicalKeyboardKey.escape) {
              _closeWindow();
            } else if (event.logicalKey == LogicalKeyboardKey.enter &&
                !HardwareKeyboard.instance.isShiftPressed &&
                !HardwareKeyboard.instance.isControlPressed) {
              _submitCapture();
            }
          }
        },
        child: Center(
          child: Container(
            width: 600,
            padding: const EdgeInsets.all(16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                // Alfred-style search box
                Container(
                  decoration: BoxDecoration(
                    color: const Color(0xFF1C1C1E).withValues(alpha: 0.95),
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(
                      color: Colors.white.withValues(alpha: 0.1),
                      width: 1,
                    ),
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black.withValues(alpha: 0.5),
                        blurRadius: 40,
                        spreadRadius: 0,
                        offset: const Offset(0, 20),
                      ),
                    ],
                  ),
                  child: Column(
                    children: [
                      // Input area
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 24,
                          vertical: 20,
                        ),
                        child: Row(
                          children: [
                            // Icon
                            Container(
                              width: 40,
                              height: 40,
                              decoration: BoxDecoration(
                                gradient: LinearGradient(
                                  begin: Alignment.topLeft,
                                  end: Alignment.bottomRight,
                                  colors: [
                                    AppTheme.accentPrimary,
                                    AppTheme.accentPrimary.withValues(alpha: 0.7),
                                  ],
                                ),
                                borderRadius: BorderRadius.circular(10),
                                boxShadow: [
                                  BoxShadow(
                                    color: AppTheme.accentPrimary.withValues(alpha: 0.3),
                                    blurRadius: 12,
                                    spreadRadius: 0,
                                  ),
                                ],
                              ),
                              child: const Icon(
                                Icons.edit_note_rounded,
                                color: Colors.white,
                                size: 24,
                              ),
                            ),
                            const SizedBox(width: 16),

                            // Text input
                            Expanded(
                              child: TextField(
                                controller: _controller,
                                focusNode: _focusNode,
                                maxLines: 1,
                                enabled: !_isSubmitting,
                                style: const TextStyle(
                                  color: Colors.white,
                                  fontSize: 20,
                                  fontWeight: FontWeight.w400,
                                  height: 1.4,
                                ),
                                decoration: InputDecoration(
                                  hintText: '记录此刻的想法...',
                                  hintStyle: TextStyle(
                                    color: Colors.white.withValues(alpha: 0.3),
                                    fontSize: 20,
                                    fontWeight: FontWeight.w400,
                                  ),
                                  border: InputBorder.none,
                                  isDense: true,
                                  contentPadding: EdgeInsets.zero,
                                ),
                              ),
                            ),

                            // Loading indicator
                            if (_isSubmitting)
                              const SizedBox(
                                width: 20,
                                height: 20,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                  valueColor: AlwaysStoppedAnimation<Color>(
                                    Colors.white,
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),

                      // Bottom hint bar
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 24,
                          vertical: 12,
                        ),
                        decoration: BoxDecoration(
                          border: Border(
                            top: BorderSide(
                              color: Colors.white.withValues(alpha: 0.06),
                              width: 1,
                            ),
                          ),
                        ),
                        child: Row(
                          children: [
                            _buildHintChip('↵', '提交'),
                            const SizedBox(width: 12),
                            _buildHintChip('esc', '取消'),
                            const Spacer(),
                            Text(
                              'Elsewhen',
                              style: TextStyle(
                                color: Colors.white.withValues(alpha: 0.4),
                                fontSize: 12,
                                fontWeight: FontWeight.w500,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildHintChip(String key, String label) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(
            horizontal: 8,
            vertical: 4,
          ),
          decoration: BoxDecoration(
            color: Colors.white.withValues(alpha: 0.08),
            borderRadius: BorderRadius.circular(4),
            border: Border.all(
              color: Colors.white.withValues(alpha: 0.1),
              width: 1,
            ),
          ),
          child: Text(
            key,
            style: TextStyle(
              color: Colors.white.withValues(alpha: 0.6),
              fontSize: 11,
              fontWeight: FontWeight.w600,
              fontFamily: 'monospace',
            ),
          ),
        ),
        const SizedBox(width: 6),
        Text(
          label,
          style: TextStyle(
            color: Colors.white.withValues(alpha: 0.5),
            fontSize: 12,
          ),
        ),
      ],
    );
  }
}
