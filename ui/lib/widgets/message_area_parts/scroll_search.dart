part of '../message_area.dart';

//! _MessageAreaState 的滚动锚定/搜索/载入更多/空态。
extension _MessageAreaScrollSearch on _MessageAreaState {

  /// 消息列表滚到底部（新消息 / 临时提示 / 会话切换时）。
  /// 懒加载 ListView 滚动中会 build 出新条目、extent 变大，单次滚动目标
  /// 会落后；因此跳到当前底部后在下一帧复查，extent 还有增长就继续跳，
  /// 迭代至固定点 —— 保证真正停在最新消息位置。
  void _scheduleScrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) => _jumpToBottom());
  }

  void _jumpToBottom() {
    if (!_scrollController.hasClients) return;
    _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_scrollController.hasClients) return;
      final pos = _scrollController.position;
      if (pos.pixels < pos.maxScrollExtent - 1) {
        _jumpToBottom();
      }
    });
  }

  Widget _buildSearchBar(List<Message> messages) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
      child: Row(
        children: [
          const Spacer(),
          IconButton(
            tooltip: '按日期定位',
            icon: Icon(
              Icons.calendar_today_outlined,
              size: 18,
              color: _selectedDate == null
                  ? AppTheme.textSecondary
                  : AppTheme.accentPrimary,
            ),
            onPressed: () => _pickDate(messages),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: TextField(
              controller: _searchController,
              decoration: InputDecoration(
                isDense: true,
                hintText: '搜索你说过的话',
                hintStyle: TextStyle(color: AppTheme.textTertiary),
                prefixIcon: Icon(
                  Icons.search,
                  size: 18,
                  color: AppTheme.textSecondary,
                ),
                suffixIcon: (_searchQuery.isNotEmpty || _selectedDate != null)
                    ? IconButton(
                        tooltip: '清除定位',
                        icon: const Icon(Icons.clear, size: 17),
                        onPressed: _clearLocation,
                      )
                    : null,
                filled: true,
                fillColor: AppTheme.surface1,
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.surface3),
                ),
                focusedBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
                  borderSide: BorderSide(color: AppTheme.accentPrimary),
                ),
              ),
              onChanged: (value) => setState(() {
                _searchQuery = value;
                _expanded = true;
                _shownSince = 0;
              }),
            ),
          ),
          const SizedBox(width: 8),
          IconButton(
            tooltip: '回到现在',
            icon: Icon(
              Icons.vertical_align_bottom,
              size: 18,
              color: AppTheme.textSecondary,
            ),
            onPressed: _returnToNow,
          ),
        ],
      ),
    );
  }

  Future<void> _pickDate(List<Message> messages) async {
    final now = DateTime.now();
    final dates = messages.map((m) => m.createdAt).toList()..sort();
    final first = dates.isEmpty ? DateTime(now.year - 1) : dates.first;
    final picked = await showDatePicker(
      context: context,
      initialDate: _selectedDate ?? now,
      firstDate: DateTime(first.year, first.month, first.day),
      lastDate: DateTime(now.year, now.month, now.day),
      helpText: '跳到日期',
      cancelText: '取消',
      confirmText: '跳转',
    );
    if (picked != null && mounted) setState(() => _selectedDate = picked);
  }

  void _clearLocation() {
    _searchController.clear();
    setState(() {
      _searchQuery = '';
      _selectedDate = null;
    });
  }

  void _returnToNow() {
    _clearLocation();
    _scheduleScrollToBottom();
  }

  /// 错误气泡上的「重新生成」：清掉本次失败提示后，
  /// 按与首次发送完全相同的路径重启 AI 生成。
  /// 重试期间以「AI 正在思考…」占位反馈；若再次失败，
  /// runAiGeneration 会追加新的错误气泡（仍可继续重试）。
  void _retryAiGeneration(String conversationId, String inputMessageId) {
    if (ref.read(aiGeneratingProvider).contains(conversationId)) return;
    clearConversationNotices(ref, conversationId);
    if (mounted) setState(() {});
    runAiGeneration(
      ref,
      conversationId,
      inputMessageId: inputMessageId,
      isMounted: () => mounted,
    );
  }

  /// 加载更早消息：窗口向前扩展一页，然后回到顶部看旧内容
  void _loadMore(int total) {
    // 当前有效窗口：未展开时为「只看最近 N 条」的起点
    final currentShown = _expanded
        ? _shownSince
        : (total > _windowSize  ? total - _windowSize : 0);
    setState(() {
      _expanded = true;
      _shownSince = (currentShown - _pageSize).clamp(0, total);
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          0,
          duration: const Duration(milliseconds: 250),
          curve: Curves.easeOut,
        );
      }
    });
  }

  Widget _buildLoadMoreButton(int total) {
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space4),
      child: Center(
        child: TextButton.icon(
          onPressed: () => _loadMore(total),
          icon: const Icon(Icons.keyboard_arrow_up, size: 18),
          label: const Text('显示更早的消息'),
          style: TextButton.styleFrom(
            foregroundColor: AppTheme.textSecondary,
            visualDensity: VisualDensity.compact,
          ),
        ),
      ),
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.forum_outlined, size: 64, color: AppTheme.textTertiary),
          const SizedBox(height: AppTheme.space4),
          Text(
            '选择一个对话开始聊天',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 16),
          ),
        ],
      ),
    );
  }

  Widget _buildNoMessages() {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.chat_bubble_outline,
            size: 48,
            color: AppTheme.textTertiary,
          ),
          const SizedBox(height: AppTheme.space3),
          Text(
            '还没有消息',
            style: TextStyle(color: AppTheme.textSecondary, fontSize: 14),
          ),
          const SizedBox(height: AppTheme.space2),
          Text(
            '在下方输入框开始对话',
            style: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
          ),
        ],
      ),
    );
  }
}
