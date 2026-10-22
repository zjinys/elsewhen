part of '../wiki_page_detail_view.dart';

//! 编辑持久化、冲突处理与页眉/页脚/标签/关系行。
extension _WikiPageBodyEdit on _WikiPageBodyState {
  Widget _barAction({
    required bool compact,
    required IconData icon,
    required String label,
    required String tooltip,
    required VoidCallback? onPressed,
  }) {
    if (compact) {
      return IconButton(
        tooltip: tooltip,
        visualDensity: VisualDensity.compact,
        onPressed: onPressed,
        icon: Icon(icon, size: 18),
        color: AppTheme.textSecondary,
      );
    }
    return Tooltip(
      message: tooltip,
      child: FilledButton.tonalIcon(
        onPressed: onPressed,
        icon: Icon(icon, size: 15),
        label: Text(label),
        style: FilledButton.styleFrom(
          minimumSize: const Size(0, 32),
          padding: const EdgeInsets.symmetric(horizontal: 10),
        ),
      ),
    );
  }

  Widget _buildEditErrorBar() {
    return Container(
      width: double.infinity,
      color: AppTheme.error.withValues(alpha: 0.08),
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space6,
        vertical: AppTheme.space2,
      ),
      child: Text(
        '保存失败：$_editError',
        style: TextStyle(fontSize: 12, color: AppTheme.error),
      ),
    );
  }

  /// 进入编辑态（素材页不显示入口，到不了这里）
  void _enterEditing() {
    setState(() {
      _editing = true;
      _saving = false;
      _editError = null;
    });
    _setDirty(_editorKey.currentState?.isDirty ?? false);
  }

  /// 「完成」：保存正文（无改动直接退出；保存失败留在编辑态，错误条已展示）。
  /// 返回是否保存成功/无改动——v1.5 关闭脏 tab 的「保存并关闭」复用此判定。
  Future<bool> _finishEditing() async {
    final state = _editorKey.currentState;
    if (state == null) return true; // 从未进入编辑态：视为无脏内容
    setState(() => _saving = true);
    final ok = await state.save();
    if (!mounted) return ok;
    setState(() {
      _saving = false;
      if (ok) _editing = false;
    });
    return ok;
  }

  /// 「取消」：放弃修改，从加载快照重建文档
  void _cancelEditing() {
    _editorKey.currentState?.discard();
    setState(() => _editing = false);
    _setDirty(false);
  }

  /// 保存链路（§6.2）：documentToMarkdown → saveWikiPageContent → 刷新 → 退出编辑态。
  /// 乐观锁（§11 Q3）：携带加载时的 updatedAt；编辑期间页面被后台更新则冲突，
  /// 弹「重新加载 / 强制覆盖 / 取消」三选；取消/重载抛 [_SaveCancelled]，
  /// 中止保存但不视为失败（不显示错误条）。
  Future<void> _persistEdit(String markdown) async {
    final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
    try {
      await repo.saveWikiPageContent(
        slug: widget.page.slug,
        contentMd: markdown,
        reason: 'GUI 编辑',
        expectedUpdatedAt: widget.page.updatedAt.toUtc().toIso8601String(),
      );
    } on Exception catch (e) {
      if (!mounted || !_isConflictError(e)) rethrow;
      // 保存尝试已被拒，先放下 saving 态再弹冲突对话框
      // （否则工具栏 spinner 常转，pumpAndSettle 永不收敛）。
      setState(() => _saving = false);
      final action = await _showConflictDialog();
      if (!mounted) throw const _SaveCancelled();
      switch (action) {
        case 'overwrite':
          // 强制覆盖：跳过乐观锁再存一次；仍失败则交给错误条
          await repo.saveWikiPageContent(
            slug: widget.page.slug,
            contentMd: markdown,
            reason: 'GUI 编辑（冲突后覆盖）',
            expectedUpdatedAt: null,
          );
        case 'reload':
          // 重新加载：放弃本地改动，刷新为最新页面内容
          ref.invalidate(wikiPageProvider(widget.page.slug));
          ref.invalidate(wikiPagesProvider);
          setState(() {
            _editing = false;
            _editError = null;
          });
          _setDirty(false);
          throw const _SaveCancelled();
        default:
          // 取消：留在编辑态，改动保留，不显示错误条
          throw const _SaveCancelled();
      }
    }
    ref.invalidate(wikiPageProvider(widget.page.slug));
    ref.invalidate(wikiPagesProvider);
    if (!mounted) return;
    setState(() {
      _editing = false;
      _editError = null;
    });
    _setDirty(false);
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        const SnackBar(
          content: Text('已保存到知识库'),
          behavior: SnackBarBehavior.floating,
          duration: Duration(seconds: 2),
        ),
      );
  }

  /// 冲突判定：Rust 侧乐观锁拒绝时的错误消息前缀
  static bool _isConflictError(Object e) => e.toString().contains('编辑冲突');

  /// 编辑冲突三选：重新加载（弃本地）/ 强制覆盖（盖后台）/ 取消（继续编辑）
  Future<String?> _showConflictDialog() {
    return showDialog<String>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('页面已被后台更新'),
        content: const Text(
          '你编辑期间，该页面被后台 digest 更新过。\n'
          '「重新加载」放弃你的修改并查看最新内容；\n'
          '「强制覆盖」以你的修改覆盖后台更新（两个版本都留在修订历史里）。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('cancel'),
            child: const Text('取消'),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop('reload'),
            child: const Text('重新加载'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop('overwrite'),
            child: const Text('强制覆盖'),
          ),
        ],
      ),
    );
  }

  /// 脏标记上报（未保存保护：tab 关闭前确认用）
  void _setDirty(bool dirty) {
    if (!mounted) return;
    ref.read(wikiDirtyTabsProvider.notifier).update((set) {
      if (dirty) return {...set, widget.page.slug};
      return {...set}..remove(widget.page.slug);
    });
  }

  /// wikilink 点击：slug → 查询目标页 → 打开/激活对应 tab
  Future<void> _openWikiPageFromSlug(String slug) async {
    try {
      final target = await ref.read(wikiPageProvider(slug).future);
      if (!mounted || target == null) return;
      openWikiPageTab(ref, target);
    } catch (_) {
      // 目标页不存在或查询失败：静默忽略，保持当前阅读位置
    }
  }

  String _errText(Object e) => e.toString().replaceFirst('Exception: ', '');

  Widget _buildHeader(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space3,
        AppTheme.space6,
        AppTheme.space2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.72),
        border: Border(bottom: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Align(
        alignment: Alignment.centerLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // 紧凑主行：kind 徽章 + 标题 + 元数据同一流动行（窄屏自然换行）。
              // 布局原则：头部只保留「这页是什么」，把纵向空间还给正文。
              Wrap(
                spacing: AppTheme.space2,
                runSpacing: 4,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      color: AppTheme.accentPrimary.withValues(alpha: 0.12),
                      borderRadius: BorderRadius.circular(AppTheme.radiusFull),
                    ),
                    child: Text(
                      widget.page.kindLabel,
                      style: TextStyle(
                        fontSize: 11,
                        color: AppTheme.accentPrimary,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 980),
                    child: _buildHeaderTitle(context),
                  ),
                  Text(
                    '${widget.page.slug} · 采集 ${_fmtDate(widget.page.lastSeenAt)}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 11,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                  if (widget.page.sourceUrl != null)
                    if (widget.page.isLocalPath)
                      _LocalPathChip(page: widget.page)
                    else
                      _SourceChip(url: widget.page.sourceUrl!),
                ],
              ),
              if (widget.page.tags.contains('work-item')) ...[
                const SizedBox(height: AppTheme.space2),
                _WorkItemPanel(page: widget.page),
              ],
              // 联系人关系是内容级信息，保持直接可见（空时自隐藏）
              _buildRelationsRow(context),
              // 相关页面（知识关系网 P3，提案 2026-10-19）：
              // 点击跳对端页，人工边可删，「+」从页列表中拉取。——
              // 衍生边（tags 命中标题）不在此删，去改 tags 让事实消失。
              _buildRelatedPagesRow(context),
              // 标签直接展开，避免用户还要打开一个元数据折叠行。
              _buildTagRow(context),
              if (widget.page.kind == 'habit') _buildHabitNudgeEntry(context),
              KnowledgePageActions(
                page: widget.page,
                showSourcesButton:
                    false, // 来源与修订已在下方 Section TabBar 中直观可见，头部避免重复
                onShowSources: () => _showSection(_WikiPageSection.sources),
                onShowOutputs: () => _showSection(_WikiPageSection.outputs),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// 习惯页的主动问候入口。时间/问法/按钮都来自已确认的计划；没有计划时
  /// 打开推断确认框，推断结果不会静默生效。
  Widget _buildHabitNudgeEntry(BuildContext context) {
    final plan = ref.watch(habitNudgeForPageProvider(widget.page.slug)).value;
    final String summary;
    if (plan == null) {
      summary = '到点在主对话里问我，问法和按钮先由你确认';
    } else if (!plan.enabled) {
      summary = '已暂停 · ${plan.localTime}';
    } else {
      summary =
          '${_habitWeekdaySummary(plan.weekdays)} ${plan.localTime} · '
          '${plan.doneLabel}/${plan.missedLabel}';
    }
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space2),
      child: Material(
        color: AppTheme.surface0,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        child: InkWell(
          borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
          onTap: () async {
            final changed = await showHabitNudgeDialog(
              context,
              page: widget.page,
            );
            if (changed == true && context.mounted) {
              ScaffoldMessenger.of(context).showSnackBar(
                const SnackBar(
                  content: Text('定期问候已更新'),
                  behavior: SnackBarBehavior.floating,
                ),
              );
            }
          },
          child: Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: AppTheme.space3,
              vertical: AppTheme.space2,
            ),
            child: Row(
              children: [
                Icon(
                  plan?.enabled == true
                      ? Icons.notifications_active_outlined
                      : Icons.notifications_none,
                  size: 17,
                  color: plan?.enabled == true
                      ? AppTheme.accentPrimary
                      : AppTheme.textTertiary,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        plan == null ? '定期问我' : '定期问候',
                        style: TextStyle(
                          fontSize: 12.5,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.textPrimary,
                        ),
                      ),
                      const SizedBox(height: 1),
                      Text(
                        summary,
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textTertiary,
                        ),
                      ),
                    ],
                  ),
                ),
                Icon(
                  Icons.chevron_right,
                  size: 18,
                  color: AppTheme.textTertiary,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  static String _habitWeekdaySummary(int mask) {
    if (mask == 127) return '每天';
    const names = ['一', '二', '三', '四', '五', '六', '日'];
    final days = [
      for (var i = 0; i < 7; i++)
        if ((mask & (1 << i)) != 0) names[i],
    ];
    return '每周 ${days.join('、')}';
  }

  /// 相关页面（P3 导航面）：数据来自详情派生读（details.relations），
  /// 逻辑独立成 [RelatedPagesRow]（详情页与测试共用）。
  Widget _buildRelatedPagesRow(BuildContext context) =>
      RelatedPagesRow(slug: widget.page.slug);

  /// 知识页标题：单行保留，过长省略并附 tooltip 显示完整标题；末尾挂一个复制按钮。
  ///
  /// 复制按钮放进标题自己的 Row 里（而不是当作头部 Wrap 的另一个子节点），是为了
  /// 让它始终紧贴标题末尾：短标题时贴在文字后面，长标题省略号也贴在省略号后面。
  /// 若作为 Wrap 的独立子节点，窄屏换行后它会变成一个孤零零的图标，混进下面的
  /// slug 元数据里分不清是干什么的。标题用 Flexible，复制按钮定宽，因此按钮永远
  /// 不被挤掉，也不会改变头部 Wrap 的换行行为。
  Widget _buildHeaderTitle(BuildContext context) {
    final title = widget.page.title;
    final style = TextStyle(
      fontSize: 23,
      fontWeight: FontWeight.w600,
      color: AppTheme.textPrimary,
      height: 1.25,
    );
    final text = Text(
      title,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: style,
    );

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Flexible(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final painter = TextPainter(
                text: TextSpan(text: title, style: style),
                maxLines: 1,
                textDirection: TextDirection.ltr,
              )..layout(maxWidth: constraints.maxWidth);
              // 标题在约束内能完整排入一行就不包 tooltip；溢出（省略号出现）才 hover 展示全文。
              if (!painter.didExceedMaxLines) return text;
              return Tooltip(message: title, child: text);
            },
          ),
        ),
        _copyTitleButton(context),
      ],
    );
  }

  /// 复制标题：与来源链接 chip 同一套反馈（floating SnackBar），文案带「标题」
  /// 以区分页面上其他可复制的东西（路径、链接、正文、代码块）。
  Widget _copyTitleButton(BuildContext context) {
    return Tooltip(
      message: '复制标题',
      child: InkWell(
        onTap: () {
          Clipboard.setData(ClipboardData(text: widget.page.title));
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('标题已复制'),
              behavior: SnackBarBehavior.floating,
            ),
          );
        },
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        // 标题是 23px/1.25 的行，图标需要额外留出可点区域才不会显得难点
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          child: Icon(Icons.copy, size: 13, color: AppTheme.textTertiary),
        ),
      ),
    );
  }

  Widget _buildFooter() {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(
        AppTheme.space6,
        AppTheme.space2,
        AppTheme.space6,
        AppTheme.space3,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface1.withValues(alpha: 0.55),
        border: Border(top: BorderSide(color: AppTheme.surface3, width: 1)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: _kReadingMaxWidth),
          child: Row(
            children: [
              Icon(Icons.track_changes, size: 12, color: AppTheme.textTertiary),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  '源于 ${widget.page.sourceEventIds.isEmpty ? "尚无事件溯源" : "${widget.page.sourceEventIds.length} 条事件"}',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// 标签行：标签 chips + 编辑入口（标签是用户组织知识库的主要元数据）
  Widget _buildTagRow(BuildContext context) {
    return Wrap(
      spacing: 6,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        for (final tag in widget.page.tags)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
            decoration: BoxDecoration(
              color: AppTheme.surface2,
              borderRadius: BorderRadius.circular(AppTheme.radiusFull),
              border: Border.all(color: AppTheme.surface3),
            ),
            child: Text(
              '#$tag',
              style: TextStyle(fontSize: 11, color: AppTheme.textSecondary),
            ),
          ),
        InkWell(
          onTap: () => _editTags(context),
          borderRadius: BorderRadius.circular(AppTheme.radiusFull),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  widget.page.tags.isEmpty ? Icons.add : Icons.edit_outlined,
                  size: 12,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(width: 4),
                Text(
                  widget.page.tags.isEmpty ? '添加标签' : '编辑标签',
                  style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  /// 联系人关系区块：AI 从对话识别、用户确认后保存的「联系人 ↔ 事情/项目」。
  /// 双侧方向都以当前页为中心展示（出→ 人·事；入← 人·事）。
  Widget _buildRelationsRow(BuildContext context) {
    final relationsAsync = ref.watch(pageRelationsProvider(widget.page.slug));
    final relations = relationsAsync.value ?? const [];
    if (relations.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          '联系人关系',
          style: TextStyle(
            fontSize: 11,
            fontWeight: FontWeight.w600,
            color: AppTheme.textTertiary,
            letterSpacing: 0.5,
          ),
        ),
        const SizedBox(height: AppTheme.space2),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final r in relations)
              _RelationChip(relation: r, pageSlug: widget.page.slug),
          ],
        ),
      ],
    );
  }

  /// 编辑标签：空格 / 逗号分隔，留空即清空。保存后刷新页面与列表。
  Future<void> _editTags(BuildContext context) async {
    final submitted = await showDialog<String>(
      context: context,
      builder: (_) => _EditTagsDialog(initial: widget.page.tags.join(' ')),
    );
    if (submitted == null || !context.mounted) return;

    final tags = submitted
        .split(RegExp(r'[\s,，、]+'))
        .map((t) => t.trim().replaceFirst(RegExp(r'^#+'), '').trim())
        .where((t) => t.isNotEmpty)
        .toList();

    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.updateWikiTags(slug: widget.page.slug, tags: tags);
      if (!context.mounted) return;
      ref.invalidate(wikiPageProvider(widget.page.slug));
      ref.invalidate(pageRelationsProvider(widget.page.slug));
      ref.invalidate(wikiPagesProvider);
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('标签已更新'),
          behavior: SnackBarBehavior.floating,
        ),
      );
    } catch (e) {
      if (!context.mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            '标签更新失败：${e.toString().replaceFirst('Exception: ', '')}',
          ),
          behavior: SnackBarBehavior.floating,
        ),
      );
    }
  }

  String _fmtDate(DateTime t) {
    return '${t.year}-${t.month.toString().padLeft(2, '0')}-${t.day.toString().padLeft(2, '0')}';
  }
}
