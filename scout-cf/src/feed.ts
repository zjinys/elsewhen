/** Fixed upper bound and monotonic IDs, independent of client/server clock skew. */
export async function findingPage(db: D1Database, url: URL): Promise<unknown> {
  const integer = (name: string, fallback: number): number => {
    const value = url.searchParams.get(name);
    if (value === null) return fallback;
    if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) throw new Error('invalid_cursor');
    return Number(value);
  };
  const after = integer('after', 0);
  const head = await db.prepare('SELECT COALESCE(MAX(id),0) AS id FROM scout_findings').first<{ id: number }>();
  const upper = integer('upper', head!.id);
  if (after > upper || upper > head!.id) throw new Error('invalid_cursor');
  const limit = Math.max(1, Math.min(integer('limit', 250), 500));
  const rows = await db.prepare('SELECT * FROM scout_findings WHERE id>? AND id<=? ORDER BY id LIMIT ?')
    .bind(after, upper, limit).all<Record<string, unknown>>();
  const next = rows.results.length === limit ? Number(rows.results.at(-1)!.id) : upper;
  const instance = await db.prepare('SELECT instance_id FROM scout_instance WHERE id=1').first<{ instance_id: string }>();
  return { version: 1, instance_id: instance!.instance_id, upper, next, has_more: next < upper,
    items: rows.results.map(r => ({ ...r, payload: JSON.parse(String(r.payload_json)), delta: r.delta_json ? JSON.parse(String(r.delta_json)) : undefined, payload_json: undefined, delta_json: undefined })) };
}
