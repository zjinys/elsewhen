part of '../scout_view.dart';

class ScoutProductWorkspace extends ConsumerStatefulWidget {
  const ScoutProductWorkspace({super.key});
  @override
  ConsumerState<ScoutProductWorkspace> createState() =>
      _ScoutProductWorkspaceState();
}

class _ScoutProductWorkspaceState extends ConsumerState<ScoutProductWorkspace> {
  late final _query = TextEditingController(
    text: ref.read(scoutProductQueryProvider),
  );
  @override
  void dispose() {
    _query.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final page = ref.watch(scoutProductPageProvider);
    final view = ref.watch(scoutProductViewProvider);
    final studies =
        ref.watch(scoutStudiesProvider).value ?? <Map<String, dynamic>>[];
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  context.l10n.scoutDiscover,
                  style: TextStyle(
                    fontSize: 20,
                    fontWeight: FontWeight.w600,
                    color: AppTheme.textPrimary,
                  ),
                ),
              ),
              const _ScoutPreferencesButton(),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
          child: TextField(
            controller: _query,
            decoration: InputDecoration(
              hintText: context.l10n.scoutSearchProducts,
              prefixIcon: const Icon(Icons.search),
              suffixIcon: IconButton(
                icon: const Icon(Icons.arrow_forward),
                onPressed: () => ref
                    .read(scoutProductQueryProvider.notifier)
                    .set(_query.text.trim()),
              ),
            ),
            onSubmitted: (s) =>
                ref.read(scoutProductQueryProvider.notifier).set(s.trim()),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Align(
            alignment: Alignment.centerLeft,
            child: Wrap(
              spacing: 8,
              runSpacing: 6,
              children: [
                for (final option in ['signals', 'growth', 'price', 'all'])
                  ChoiceChip(
                    label: Text(switch (option) {
                      'signals' => context.l10n.scoutWhyNow,
                      'growth' => context.l10n.scoutGrowthSignals,
                      'price' => context.l10n.scoutPriceSignals,
                      _ => context.l10n.scoutAllProducts,
                    }),
                    selected: view == option,
                    onSelected: (_) =>
                        ref.read(scoutProductViewProvider.notifier).set(option),
                  ),
              ],
            ),
          ),
        ),
        Expanded(
          child: page.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    '$e'.contains('scout_upgrade_required')
                        ? context.l10n.scoutUpgradeRequired
                        : context.errorMessage(context.l10n.scoutDiscover, e),
                  ),
                  TextButton(
                    onPressed: () => ref.invalidate(scoutProductPageProvider),
                    child: Text(context.l10n.scoutRetry),
                  ),
                  TextButton(
                    onPressed: () => ref
                        .read(scoutWorkspaceProvider.notifier)
                        .set(ScoutWorkspace.activity),
                    child: Text(context.l10n.scoutActivityLog),
                  ),
                ],
              ),
            ),
            data: (p) {
              final visible = p.items.where((product) {
                final study = studies
                    .where(
                      (s) =>
                          s['instance_id'] == p.instance &&
                          (s['product_id'] == product.id ||
                              (product.data['aliases'] as List? ?? []).contains(
                                s['product_id'],
                              )),
                    )
                    .firstOrNull;
                return view == 'all' ||
                    (product.reasons.isNotEmpty &&
                        (study == null ||
                            study['state'] != 'rejected' ||
                            scoutHasNewEvidence(product, study)));
              }).toList();
              return Column(
                children: [
                  if (view != 'all' && p.indexPending > 0)
                    Padding(
                      padding: const EdgeInsets.all(8),
                      child: Text(context.l10n.scoutPreparingSignals),
                    ),
                  if (p.stale)
                    Padding(
                      padding: const EdgeInsets.all(8),
                      child: Text(context.l10n.scoutStale),
                    ),
                  Expanded(
                    child: visible.isEmpty && p.next == null
                        ? Center(child: Text(context.l10n.scoutNoProducts))
                        : LayoutBuilder(
                            builder: (context, c) => GridView.builder(
                              key: PageStorageKey(
                                'scout-products-${ref.watch(scoutProductQueryProvider)}:${ref.watch(scoutSourceFilterProvider)}:$view',
                              ),
                              padding: const EdgeInsets.all(16),
                              gridDelegate:
                                  SliverGridDelegateWithFixedCrossAxisCount(
                                    crossAxisCount: ((c.maxWidth - 32) / 320)
                                        .floor()
                                        .clamp(1, 4),
                                    mainAxisExtent: 360,
                                    crossAxisSpacing: 14,
                                    mainAxisSpacing: 14,
                                  ),
                              itemCount:
                                  visible.length + (p.next == null ? 0 : 1),
                              itemBuilder: (context, i) => i == visible.length
                                  ? Center(
                                      child: TextButton(
                                        onPressed: () async {
                                          try {
                                            await ref
                                                .read(
                                                  scoutProductPageProvider
                                                      .notifier,
                                                )
                                                .more();
                                          } catch (e) {
                                            if (context.mounted) {
                                              ScaffoldMessenger.of(context)
                                                  .showSnackBar(
                                                    SnackBar(
                                                      content: Text(
                                                        context.errorMessage(
                                                          context
                                                              .l10n
                                                              .scoutLoadMore,
                                                          e,
                                                        ),
                                                      ),
                                                    ),
                                                  );
                                            }
                                          }
                                        },
                                        child: Text(context.l10n.scoutLoadMore),
                                      ),
                                    )
                                  : _ProductCard(
                                      product: visible[i],
                                      instance: p.instance,
                                    ),
                            ),
                          ),
                  ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}

class _ProductCard extends ConsumerWidget {
  final ScoutProduct product;
  final String instance;
  const _ProductCard({required this.product, required this.instance});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Card(
    margin: EdgeInsets.zero,
    clipBehavior: Clip.antiAlias,
    color: AppTheme.surface1,
    child: InkWell(
      onTap: () => _showProduct(context, product, instance),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(child: ScoutSourceBadge(sourceId: product.source)),
                if ((ref.watch(scoutStudiesProvider).value ??
                        <Map<String, dynamic>>[])
                    .any(
                      (s) =>
                          s['instance_id'] == instance &&
                          (s['product_id'] == product.id ||
                              (product.data['aliases'] as List? ?? []).contains(
                                s['product_id'],
                              )),
                    ))
                  Tooltip(
                    message: context.l10n.scoutStudies,
                    child: Icon(
                      Icons.bookmark_outline,
                      size: 16,
                      color: AppTheme.accentPrimary,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 14),
            Text(
              product.title,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 17,
                fontWeight: FontWeight.w600,
                color: AppTheme.textPrimary,
              ),
            ),
            if (product.description != null)
              Padding(
                padding: const EdgeInsets.only(top: 7),
                child: Text(
                  product.description!,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
                ),
              ),
            const SizedBox(height: 12),
            Text(
              _lead(context, product),
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 13,
                color: product.reasons.isEmpty
                    ? AppTheme.textSecondary
                    : AppTheme.accentPrimary,
              ),
            ),
            const Spacer(),
            Text(
              _price(context, product),
              style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
            ),
            const SizedBox(height: 8),
            Text(
              context.l10n.scoutGapsHelp,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 10.5, color: AppTheme.textTertiary),
            ),
            const SizedBox(height: 6),
            Row(
              children: [
                TextButton(
                  onPressed: () =>
                      _editResearch(context, ref, product, instance),
                  child: Text(context.l10n.scoutStudy),
                ),
                TextButton(
                  onPressed: () => _editResearch(
                    context,
                    ref,
                    product,
                    instance,
                    initialState: 'watching',
                  ),
                  child: Text(context.l10n.scoutStateWatching),
                ),
                const Spacer(),
                IconButton(
                  tooltip: context.l10n.scoutNotRelevant,
                  icon: const Icon(Icons.visibility_off_outlined, size: 17),
                  onPressed: () => _editResearch(
                    context,
                    ref,
                    product,
                    instance,
                    initialState: 'rejected',
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

String _price(BuildContext context, ScoutProduct p) {
  final amount = p.payload['price_cents'];
  return amount is num
      ? '${context.l10n.scoutKnownPrice}: ${p.payload['currency_code'] ?? ''} ${(amount / 100).toStringAsFixed(2)}'
      : context.l10n.scoutPriceUnknown;
}

String _trendNumbers(BuildContext context, ScoutProduct p, Map r) {
  final metric = '${r['metric']}';
  final from = r['from'], to = r['to'];
  if (metric == 'rank' && from is num && to is num) {
    final delta = from - to;
    return '${_fieldLabel(context, metric)} $from → $to · ${delta >= 0 ? context.l10n.scoutRankUp(delta.abs().toInt()) : context.l10n.scoutRankDown(delta.abs().toInt())}';
  }
  if (metric == 'price_cents' && from is num && to is num) {
    return '${p.payload['currency_code'] ?? ''} ${(from / 100).toStringAsFixed(2)} → ${(to / 100).toStringAsFixed(2)}';
  }
  return '${_fieldLabel(context, metric)} $from → $to';
}

String _lead(BuildContext context, ScoutProduct p) {
  if (p.reasons.isEmpty) return context.l10n.scoutNoSignal;
  final r = p.reasons.first;
  if (r['kind'] == 'new_scope') {
    return '${context.l10n.scoutNewScope}\n${_day('${r['to_at']}')}';
  }
  return '${r['days'] == 0 ? context.l10n.scoutRecent : '${r['days']}d'} · ${_trendNumbers(context, p, r)}\n${_day('${r['from_at']}')} – ${_day('${r['to_at']}')}';
}

Future<void> _showProduct(
  BuildContext context,
  ScoutProduct product,
  String instance,
) => showGeneralDialog<void>(
  context: context,
  barrierDismissible: true,
  barrierLabel: MaterialLocalizations.of(context).modalBarrierDismissLabel,
  barrierColor: Colors.black.withValues(alpha: .35),
  transitionDuration: Duration(
    milliseconds: MediaQuery.disableAnimationsOf(context) ? 0 : 220,
  ),
  transitionBuilder: (context, a, b, child) => SlideTransition(
    position: Tween<Offset>(
      begin: const Offset(1, 0),
      end: Offset.zero,
    ).animate(CurvedAnimation(parent: a, curve: Curves.easeOutCubic)),
    child: child,
  ),
  pageBuilder: (ctx, _, secondary) => Align(
    alignment: Alignment.centerRight,
    child: SizedBox(
      width: MediaQuery.sizeOf(ctx).width.clamp(0, 680),
      height: double.infinity,
      child: Material(
        color: AppTheme.surface1,
        child: SafeArea(
          child: _ProductDetail(
            product: product,
            instance: instance,
            onClose: () => Navigator.pop(ctx),
          ),
        ),
      ),
    ),
  ),
);

class _ProductDetail extends ConsumerWidget {
  final ScoutProduct product;
  final String instance;
  final VoidCallback onClose;
  const _ProductDetail({
    required this.product,
    required this.instance,
    required this.onClose,
  });
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final detail = ref.watch(scoutProductDetailProvider(product.id));
    Widget heading(String text) => Padding(
      padding: const EdgeInsets.only(top: 24, bottom: 10),
      child: Text(
        text,
        style: TextStyle(
          fontWeight: FontWeight.w600,
          fontSize: 16,
          color: AppTheme.textPrimary,
        ),
      ),
    );
    return Column(
      children: [
        Row(
          children: [
            const Spacer(),
            IconButton(
              tooltip: context.l10n.close,
              onPressed: onClose,
              icon: const Icon(Icons.close),
            ),
          ],
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(24, 4, 24, 24),
            children: [
              ScoutSourceBadge(sourceId: product.source),
              const SizedBox(height: 16),
              Text(
                product.title,
                style: TextStyle(
                  fontSize: 24,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
              if (product.description != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: SelectableText(product.description!),
                ),
              if ((ref.watch(scoutStudiesProvider).value ??
                      <Map<String, dynamic>>[])
                  .any(
                    (s) =>
                        s['instance_id'] == instance &&
                        (s['product_id'] == product.id ||
                            (product.data['aliases'] as List? ?? []).contains(
                              s['product_id'],
                            )) &&
                        scoutHasNewEvidence(product, s),
                  ))
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Text(
                    context.l10n.scoutResearchSourceChanged,
                    style: TextStyle(color: AppTheme.warning),
                  ),
                ),
              heading(context.l10n.scoutWhyNow),
              Text(_lead(context, product)),
              heading(context.l10n.scoutKnownPrice),
              Text(_price(context, product)),
              heading(context.l10n.scoutEvidenceGaps),
              Text(context.l10n.scoutGapsHelp),
              const SizedBox(height: 18),
              FilledButton.icon(
                onPressed: () => _editResearch(context, ref, product, instance),
                icon: const Icon(Icons.science_outlined),
                label: Text(context.l10n.scoutStudy),
              ),
              if (product.link != null)
                TextButton.icon(
                  onPressed: () => launchUrl(
                    Uri.parse(product.link!),
                    mode: LaunchMode.externalApplication,
                  ),
                  icon: const Icon(Icons.open_in_new),
                  label: Text(context.l10n.scoutOpenSource),
                ),
              heading(context.l10n.scoutWindow),
              for (final t in product.trends)
                Padding(
                  padding: const EdgeInsets.only(bottom: 14),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '${t['days']}d · ${_trendNumbers(context, product, t)}',
                        style: TextStyle(color: AppTheme.textPrimary),
                      ),
                      Text(
                        '${_exactTime('${t['from_at']}')} – ${_exactTime('${t['to_at']}')} · ${context.l10n.scoutCoverage} ${t['samples']}',
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                      if (t['series'] is List &&
                          (t['series'] as List).length > 1)
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 10),
                          child: SizedBox(
                            height: 60,
                            width: double.infinity,
                            child: CustomPaint(
                              painter: _ScoutTrendPainter(
                                (t['series'] as List)
                                    .whereType<Map>()
                                    .where((p) => p['value'] is num)
                                    .map(
                                      (p) => (
                                        DateTime.parse('${p['at']}')
                                            .millisecondsSinceEpoch
                                            .toDouble(),
                                        (p['value'] as num).toDouble(),
                                      ),
                                    )
                                    .toList(),
                                AppTheme.accentPrimary,
                              ),
                            ),
                          ),
                        ),
                      if (t['complete'] != true)
                        Text(
                          context.l10n.scoutHistoryIncomplete,
                          style: TextStyle(
                            fontSize: 11,
                            color: AppTheme.warning,
                          ),
                        ),
                    ],
                  ),
                ),
              heading(context.l10n.scoutHistory),
              detail.when(
                loading: () => const LinearProgressIndicator(),
                error: (e, _) => Text(context.l10n.scoutHistoryOffline),
                data: (data) => Column(
                  children: [
                    if (data['stale'] == true) Text(context.l10n.scoutStale),
                    for (final o
                        in (data['history'] as List? ?? []).whereType<Map>())
                      ExpansionTile(
                        title: Text(_exactTime('${o['taken_at']}')),
                        children: [
                          SelectableText(
                            const JsonEncoder.withIndent('  ')
                                .convert(o['payload']),
                          ),
                        ],
                      ),
                    if (data['next'] != null)
                      TextButton(
                        onPressed: () async {
                          try {
                            await ref
                                .read(
                                  scoutProductDetailProvider(product.id)
                                      .notifier,
                                )
                                .more();
                          } catch (e) {
                            if (context.mounted) {
                              ScaffoldMessenger.of(context).showSnackBar(
                                SnackBar(
                                  content: Text(
                                    context.errorMessage(
                                      context.l10n.scoutLoadMore,
                                      e,
                                    ),
                                  ),
                                ),
                              );
                            }
                          }
                        },
                        child: Text(context.l10n.scoutLoadMore),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _ScoutTrendPainter extends CustomPainter {
  final List<(double, double)> values;
  final Color color;
  _ScoutTrendPainter(this.values, this.color);
  @override
  void paint(Canvas canvas, Size size) {
    if (values.length < 2) return;
    final start = values.first.$1, end = values.last.$1;
    final min = values.map((v) => v.$2).reduce((a, b) => a < b ? a : b),
        max = values.map((v) => v.$2).reduce((a, b) => a > b ? a : b);
    final path = Path();
    for (final (i, v) in values.indexed) {
      final x = (v.$1 - start) / (end == start ? 1 : end - start) * size.width;
      final y = max == min
          ? size.height / 2
          : size.height - (v.$2 - min) / (max - min) * (size.height - 8) - 4;
      if (i == 0) {
        path.moveTo(x, y);
      } else {
        path.lineTo(x, y);
      }
    }
    canvas.drawPath(
      path,
      Paint()
        ..color = color
        ..strokeWidth = 2
        ..style = PaintingStyle.stroke,
    );
  }

  @override
  bool shouldRepaint(_ScoutTrendPainter old) =>
      old.values != values || old.color != color;
}
