import { productIdentity, titleOf, validateCollection } from './contracts';
/** Store layer over D1: same transactional semantics as the Python service. */

import type { Finding, Item } from "./diff";

export interface RunResult {
  source: string;
  skipped?: boolean;
  snapshot_id?: number;
  items?: number;
  findings?: number;
  /** 本轮因采集形状变化而重置基线（只更新快照、不产出 findings）。 */
  baseline_reset?: boolean;
  error?: string;
}

export async function upsertSource(
  db: D1Database,
  spec: { id: string; name: string; enabled: boolean; specHash: string; intervalMinutes: number }
): Promise<void> {
  // enabled/interval_minutes 是用户粘性的（管理页可改）：首次插入用 spec 默认值，
  // 之后代码 spec 只刷新 name/spec_hash，不覆盖用户修改。
  await db
    .prepare(
      `INSERT INTO scout_sources (id, name, enabled, spec_hash, interval_minutes, last_run_at)
       VALUES (?1, ?2, ?3, ?4, ?5, NULL)
       ON CONFLICT(id) DO UPDATE SET
         name = excluded.name, spec_hash = excluded.spec_hash`
    )
    .bind(spec.id, spec.name, spec.enabled ? 1 : 0, spec.specHash, spec.intervalMinutes)
    .run();
}

export async function getSetting(db: D1Database, key: string): Promise<string | null> {
  const row = await db
    .prepare("SELECT value FROM scout_settings WHERE key = ?")
    .bind(key)
    .first<{ value: string }>();
  return row?.value ?? null;
}

export async function setSetting(db: D1Database, key: string, value: string): Promise<void> {
  await db
    .prepare(
      `INSERT INTO scout_settings (key, value, updated_at) VALUES (?, ?, ?)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at`
    )
    .bind(key, value, new Date().toISOString())
    .run();
}

export async function getLastSnapshot(
  db: D1Database,
  sourceId: string
): Promise<{ id: number; data: Item[]; spec_hash: string | null } | null> {
  const row = await db
    .prepare(
      "SELECT id, data_json, spec_hash FROM scout_snapshots WHERE source_id = ? AND run_key IS NOT NULL ORDER BY id DESC LIMIT 1"
    )
    .bind(sourceId)
    .first<{ id: number; data_json: string; spec_hash: string | null }>();
  if (!row) return null;
  return { id: row.id, data: JSON.parse(row.data_json) as Item[], spec_hash: row.spec_hash };
}

export async function acquireSource(db: D1Database, source: string): Promise<string | null> {
  const token = crypto.randomUUID();
  const result = await db.prepare(`UPDATE scout_sources SET lease_token=?, lease_until=unixepoch()+120
    WHERE id=? AND (lease_until IS NULL OR lease_until<=unixepoch())`).bind(token, source).run();
  return result.meta.changes ? token : null;
}
export async function releaseSource(db: D1Database, source: string, token: string): Promise<void> {
  await db.prepare('UPDATE scout_sources SET lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?').bind(source, token).run();
}

export async function recordRun(
  db: D1Database, sourceId: string, takenAt: string, items: Item[], findings: Finding[], startedAt: string,
  token: string, specHash: string, interval: number,
): Promise<number> {
  validateCollection(items);
  const runKey = crypto.randomUUID();
  const normalized = items.map(item => ({ ...productIdentity(sourceId, item), title: titleOf(item), payload: item,
    legacy_product_id: sourceId.startsWith('github') && /^\d+$/.test(item.id) && item.full_name
      ? JSON.stringify(['github',String(item.full_name).toLowerCase()]) : null,
  }));
  const rows = JSON.stringify(normalized);
  // All statements including snapshot + observation publication execute in one D1 batch transaction.
  // A stale/expired collector fails the CHECK, rolling the entire batch back.
  const statements = [
    db.prepare(`INSERT INTO scout_commit_guard(valid) SELECT CASE WHEN EXISTS(
      SELECT 1 FROM scout_sources WHERE id=? AND lease_token=? AND lease_until>unixepoch()) THEN 1 ELSE 0 END`).bind(sourceId, token),
    db.prepare(`INSERT INTO scout_snapshots(source_id,taken_at,data_json,item_count,run_key,spec_hash,interval_minutes) VALUES(?,?,?,?,?,?,?)`)
      .bind(sourceId,takenAt,JSON.stringify(items),items.length,runKey,specHash,interval),
    db.prepare(`INSERT INTO scout_products(product_id,platform,platform_key,title,payload_json,first_seen_at,last_seen_at)
      SELECT json_extract(value,'$.product_id'),json_extract(value,'$.platform'),json_extract(value,'$.platform_key'),
        json_extract(value,'$.title'),json_extract(value,'$.payload'),?,? FROM json_each(?) WHERE 1
      ON CONFLICT(product_id) DO UPDATE SET title=excluded.title,payload_json=excluded.payload_json,last_seen_at=excluded.last_seen_at`)
      .bind(takenAt,takenAt,rows),
    db.prepare(`INSERT INTO scout_product_aliases(alias_id,product_id)
      SELECT json_extract(value,'$.legacy_product_id'),json_extract(value,'$.product_id') FROM json_each(?)
      WHERE json_extract(value,'$.legacy_product_id') IS NOT NULL
      ON CONFLICT(alias_id) DO UPDATE SET product_id=excluded.product_id`).bind(rows),
    // Old GitHub snapshots used full_name; move their bounded indexed history
    // to the immutable numeric identity when the next collection supplies it.
    db.prepare(`INSERT OR IGNORE INTO scout_observations
      SELECT o.snapshot_id,json_extract(j.value,'$.product_id'),o.source_id,o.item_key,o.taken_at,o.spec_hash,o.interval_minutes,o.payload_json
      FROM json_each(?) j JOIN scout_observations o ON o.product_id=json_extract(j.value,'$.legacy_product_id')`).bind(rows),
    db.prepare(`INSERT OR IGNORE INTO scout_product_sources
      SELECT json_extract(j.value,'$.product_id'),s.source_id,s.last_seen_at,s.present
      FROM json_each(?) j JOIN scout_product_sources s ON s.product_id=json_extract(j.value,'$.legacy_product_id')`).bind(rows),
    db.prepare(`UPDATE scout_products SET first_seen_at=MIN(first_seen_at,COALESCE((SELECT MIN(o.taken_at) FROM scout_observations o WHERE o.product_id=scout_products.product_id),first_seen_at))
      WHERE product_id IN (SELECT json_extract(value,'$.product_id') FROM json_each(?))`).bind(rows),
    db.prepare(`DELETE FROM scout_product_insights WHERE product_id IN (SELECT json_extract(value,'$.legacy_product_id') FROM json_each(?))`).bind(rows),
    db.prepare(`DELETE FROM scout_observations WHERE product_id IN (SELECT json_extract(value,'$.legacy_product_id') FROM json_each(?))`).bind(rows),
    db.prepare(`DELETE FROM scout_product_sources WHERE product_id IN (SELECT json_extract(value,'$.legacy_product_id') FROM json_each(?))`).bind(rows),
    db.prepare(`DELETE FROM scout_products WHERE product_id IN (SELECT json_extract(value,'$.legacy_product_id') FROM json_each(?))`).bind(rows),
    db.prepare('UPDATE scout_product_sources SET present=0 WHERE source_id=?').bind(sourceId),
    db.prepare(`INSERT INTO scout_product_sources(product_id,source_id,last_seen_at,present)
      SELECT json_extract(value,'$.product_id'),?,?,1 FROM json_each(?) WHERE 1
      ON CONFLICT(product_id,source_id) DO UPDATE SET last_seen_at=excluded.last_seen_at,present=1`).bind(sourceId,takenAt,rows),
    db.prepare(`INSERT INTO scout_observations(snapshot_id,product_id,source_id,item_key,taken_at,spec_hash,interval_minutes,payload_json)
      SELECT (SELECT id FROM scout_snapshots WHERE run_key=?),json_extract(value,'$.product_id'),?,
        json_extract(value,'$.payload.id'),?,?,?,json_extract(value,'$.payload') FROM json_each(?)`)
      .bind(runKey,sourceId,takenAt,specHash,interval,rows),
  ];
  // One JSON parameter keeps batch size bounded even for large collections.
  statements.push(db.prepare(`INSERT INTO scout_findings(source_id,snapshot_id,kind,item_key,payload_json,delta_json,detected_at)
    SELECT ?, (SELECT id FROM scout_snapshots WHERE run_key=?),json_extract(value,'$.kind'),json_extract(value,'$.item_key'),
      json_extract(value,'$.payload'),json_extract(value,'$.delta'),? FROM json_each(?)`)
    .bind(sourceId,runKey,takenAt,JSON.stringify(findings)));
  statements.push(db.prepare(`INSERT INTO scout_runs(source_id,status,started_at,finished_at,items_count,findings_count,error) VALUES(?,'succeeded',?,?,?,?,NULL)`)
    .bind(sourceId,startedAt,takenAt,items.length,findings.length));
  statements.push(db.prepare('UPDATE scout_sources SET last_run_at=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?').bind(takenAt,sourceId,token));
  statements.push(db.prepare('DELETE FROM scout_commit_guard'));
  const result = await db.batch(statements);
  return result[1].meta.last_row_id as number;
}

export async function recordFailure(
  db: D1Database,
  sourceId: string,
  startedAt: string,
  finishedAt: string,
  error: string
): Promise<void> {
  await db
    .prepare(
      "INSERT INTO scout_runs (source_id, status, started_at, finished_at, items_count, findings_count, error) VALUES (?, 'failed', ?, ?, 0, 0, ?)"
    )
    .bind(sourceId, startedAt, finishedAt, error.slice(0, 2000))
    .run();
}

export async function pruneSnapshots(
  db: D1Database,
  keepDays = 90,
  sampleEveryDays = 1
): Promise<number> {
  const cutoff = new Date(Date.now() - keepDays * 86400_000).toISOString();
  const rows = await db
    .prepare(
      "SELECT id, source_id, taken_at FROM scout_snapshots s WHERE taken_at < ? AND NOT EXISTS(SELECT 1 FROM scout_findings f WHERE f.snapshot_id=s.id) ORDER BY id DESC LIMIT 500"
    )
    .bind(cutoff)
    .all<{ id: number; source_id: string; taken_at: string }>();
  const keep = new Set<number>();
  const seen = new Map<string, number>();
  for (const r of rows.results) {
    const day = r.taken_at.slice(0, 10);
    // sample: keep the newest snapshot whose day index is divisible by sampleEveryDays
    const dayIndex = Math.floor(Date.parse(day) / 86400_000);
    if (dayIndex % sampleEveryDays === 0 && !seen.has(r.source_id + day)) {
      seen.set(r.source_id + day, 1);
      keep.add(r.id);
    }
  }
  const kill = rows.results.filter((r) => !keep.has(r.id));
  if (kill.length === 0) return 0;
  const ids = kill.map((r) => r.id);
  await db
    .prepare(
      `DELETE FROM scout_snapshots WHERE id IN (${ids.map(() => "?").join(",")})`
    )
    .bind(...ids)
    .run();
  return kill.length;
}

/** Bounded, resumable backfill. Legacy rows remain explicitly non-comparable. */
export async function backfillObservations(db:D1Database, after=0):Promise<{next:number;count:number}> {
 const snapshots=await db.prepare(`SELECT s.*,COALESCE(s.interval_minutes,720) AS interval_minutes FROM scout_snapshots s
   WHERE s.id>? AND s.run_key IS NOT NULL ORDER BY s.id LIMIT 20`).bind(after).all<{id:number;source_id:string;taken_at:string;data_json:string;spec_hash:string|null;interval_minutes:number}>();
 for(const snapshot of snapshots.results){
   const items=JSON.parse(snapshot.data_json) as Item[];
   try{validateCollection(items)}catch{continue}
   const normalized=items.map(item=>({...productIdentity(snapshot.source_id,item),title:titleOf(item),payload:item}));
   const aliases=await db.prepare('SELECT * FROM scout_product_aliases WHERE alias_id IN (SELECT value FROM json_each(?))').bind(JSON.stringify(normalized.map(i=>i.product_id))).all<{alias_id:string;product_id:string}>();
   const rows=JSON.stringify(normalized.map(item=>{
     const alias=aliases.results.find(a=>a.alias_id===item.product_id);
     return alias ? {...item,product_id:alias.product_id,platform_key:JSON.parse(alias.product_id)[1]} : item;
   }));
   await db.batch([
    db.prepare(`INSERT INTO scout_products(product_id,platform,platform_key,title,payload_json,first_seen_at,last_seen_at)
      SELECT json_extract(value,'$.product_id'),json_extract(value,'$.platform'),json_extract(value,'$.platform_key'),json_extract(value,'$.title'),json_extract(value,'$.payload'),?,? FROM json_each(?) WHERE 1
      ON CONFLICT(product_id) DO UPDATE SET first_seen_at=MIN(first_seen_at,excluded.first_seen_at),
       title=CASE WHEN excluded.last_seen_at>last_seen_at THEN excluded.title ELSE title END,
       payload_json=CASE WHEN excluded.last_seen_at>last_seen_at THEN excluded.payload_json ELSE payload_json END,
       last_seen_at=MAX(last_seen_at,excluded.last_seen_at)`).bind(snapshot.taken_at,snapshot.taken_at,rows),
    db.prepare(`INSERT INTO scout_observations(snapshot_id,product_id,source_id,item_key,taken_at,spec_hash,interval_minutes,payload_json)
     SELECT ?,json_extract(value,'$.product_id'),?,json_extract(value,'$.payload.id'),?,?,?,json_extract(value,'$.payload') FROM json_each(?) WHERE 1
     ON CONFLICT(snapshot_id,product_id) DO NOTHING`).bind(snapshot.id,snapshot.source_id,snapshot.taken_at,snapshot.spec_hash??'legacy-unknown',snapshot.interval_minutes,rows),
    db.prepare(`INSERT INTO scout_product_sources(product_id,source_id,last_seen_at,present)
     SELECT json_extract(value,'$.product_id'),?,?,0 FROM json_each(?) WHERE 1
     ON CONFLICT(product_id,source_id) DO UPDATE SET last_seen_at=MAX(last_seen_at,excluded.last_seen_at)`).bind(snapshot.source_id,snapshot.taken_at,rows),
   ]);
 }
 return {next:snapshots.results.at(-1)?.id??after,count:snapshots.results.length};
}
