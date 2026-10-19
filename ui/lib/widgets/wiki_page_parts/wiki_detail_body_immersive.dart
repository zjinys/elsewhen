part of '../wiki_page_detail_view.dart';

//! 沉浸模式遮罩、分区条与分区内容渲染。
  /// 进入沉浸阅读：创建全屏覆盖层并插入 app Overlay（盖住含左侧导航在内的整个窗口）。
extension _WikiPageBodyImmersive on _WikiPageBodyState {
  void _showImmersive() {
    if (_immersiveEntry != null) return;
    final overlay = Overlay.of(context);
    final entry = OverlayEntry(builder: _buildImmersiveOverlay);
    _immersiveEntry = entry;
    overlay.insert(entry);
  }

  /// 退出沉浸阅读：从 Overlay 移除覆盖层。
  void _hideImmersive() {
    _immersiveEntry?.remove();
    _immersiveEntry = null;
  }

  Widget _buildNormalBody(BuildContext context) {
    return DefaultTabController(
      length: _WikiPageSection.values.length,
      initialIndex: _section.index,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildHeader(context),
          if (_editError != null) _buildEditErrorBar(),
          _buildSectionBar(),

          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final width = math.min(
                  720.0,
                  math.max(0.0, constraints.maxWidth - 24),
                );
                final height = math.min(
                  760.0,
                  math.max(0.0, constraints.maxHeight - 24),
                );
                final maxX = math.max(0.0, constraints.maxWidth - width);
                final maxY = math.max(0.0, constraints.maxHeight - height);
                final x = _chatPosition == null
                    ? maxX - 12
                    : _chatPosition!.dx.clamp(0.0, maxX);
                final y = _chatPosition == null
                    ? maxY - 12
                    : _chatPosition!.dy.clamp(0.0, maxY);
                return Stack(
                  children: [
                    Positioned.fill(child: _buildSectionContent()),
                    if (!_chatOpen)
                      Positioned(
                        right: 16,
                        bottom: 16,
                        child: FloatingActionButton.small(
                          // 显式 heroTag：不设时 FAB 用默认 tag，路由切换时
                          // 若前后页面（或嵌套 Navigator）各有一个 FAB，
                          // Hero 控制器会抛「multiple heroes share the same tag」。
                          heroTag: 'wiki-detail-ai-chat-fab',
                          tooltip: '和 AI 讨论此页',
                          onPressed: () => setState(() {
                            _chatOpen = true;
                            _chatInitialized = true;
                          }),
                          child: const Icon(Icons.auto_awesome_outlined),
                        ),
                      ),
                    // 沉浸期间正常态聊天面板不挂载：沉浸覆盖层里有同 slug 的另一个
                    // panel，并存会成两个独立实例（各自调 AI 后端），状态会乱。
                    if (_chatInitialized && _immersiveEntry == null)
                      Positioned(
                        left: x,
                        top: y,
                        width: width,
                        height: height,
                        child: Offstage(
                          offstage: !_chatOpen,
                          child: Material(
                            elevation: 12,
                            borderRadius: BorderRadius.circular(
                              AppTheme.radiusMedium,
                            ),
                            clipBehavior: Clip.antiAlias,
                            child: WikiAiChatPanel(
                              slug: widget.page.slug,
                              onMinimize: () =>
                                  setState(() => _chatOpen = false),
                              onDragUpdate: (delta) => setState(() {
                                _chatPosition = Offset(
                                  (x + delta.dx).clamp(0.0, maxX),
                                  (y + delta.dy).clamp(0.0, maxY),
                                );
                              }),
                            ),
                          ),
                        ),
                      ),
                  ],
                );
              },
            ),
          ),

          _buildFooter(),
        ],
      ),
    );
  }

  /// 沉浸阅读覆盖层：不透明全屏，盖住所有 chrome。
  /// 内容 = 居中限宽的只读正文 + 右下角 AI 聊天入口（沿用原面板的可拖/可最小化机制）
  /// + 左上角退出按钮 / Esc 退出。
  /// 设计要点：
  /// - 暖纸底色（surface0 微暖调），减少纯白眩光；
  /// - 正文限宽 780px，接近纸质书行宽，视线不疲劳；
  /// - 文字色 softened（textSecondary 代替 textPrimary），降低对比度疲劳；
  /// - 退出按钮 / FAB 用 surface2 底色 + 微阴影，不喧宾夺主；
  /// - 底部加阅读进度条，给长文位置感。
  Widget _buildImmersiveOverlay(BuildContext context) {
    final isDark = AppTheme.brightness == Brightness.dark;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): _hideImmersive,
      },
      child: Focus(
        autofocus: true,
        // StatefulBuilder：覆盖层是独立 OverlayEntry，不随 _WikiPageBody 的 build 重建。
        // 聊天开合 / 拖动 / 最小化这些交互要触发覆盖层自身重建，故用它的局部 setState。
        child: StatefulBuilder(
          builder: (context, setOverlayState) => Material(
            color: isDark ? AppTheme.surface0 : const Color(0xFFF8F6F2),
            child: LayoutBuilder(
              builder: (context, constraints) {
                // 聊天面板定位逻辑与正常态一致（同一份 _chatPosition / 夹取范围）。
                final width = math.min(
                  720.0,
                  math.max(0.0, constraints.maxWidth - 24),
                );
                final height = math.min(
                  760.0,
                  math.max(0.0, constraints.maxHeight - 24),
                );
                final maxX = math.max(0.0, constraints.maxWidth - width);
                final maxY = math.max(0.0, constraints.maxHeight - height);
                final x = _chatPosition == null
                    ? maxX - 12
                    : _chatPosition!.dx.clamp(0.0, maxX);
                final y = _chatPosition == null
                    ? maxY - 12
                    : _chatPosition!.dy.clamp(0.0, maxY);
                return Stack(
                  children: [
                    // 只读正文：水平居中、限宽，上下留呼吸空间。
                    // AppFlowyEditor（shrinkWrap:false）需要有界高度才能内部滚动，
                    // 所以用 Center（保留高度约束）而不是 Align（会松开约束）。
                    Positioned.fill(
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(
                          AppTheme.space6,
                          72,
                          AppTheme.space6,
                          64,
                        ),
                        child: Center(
                          child: ConstrainedBox(
                            constraints: const BoxConstraints(
                              maxWidth: _kImmersiveMaxWidth,
                            ),
                            child: SizedBox.expand(
                              // 沉浸覆盖层是只读的，不能用正常态的 _editorKey：正常态正文
                              // 编辑器（_buildNormalBody 里）始终挂着、也用这个 key，沉浸再挂
                              // 会同 key 冲突（Duplicate GlobalKey）。只读不编辑不保存，无需 key。
                              child: _ImmersiveContent(
                                slug: widget.page.slug,
                                contentMd: widget.page.contentMd,
                                onWikiLinkTap: _openWikiPageFromSlug,
                                onSave: _persistEdit,
                                onSaveError: (e) {},
                                onDirtyChanged: _setDirty,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                    // 左上角退出按钮：surface2 底色 + 微阴影，不喧宾夺主。
                    Positioned(
                      left: 20,
                      top: 20,
                      child: Container(
                        decoration: BoxDecoration(
                          color: AppTheme.surface2,
                          borderRadius: BorderRadius.circular(10),
                          boxShadow: [
                            BoxShadow(
                              color: isDark
                                  ? Colors.black.withValues(alpha: 0.3)
                                  : Colors.black.withValues(alpha: 0.08),
                              blurRadius: 8,
                              offset: const Offset(0, 2),
                            ),
                          ],
                        ),
                        child: IconButton(
                          tooltip: '退出沉浸阅读（Esc）',
                          onPressed: _hideImmersive,
                          icon: Icon(
                            Icons.close,
                            size: 20,
                            color: AppTheme.textSecondary,
                          ),
                        ),
                      ),
                    ),
                    // 右下角 AI 聊天入口（未打开时）。
                    if (!_chatOpen)
                      Positioned(
                        right: 20,
                        bottom: 20,
                        child: Container(
                          decoration: BoxDecoration(
                            color: AppTheme.surface2,
                            borderRadius: BorderRadius.circular(12),
                            boxShadow: [
                              BoxShadow(
                                color: isDark
                                    ? Colors.black.withValues(alpha: 0.3)
                                    : Colors.black.withValues(alpha: 0.08),
                                blurRadius: 10,
                                offset: const Offset(0, 3),
                              ),
                            ],
                          ),
                          child: Material(
                            color: Colors.transparent,
                            child: InkWell(
                              borderRadius: BorderRadius.circular(12),
                              onTap: () => setOverlayState(() {
                                _chatOpen = true;
                                _chatInitialized = true;
                              }),
                              child: Padding(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 14,
                                  vertical: 10,
                                ),
                                child: Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Icon(
                                      Icons.auto_awesome_outlined,
                                      size: 18,
                                      color: AppTheme.accentPrimary,
                                    ),
                                    const SizedBox(width: 6),
                                    Text(
                                      '问 AI',
                                      style: TextStyle(
                                        fontSize: 13,
                                        fontWeight: FontWeight.w500,
                                        color: AppTheme.textSecondary,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                    // AI 聊天面板（可拖动、可最小化），与正常态同一份状态。
                    if (_chatInitialized)
                      Positioned(
                        left: x,
                        top: y,
                        width: width,
                        height: height,
                        child: Offstage(
                          offstage: !_chatOpen,
                          child: Material(
                            elevation: 12,
                            borderRadius: BorderRadius.circular(
                              AppTheme.radiusMedium,
                            ),
                            clipBehavior: Clip.antiAlias,
                            child: WikiAiChatPanel(
                              slug: widget.page.slug,
                              onMinimize: () =>
                                  setOverlayState(() => _chatOpen = false),
                              onDragUpdate: (delta) => setOverlayState(() {
                                _chatPosition = Offset(
                                  (x + delta.dx).clamp(0.0, maxX),
                                  (y + delta.dy).clamp(0.0, maxY),
                                );
                              }),
                            ),
                          ),
                        ),
                      ),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildSectionBar() {
    return Container(
      height: 48,
      padding: const EdgeInsets.symmetric(horizontal: AppTheme.space6),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: AppTheme.surface3)),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          // 窄视口（<480 内宽，对应 ~528 以下窗口）下编辑操作收成紧凑态，
          // 避免「取消/完成」按钮把 Tab 栏挤到溢出。
          final compact = constraints.maxWidth < 480;
          return Row(
            // 不自适应时 TabBar 用自然宽度（isScrollable），宽度随内容；
            // spaceBetween 让 Tab 组贴左、编辑操作居中、聊天贴右。
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                child: TabBar(
                  controller: _sectionController,
                  isScrollable: true,
                  tabAlignment: TabAlignment.start,
                  labelColor: AppTheme.textPrimary,
                  unselectedLabelColor: AppTheme.textTertiary,
                  indicatorColor: AppTheme.accentPrimary,
                  indicatorWeight: 2,
                  labelStyle: const TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                  ),
                  unselectedLabelStyle: const TextStyle(fontSize: 13),
                  dividerColor: Colors.transparent,
                  onTap: (index) => setState(() {
                    _section = _WikiPageSection.values[index];
                  }),
                  tabs: [
                    for (final section in _WikiPageSection.values)
                      Tab(
                        text: section == _WikiPageSection.content && !_canEdit
                            ? '原文'
                            : section.label,
                      ),
                  ],
                ),
              ),
              if (_section == _WikiPageSection.content) ...[
                // 右侧操作组：「正文阅读设置」永远贴最右；沉浸阅读恒在，排
                // 在它左侧；编辑操作（编辑正文/取消/完成）只在可编辑页出现，
                // 排最左——编辑按钮显隐不影响右侧两个阅读按钮的位置。
                // 三个操作按视口宽度统一：宽 = 图标 + 文字，窄 = 纯图标。
                Row(
                  mainAxisSize: MainAxisSize.min,
                  spacing: 6,
                  children: [
                    if (_canEdit) _buildInlineEditActions(compact: compact),
                    // 沉浸阅读：隐藏标题栏 / Tab 栏 / 页脚 / 侧栏，只留正文 + AI
                    // 聊天入口。覆盖层是只读的，与可编辑性无关，故素材页
                    // （source/note 只读）同样要有入口——放在编辑组之外。
                    _barAction(
                      compact: compact,
                      icon: Icons.menu_book_outlined,
                      label: '沉浸阅读',
                      tooltip: '沉浸阅读（Esc 退出）',
                      onPressed: _showImmersive,
                    ),
                    // 正文阅读参数（AA 浮层：字体/字号/行距，两层覆盖模型）。
                    _barAction(
                      compact: compact,
                      icon: Icons.format_size,
                      label: '阅读设置',
                      tooltip: '正文阅读设置（字体 / 字号 / 行距）',
                      onPressed: () => showWikiReadingSettings(context),
                    ),
                  ],
                ),
              ],
            ],
          );
        },
      ),
    );
  }

  Widget _buildSectionContent() {
    return Padding(
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space3,
        AppTheme.space6,
        0,
      ),
      child: Align(
        alignment: Alignment.topLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: switch (_section) {
            _WikiPageSection.content => WikiContentEditor(
              key: _editorKey,
              slug: widget.page.slug,
              contentMd: widget.page.contentMd,
              editable: _editing,
              onWikiLinkTap: _openWikiPageFromSlug,
              onSave: _persistEdit,
              onSaveError: (e) {
                if (!mounted || e is _SaveCancelled) return;
                setState(() => _editError = _errText(e));
              },
              onDirtyChanged: _setDirty,
              // 两层覆盖模型：编辑器覆盖 ?? 全局内容字体（system = 跟随系统字体）
              fontFamily: ref.watch(
                settingsProvider.select((s) => s.contentFontName),
              ),
              fontSize: ref.watch(
                settingsProvider.select((s) => s.contentFontSize),
              ),
              lineHeight: ref.watch(
                settingsProvider.select((s) => s.contentLineHeight),
              ),
            ),
            _WikiPageSection.relations => SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (widget.page.basedOn != null)
                    WikiSourceLink(slug: widget.page.basedOn!),
                  KnowledgeOriginLinks(slug: widget.page.slug),
                  _EntityMergeControls(page: widget.page),
                  _EntityFacts(slug: widget.page.slug),
                  _EntityRelatedTodos(page: widget.page),
                  _EntityGuidance(slug: widget.page.slug),
                ],
              ),
            ),
            _WikiPageSection.outputs => KnowledgeOutputsSection(
              page: widget.page,
            ),
            _WikiPageSection.sources => KnowledgeSourcesSection(
              page: widget.page,
            ),
          },
        ),
      ),
    );
  }

  Widget _buildInlineEditActions({bool compact = false}) {
    if (_editing) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (compact)
            IconButton(
              tooltip: '取消',
              visualDensity: VisualDensity.compact,
              onPressed: _saving ? null : _cancelEditing,
              icon: const Icon(Icons.close, size: 18),
            )
          else ...[
            TextButton(
              onPressed: _saving ? null : _cancelEditing,
              child: const Text('取消'),
            ),
            const SizedBox(width: 2),
          ],
          FilledButton(
            onPressed: _saving ? null : _finishEditing,
            style: FilledButton.styleFrom(
              minimumSize: const Size(58, 32),
              padding: EdgeInsets.symmetric(horizontal: compact ? 8 : 12),
            ),
            child: _saving
                ? const SizedBox(
                    width: 13,
                    height: 13,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('完成'),
          ),
        ],
      );
    }
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        _barAction(
          compact: compact,
          icon: Icons.edit_outlined,
          label: '编辑正文',
          tooltip: '编辑正文',
          onPressed: _enterEditing,
        ),
        // 删除：仅人工主题/档案页可删（素材页由导入维护管辖）。
        if (_canEdit)
          _barAction(
            compact: compact,
            icon: Icons.delete_outline,
            label: '删除页面',
            tooltip: '删除页面',
            onPressed: _confirmDeletePage,
          ),
      ],
    );
  }

  /// P5b：业务删除。同步库内先捕获 deleted 变更再物理删除；删除后关闭本页
  /// tab（对端离线迟到编辑会成为 wiki_page_deleted 冲突候选，不静默覆盖）。
  Future<void> _confirmDeletePage() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppTheme.surface1,
        title: Text(
          '删除页面？',
          style: TextStyle(color: AppTheme.textPrimary, fontSize: 16),
        ),
        content: Text(
          '「${widget.page.title}」将被删除，修订历史随之清除，且不可恢复。'
          '其他设备同步后同样删除；它们与此页冲突的离线编辑会保留为冲突候选。',
          style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text('取消', style: TextStyle(color: AppTheme.textSecondary)),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text('删除', style: TextStyle(color: AppTheme.error)),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    await repo.deleteWikiPage(slug: widget.page.slug);
    ref.invalidate(wikiPageProvider(widget.page.slug));
    ref.invalidate(wikiPagesProvider);
    if (!mounted) return;
    closeWikiTab(ref, 'page-${widget.page.slug}');
  }

  /// section bar 右侧操作的统一样式，供「编辑正文 / 沉浸阅读 / 阅读设置」共用：
  /// - 宽视口（!compact）：图标 + 文字的 tonal 标签按钮，统一 32 高、15 图标、
  ///   10 横向 padding，保证三者同高同距。
  /// - 窄视口（compact）：纯图标按钮，统一 18 图标 + tooltip，避免挤爆 Tab 栏。
}
