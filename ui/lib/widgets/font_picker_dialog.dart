import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

import '../models/settings.dart';
import '../theme/app_theme.dart';
import '../utils/system_fonts.dart';

/// 「跟随全局」哨兵：编辑器字体覆盖用，调用方映射回 null。
const String kFontFollowGlobal = '__follow_global__';

/// 自研字体选择框：系统默认 + fontconfig 本地字体 + Google Fonts，
/// 每行用自身字体实时预览。返回存值（`system` / `google:X` / `local:X`），
/// 取消返回 null；[includeFollowGlobal] 为 true 时首行是「跟随全局」，
/// 点按返回 [kFontFollowGlobal]。
Future<String?> showFontPickerDialog(
  BuildContext context, {
  required String? current,
  bool includeFollowGlobal = false,
  String title = '选择字体',
}) {
  return showDialog<String>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: Text(title),
      contentPadding: const EdgeInsets.fromLTRB(8, 8, 8, 16),
      content: SizedBox(
        width: 520,
        height: 560,
        child: _FontPickerBody(
          current: current,
          includeFollowGlobal: includeFollowGlobal,
        ),
      ),
    ),
  );
}

class _FontPickerBody extends StatefulWidget {
  const _FontPickerBody({
    required this.current,
    required this.includeFollowGlobal,
  });

  final String? current;
  final bool includeFollowGlobal;

  @override
  State<_FontPickerBody> createState() => _FontPickerBodyState();
}

class _FontPickerBodyState extends State<_FontPickerBody> {
  final _searchController = TextEditingController();
  String _query = '';
  late final Future<List<SystemFontEntry>> _localFuture;
  late final List<String> _googleFamilies;

  @override
  void initState() {
    super.initState();
    _localFuture = SystemFontService.instance.listFonts();
    _googleFamilies = GoogleFonts.asMap().keys.toList()..sort();
  }

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  bool _selected(String kind, String family) {
    if (widget.current == null) return false;
    final c = parseStoredFont(widget.current!);
    return c.kind == kind && c.family == family;
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
          child: TextField(
            controller: _searchController,
            decoration: InputDecoration(
              hintText: '搜索字体…',
              prefixIcon: const Icon(Icons.search, size: 18),
              suffixIcon: _query.isEmpty
                  ? null
                  : IconButton(
                      icon: const Icon(Icons.close, size: 16),
                      tooltip: '清除',
                      onPressed: () {
                        _searchController.clear();
                        setState(() => _query = '');
                      },
                    ),
              isDense: true,
            ),
            onChanged: (v) => setState(() => _query = v.trim().toLowerCase()),
          ),
        ),
        Expanded(
          child: FutureBuilder<List<SystemFontEntry>>(
            future: _localFuture,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting) {
                return const Center(child: CircularProgressIndicator());
              }
              return _buildList(context, snapshot.data ?? const []);
            },
          ),
        ),
      ],
    );
  }

  Widget _buildList(BuildContext context, List<SystemFontEntry> local) {
    final q = _query;
    final matchLocal = q.isEmpty
        ? local
        : local.where((e) => e.family.toLowerCase().contains(q)).toList();
    final matchGoogle = (q.isEmpty
        ? _googleFamilies
        : _googleFamilies.where((f) => f.toLowerCase().contains(q)).toList());
    const googleCap = 80;
    final showGoogle = matchGoogle.take(googleCap).toList();
    final hiddenGoogle = matchGoogle.length - showGoogle.length;

    // 展平为轻量条目交给 builder 懒构建：本机可能上千个字体，
    // 直接 for 循环会在每次搜索重建上千个行 widget。
    final items = <_Item>[
      if (widget.includeFollowGlobal) _FollowGlobalItem(),
      _SystemDefaultItem(),
      if (matchLocal.isNotEmpty) _HeaderItem('本地字体 (${matchLocal.length})'),
      for (final e in matchLocal) _LocalItem(e),
      if (showGoogle.isNotEmpty)
        _HeaderItem('Google Fonts (${matchGoogle.length})'),
      for (final f in showGoogle) _GoogleItem(f),
      if (hiddenGoogle > 0) _HintItem('…还有 $hiddenGoogle 个，输入关键词缩小范围'),
      if (matchLocal.isEmpty &&
          showGoogle.isEmpty &&
          !widget.includeFollowGlobal)
        _HintItem('没有匹配的字体'),
    ];

    return ListView.builder(
      padding: const EdgeInsets.only(bottom: 8),
      itemCount: items.length,
      itemBuilder: (context, index) {
        final item = items[index];
        return switch (item) {
          _FollowGlobalItem() => _Row(
            title: '跟随全局',
            subtitle: '使用全局字体设置',
            preview: null,
            selected: widget.current == null,
            onTap: () => Navigator.of(context).pop(kFontFollowGlobal),
          ),
          _SystemDefaultItem() => _Row(
            title: '系统默认',
            subtitle: '跟随操作系统，不指定字体',
            preview: null,
            selected: widget.current == AppFonts.system,
            onTap: () => Navigator.of(context).pop(AppFonts.system),
          ),
          _HeaderItem(:final text) => _SectionHeader(text),
          _LocalItem(:final entry) => _LocalRow(
            entry: entry,
            selected: _selected('local', entry.family),
            onTap: () => Navigator.of(context).pop('local:${entry.family}'),
          ),
          _GoogleItem(:final family) => _Row(
            title: family,
            subtitle: '在线字体 · 首次使用需联网下载',
            preview: GoogleFonts.getFont(family, fontSize: 15),
            selected: _selected('google', family),
            onTap: () => Navigator.of(context).pop('google:$family'),
          ),
          _HintItem(:final text) => Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: Center(
              child: Text(
                text,
                style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
              ),
            ),
          ),
        };
      },
    );
  }
}

/// 字体列表的轻量条目模型：展平交给 ListView.builder 懒构建。
sealed class _Item {
  const _Item();
}

final class _FollowGlobalItem extends _Item {}

final class _SystemDefaultItem extends _Item {}

final class _HeaderItem extends _Item {
  const _HeaderItem(this.text);
  final String text;
}

final class _LocalItem extends _Item {
  const _LocalItem(this.entry);
  final SystemFontEntry entry;
}

final class _GoogleItem extends _Item {
  const _GoogleItem(this.family);
  final String family;
}

final class _HintItem extends _Item {
  const _HintItem(this.text);
  final String text;
}

/// 字体名标签：google 直接渲染，本地字体就绪后用自身渲染（设置页/阅读设置共用）。
class FontNameLabel extends StatelessWidget {
  const FontNameLabel({super.key, required this.stored, this.fallbackStyle});

  final String stored;
  final TextStyle? fallbackStyle;

  @override
  Widget build(BuildContext context) {
    final parsed = parseStoredFont(stored);
    if (parsed.kind != 'local') {
      return Text(
        AppFonts.displayNameOf(stored),
        style: fallbackStyle,
        overflow: TextOverflow.ellipsis,
      );
    }
    return FutureBuilder<bool>(
      future: SystemFontService.instance.ensureLoaded(parsed.family),
      builder: (context, snapshot) => Text(
        parsed.family,
        style: (fallbackStyle ?? const TextStyle()).copyWith(
          fontFamily: snapshot.data == true ? parsed.family : null,
        ),
        overflow: TextOverflow.ellipsis,
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
      child: Text(
        text,
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w600,
          color: AppTheme.textTertiary,
        ),
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({
    required this.title,
    required this.subtitle,
    required this.preview,
    required this.selected,
    required this.onTap,
  });

  final String title;
  final String subtitle;
  final TextStyle? preview;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      dense: true,
      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 2),
      title: Text(title, style: preview, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        '${preview != null ? '中文测试 AaBbGg 1234 · ' : ''}$subtitle',
        style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
        overflow: TextOverflow.ellipsis,
      ),
      trailing: selected
          ? Icon(Icons.check_circle, size: 20, color: AppTheme.accentPrimary)
          : null,
      onTap: onTap,
    );
  }
}

/// 本地字体行：懒加载字体文件后用自身渲染标题。
class _LocalRow extends StatelessWidget {
  const _LocalRow({
    required this.entry,
    required this.selected,
    required this.onTap,
  });

  final SystemFontEntry entry;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<bool>(
      future: SystemFontService.instance.ensureLoaded(entry.family),
      builder: (context, snapshot) {
        final ready = snapshot.data == true;
        return _Row(
          title: entry.family,
          subtitle: entry.style,
          preview: TextStyle(
            fontSize: 15,
            fontFamily: ready ? entry.family : null,
          ),
          selected: selected,
          onTap: onTap,
        );
      },
    );
  }
}
