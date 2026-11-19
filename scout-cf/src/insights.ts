import { METRICS } from './contracts';
import { trends, type Observation } from './trends';

type Reason = Record<string, unknown>;
export function buildInsight(history: Observation[], asOf: string, entrant?: Record<string, unknown>) {
  const cutoff = Date.parse(asOf) - 62 * 86400000;
  const recent = history.filter(o => Date.parse(o.taken_at) >= cutoff);
  const windows = [...trends(recent.length ? recent : history.slice(0, 1), asOf, 7),
    ...trends(recent.length ? recent : history.slice(0, 1), asOf, 30)] as Reason[];
  const reasons: Reason[] = windows.filter(t => t.complete &&
    (t.sustained || (t.metric === 'rank' && Number(t.rank_improvement) >= 5)))
    .map(t => ({ kind: t.metric === 'rank' ? 'rank_rise' : 'sustained_growth', ...t }));
  let changedAt = history.at(-1)?.taken_at ?? asOf;
  const sources = [...new Set(history.map(o => o.source_id))];
  for (const source of sources) {
    const rows = history.filter(o => o.source_id === source).sort((a,b) => b.taken_at.localeCompare(a.taken_at));
    for (let i = 0; i < rows.length - 1; i++) {
      const current = rows[i], previous = rows[i+1];
      if (current.spec_hash !== previous.spec_hash || current.spec_hash === 'legacy-unknown') continue;
      const a = JSON.parse(previous.payload_json), b = JSON.parse(current.payload_json);
      if (Object.keys(METRICS).some(key => typeof a[key] === 'number' && typeof b[key] === 'number' && a[key] !== b[key])) {
        if (current.taken_at > changedAt) changedAt = current.taken_at;
      }
      if (!reasons.some(r => r.kind === 'price_change' && r.source_id === source) &&
          Date.parse(current.taken_at) >= Date.parse(asOf) - 7 * 86400000 &&
          typeof a.price_cents === 'number' && typeof b.price_cents === 'number' &&
          a.price_cents !== b.price_cents && typeof a.currency_code === 'string' && !!a.currency_code && a.currency_code === b.currency_code) {
        reasons.unshift({ kind:'price_change', source_id:source, metric:'price_cents', days:0,
          from:a.price_cents, to:b.price_cents, from_at:previous.taken_at, to_at:current.taken_at,
          complete:true, snapshot_ids:[previous.snapshot_id,current.snapshot_id], algorithm:'price-change-v1' });
      }
    }
  }
  if (entrant && Date.parse(String(entrant.detected_at)) >= Date.parse(asOf) - 7 * 86400000) {
    reasons.push({kind:'new_scope', days:0, to_at:entrant.detected_at,
      snapshot_ids:[entrant.snapshot_id], algorithm:'scope-entry-v1', complete:true});
    if (String(entrant.detected_at) > changedAt) changedAt = String(entrant.detected_at);
  }
  // Rank/growth need fresh samples. Discrete price/scope events stay visible for 7 days.
  const expiry = reasons.map(r => Date.parse(String(r.to_at)) +
    (r.kind === 'price_change' || r.kind === 'new_scope' ? 7 * 86400000 :
      (history.find(o => o.source_id === r.source_id)?.interval_minutes ?? 720) * 90000));
  for(const [i,reason] of reasons.entries())reason.valid_until=new Date(expiry[i]).toISOString();
  return {windows, reasons, changedAt,
    validUntil:new Date(Math.max(Date.parse(asOf), ...expiry)).toISOString()};
}

/** Bounded rebuild queue; a failed refresh never invalidates a committed collection. */
export async function refreshInsights(db:D1Database, asOf=new Date().toISOString(), limit=40) {
  const pending=await db.prepare(`SELECT p.product_id,
    (SELECT MAX(o.snapshot_id) FROM scout_observations o WHERE o.product_id=p.product_id) AS latest
    FROM scout_products p WHERE NOT EXISTS(SELECT 1 FROM scout_product_insights i
      WHERE i.product_id=p.product_id AND i.snapshot_id=(SELECT MAX(o.snapshot_id) FROM scout_observations o WHERE o.product_id=p.product_id))
    ORDER BY COALESCE((SELECT MAX(i.as_of) FROM scout_product_insights i WHERE i.product_id=p.product_id),'') ASC,p.product_id DESC LIMIT ?`).bind(Math.min(limit,40)).all<{product_id:string;latest:number}>();
  if(!pending.results.length)return 0;
  const histories=await db.batch<Observation>(pending.results.map(p=>db.prepare(
    'SELECT * FROM scout_observations WHERE product_id=? AND taken_at<=? ORDER BY taken_at DESC,snapshot_id DESC LIMIT 800').bind(p.product_id,asOf)));
  const entrants=await db.prepare(`SELECT o.product_id,MAX(f.detected_at) AS detected_at,MAX(f.snapshot_id) AS snapshot_id
    FROM scout_observations o JOIN scout_findings f ON f.snapshot_id=o.snapshot_id AND f.item_key=o.item_key
    WHERE o.product_id IN (SELECT value FROM json_each(?)) AND f.kind='new' AND f.detected_at<=? GROUP BY o.product_id`)
    .bind(JSON.stringify(pending.results.map(p=>p.product_id)),asOf).all();
  const writes=[];
  for(const [index,p] of pending.results.entries()) {
    const rows=histories[index].results;if(!rows.length || rows[0].snapshot_id!==p.latest)continue;
    const insight=buildInsight(rows,asOf,entrants.results.find(e=>e.product_id===p.product_id));
    writes.push(db.prepare(`INSERT INTO scout_product_insights(product_id,snapshot_id,as_of,changed_at,observed_at,valid_until,reasons_json)
      VALUES(?,?,?,?,?,?,?) ON CONFLICT(product_id,snapshot_id) DO NOTHING`)
      .bind(p.product_id,p.latest,asOf,insight.changedAt,rows[0].taken_at,insight.validUntil,JSON.stringify(insight.reasons)));
  }
  if(writes.length)await db.batch(writes);
  return writes.length;
}
