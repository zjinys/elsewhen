import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import '../models/conversation.dart';

/// 三处对话共用的阅读规则：历史加载到最新，新一轮回复从第一条开头读。
/// 同轮后续回复和正文增长都不推动阅读位置；显式发送/回到最新开始下一轮。
class ChatReadingController extends ScrollController {
  final _seenIds = <String>{};
  final _keys = <String, GlobalKey>{};
  List<String> _order = [];
  bool _initialized = false;
  bool _disposed = false;
  bool _nearBottom = true;
  int _revision = 0;
  String? _readingAnchorId;

  bool get preserveWindow => _readingAnchorId != null || !_nearBottom;

  ChatReadingController() {
    addListener(() {
      if (hasClients && position.hasContentDimensions) {
        _nearBottom = position.extentAfter < 120;
      }
    });
  }

  void reset() {
    _revision++;
    _initialized = false;
    _nearBottom = true;
    _readingAnchorId = null;
    _seenIds.clear();
    _keys.clear();
    _order = [];
  }

  /// 在实际展示的数据分支调用；loading/旧会话数据不能消费首次定位。
  void observeMessages(List<Message> messages, {bool follow = true}) {
    _order = messages.map((m) => m.id).toList();
    final currentIds = _order.toSet();
    final added = messages.where((m) => !_seenIds.contains(m.id)).toList();
    _seenIds.addAll(_order);
    _keys.removeWhere((id, _) => !currentIds.contains(id));
    if (!_initialized) {
      _initialized = true;
      if (follow) scrollToLatest();
      return;
    }
    if (!follow || !_nearBottom || added.isEmpty) return;
    // 明确的新输入开启下一轮；回复按当前展示序寻找第一条，而不是取 last。
    if (added.any((m) => m.isUser)) _readingAnchorId = null;
    if (_readingAnchorId != null) return;
    final replies = added.where((m) => m.isAssistant && m.kind == 'chat');
    if (replies.isEmpty) {
      scrollToLatest();
    } else {
      _readingAnchorId = replies.first.id;
      final revision = ++_revision;
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _revealReply(revision),
      );
    }
  }

  Widget anchorMessage(Message message, Widget child) => KeyedSubtree(
    key: _keys.putIfAbsent(message.id, () => GlobalKey()),
    child: child,
  );

  Widget trackUserScroll(Widget child) =>
      NotificationListener<ScrollNotification>(
        onNotification: (notification) {
          if (notification.depth == 0 &&
              ((notification is ScrollStartNotification &&
                      notification.dragDetails != null) ||
                  (notification is UserScrollNotification &&
                      notification.direction != ScrollDirection.idle))) {
            // 用户拖动/滚轮优先，取消已经排队的逐帧定位。
            _revision++;
          }
          return false;
        },
        child: child,
      );

  /// 生成中/错误等尾部状态只在用户仍跟随尾部时露出。
  void followTail() {
    if (_nearBottom && _readingAnchorId == null) scrollToLatest();
  }

  void scrollToLatest() {
    _readingAnchorId = null;
    final revision = ++_revision;
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _jumpToBottom(revision),
    );
  }

  bool _canScroll(int revision) =>
      !_disposed && revision == _revision && hasClients;

  void _jumpToBottom(int revision) {
    if (!_canScroll(revision)) return;
    jumpTo(position.maxScrollExtent);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_canScroll(revision) && position.extentAfter > 1) {
        _jumpToBottom(revision);
      }
    });
  }

  void _revealReply(int revision, [int remaining = 40]) {
    if (!_canScroll(revision) || remaining == 0) return;
    final target = _keys[_readingAnchorId]?.currentContext?.findRenderObject();
    if (target != null && target.attached) {
      final top = RenderAbstractViewport.of(target)
          .getOffsetToReveal(target, 0)
          .offset;
      final offset = (top - 16).clamp(
        position.minScrollExtent,
        position.maxScrollExtent,
      );
      if ((offset - position.pixels).abs() > 1) jumpTo(offset);
      return;
    }
    // 懒加载列表逐屏找到目标。排队输入的回复可能插在已有消息前，需要向上找。
    final targetIndex = _order.indexOf(_readingAnchorId ?? '');
    if (targetIndex < 0) return;
    final builtIndices = [
      for (var i = 0; i < _order.length; i++)
        if (_keys[_order[i]]?.currentContext != null) i,
    ];
    final direction =
        builtIndices.isNotEmpty && targetIndex < builtIndices.first ? -1 : 1;
    final offset =
        (position.pixels + direction * position.viewportDimension * 0.8).clamp(
          position.minScrollExtent,
          position.maxScrollExtent,
        );
    if ((offset - position.pixels).abs() < 1) return;
    jumpTo(offset);
    WidgetsBinding.instance.addPostFrameCallback(
      (_) => _revealReply(revision, remaining - 1),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _revision++;
    super.dispose();
  }
}
