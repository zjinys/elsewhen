/** Worker entry: cron tick (collect due sources) + HTTP API + /admin page. */

import type { Env } from "../worker-configuration";
import { adminPage } from "./admin";
import { productRoutes } from './products';
import { refreshInsights } from './insights';
import { findingPage } from './feed';
import { validateCollection } from './contracts';
import { diffItems } from "./diff";
import { SOURCES, getSource } from "./sources";
import {
  backfillObservations,
  acquireSource,
  releaseSource,
  getLastSnapshot,
  getSetting,
  pruneSnapshots,
  recordFailure,
  recordRun,
  setSetting,
  upsertSource,
  type RunResult,
} from "./store";

const TICK_MINUTES = 10; // cron runs every 10 min; sources gate by their own interval

function nowIso(): string {
  return new Date().toISOString();
}

function intervalOverride(env: Env, sourceId: string, fallback: number): number {
  try {
    const o = JSON.parse(env.INTERVAL_OVERRIDES ?? "{}") as Record<string, number>;
    return typeof o[sourceId] === "number" ? o[sourceId] : fallback;
  } catch {
    return fallback;
  }
}

async function runDueSources(env: Env, opts: { forceSource?: string } = {}): Promise<RunResult[]> {
  const results: RunResult[] = [];
  for (const spec of SOURCES) {
    if (!spec.enabled || (opts.forceSource && opts.forceSource !== spec.id)) continue;
    let interval = intervalOverride(env, spec.id, spec.intervalMinutes);
    await upsertSource(env.DB, {
      id: spec.id,
      name: spec.name,
      enabled: spec.enabled,
      specHash: spec.specHash,
      intervalMinutes: interval,
    });

    if (opts.forceSource !== spec.id) {
      // 读用户粘性的开关/间隔（管理页可改，代码 spec 不覆盖）
      const srcRow = await env.DB
        .prepare("SELECT enabled, interval_minutes, last_run_at FROM scout_sources WHERE id = ?")
        .bind(spec.id)
        .first<{ enabled: number; interval_minutes: number; last_run_at: string | null }>();
      if (!srcRow || srcRow.enabled !== 1) continue; // 管理页关掉的源跳过
      const effInterval = srcRow.interval_minutes || interval;
      interval = effInterval;
      if (srcRow.last_run_at) {
        const nextDue = Date.parse(srcRow.last_run_at) + effInterval * 60_000;
        if (Date.now() < nextDue) continue; // not due yet
      }
    }

    const lease = await acquireSource(env.DB, spec.id);
    if (!lease) continue;
    const startedAt = nowIso();
    try {
      const scopeBefore=spec.id.startsWith('cws') ? await getSetting(env.DB,'cws_extra_ids')??'' : '';
      const configured=await env.DB.prepare('SELECT interval_minutes FROM scout_sources WHERE id=?').bind(spec.id).first<{interval_minutes:number}>();
      interval=configured?.interval_minutes ?? interval;
      const items = await spec.fetch(env, { http: fetch, env });
      validateCollection(items);
      const prev = await getLastSnapshot(env.DB, spec.id);
      const takenAt = nowIso();
      // 形状变了 = 基线无效：itemHash 基于字段值，字段清单一旦增减，每条
      // 的 hash 都会变，照常比对会把整个源刷成 changed。这里只更新快照、
      // 不产出 findings——采集中间态不该污染「检出了什么变化」。
      const scopeAfter=spec.id.startsWith('cws') ? await getSetting(env.DB,'cws_extra_ids')??'' : '';
      if(scopeBefore!==scopeAfter)throw new Error('collection_scope_changed_during_fetch');
      const collectionShape=spec.specHash + (spec.id.startsWith('cws') ? ':'+scopeBefore : '');
      const baselineReset = !prev || prev.spec_hash !== collectionShape;
      const findings = baselineReset
        ? []
        : diffItems(items, prev ? prev.data : null);
      const snapshotId = await recordRun(env.DB, spec.id, takenAt, items, findings, startedAt, lease, collectionShape, interval);
      try { await refreshInsights(env.DB); }
      catch(e) { console.error(JSON.stringify({event:'insight_refresh_failed',source:spec.id,error:String(e)})); }
      results.push({
        source: spec.id,
        snapshot_id: snapshotId,
        items: items.length,
        findings: findings.length,
        ...(baselineReset ? { baseline_reset: true } : {}),
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      await recordFailure(env.DB, spec.id, startedAt, nowIso(), msg);
      results.push({ source: spec.id, error: msg });
    } finally {
      await releaseSource(env.DB, spec.id, lease);
    }
  }
  return results;
}

const json = (data: unknown, status = 200): Response =>
  new Response(JSON.stringify(data), {
    status,
    headers: { "content-type": "application/json; charset=utf-8" },
  });

function checkAuth(request: Request, env: Env): Response | null {
  const token = (env.SCOUT_TOKEN ?? "").trim();
  if (!token) return null; // unset => open (dev only)
  const auth = request.headers.get("Authorization") ?? "";
  if (auth !== `Bearer ${token}`) {
    return json({ error: "unauthorized" }, 401);
  }
  return null;
}

async function handleApi(request: Request, env: Env, url: URL): Promise<Response> {
  const path = url.pathname;

  if (path === "/health") return json({ status: "ok" });

  const unauthorized = checkAuth(request, env);
  if (unauthorized) return unauthorized;

  if (path === "/sources" && request.method === "GET") {
    const rows = await env.DB
      .prepare("SELECT * FROM scout_sources WHERE enabled = 1 ORDER BY id")
      .all();
    return json(rows.results);
  }

  if (path === '/v1/findings' && request.method === 'GET') {
    try { return json(await findingPage(env.DB, url)); }
    catch (e) { if (e instanceof Error && e.message === 'invalid_cursor') return json({error:'invalid_cursor'},400); throw e; }
  }
  if (path.startsWith('/v1/products') && request.method==='GET') return productRoutes(env.DB,url);
  if (path === "/findings" && request.method === "GET") {
    const kind = url.searchParams.get("kind");
    if (kind && !["new", "changed", "gone"].includes(kind)) {
      return json({ error: "kind must be new|changed|gone" }, 400);
    }
    const source = url.searchParams.get("source");
    const since = url.searchParams.get("since");
    const limit = Math.min(Number(url.searchParams.get("limit") ?? 200) || 200, 1000);
    let sql = "SELECT * FROM scout_findings WHERE 1=1";
    const binds: unknown[] = [];
    if (source) {
      sql += " AND source_id = ?";
      binds.push(source);
    }
    if (kind) {
      sql += " AND kind = ?";
      binds.push(kind);
    }
    if (since) {
      sql += " AND detected_at >= ?";
      binds.push(since);
    }
    sql += " ORDER BY id DESC LIMIT ?";
    binds.push(limit);
    const rows = await env.DB.prepare(sql)
      .bind(...binds)
      .all();
    return json(
      rows.results.map((r: Record<string, unknown>) => ({
        ...r,
        payload: JSON.parse(r.payload_json as string),
        // delta 只在 changed 上存在；转发给消费端才能显示「+34 stars」。
        delta: r.delta_json ? JSON.parse(r.delta_json as string) : undefined,
        payload_json: undefined,
        delta_json: undefined,
      }))
    );
  }

  const findingMatch = path.match(/^\/findings\/(\d+)$/);
  if (findingMatch && request.method === "GET") {
    const id = Number(findingMatch[1]);
    const f = await env.DB.prepare("SELECT * FROM scout_findings WHERE id = ?")
      .bind(id)
      .first<Record<string, unknown>>();
    if (!f) return json({ error: "finding not found" }, 404);
    const before=Math.min(Number(url.searchParams.get('before')??Number.MAX_SAFE_INTEGER),Number.MAX_SAFE_INTEGER);
    const rows=await env.DB.prepare(`SELECT snapshot_id,taken_at,payload_json FROM scout_observations WHERE source_id=? AND item_key=? AND snapshot_id<? ORDER BY snapshot_id DESC LIMIT 100`)
      .bind(f.source_id,f.item_key,before).all<{snapshot_id:number;taken_at:string;payload_json:string}>();
    return json({...f,payload:JSON.parse(f.payload_json as string),history:rows.results.map(r=>({snapshot_id:r.snapshot_id,taken_at:r.taken_at,item:JSON.parse(r.payload_json)})),next:rows.results.length===100?rows.results.at(-1)!.snapshot_id:null});
  }

  const snapMatch = path.match(/^\/snapshots\/(\d+)$/);
  if (snapMatch && request.method === "GET") {
    const s = await env.DB.prepare("SELECT * FROM scout_snapshots WHERE id = ?")
      .bind(Number(snapMatch[1]))
      .first<Record<string, unknown>>();
    if (!s) return json({ error: "snapshot not found" }, 404);
    return json({ ...s, data: JSON.parse(s.data_json as string), data_json: undefined });
  }

  if (path === "/probe" && request.method === "GET") {
    // 源开发调试：从 Workers 出口实测任意 URL 的可达性/形状。token 保护。
    const target = url.searchParams.get("url");
    if (!target || !/^https:\/\//.test(target)) {
      return json({ error: "url must be https" }, 400);
    }
    const method = url.searchParams.get("method") ?? "GET";
    const headers: Record<string, string> = {
      "User-Agent":
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36",
      Accept: "text/html,application/json,application/xhtml+xml,*/*;q=0.8",
      "Accept-Language": "en-US,en;q=0.9",
    };
    const extra = url.searchParams.get("headers");
    if (extra) Object.assign(headers, JSON.parse(extra) as Record<string, string>);
    const started = Date.now();
    try {
      const resp = await fetch(target, { method, headers, redirect: "follow" });
      const body = await resp.text();
      return json({
        url: target,
        status: resp.status,
        bytes: body.length,
        ms: Date.now() - started,
        finalUrl: resp.url,
        contentType: resp.headers.get("content-type"),
        head: url.searchParams.get("full") ? body : body.slice(0, 1500),
      });
    } catch (e) {
      return json(
        { url: target, error: e instanceof Error ? e.message : String(e), ms: Date.now() - started },
        200
      );
    }
  }

  if (path === "/runs" && request.method === "GET") {
    const source = url.searchParams.get("source");
    const limit = Math.min(Number(url.searchParams.get("limit") ?? 50) || 50, 500);
    const rows = source
      ? await env.DB
          .prepare("SELECT * FROM scout_runs WHERE source_id = ? ORDER BY id DESC LIMIT ?")
          .bind(source, limit)
          .all()
      : await env.DB.prepare("SELECT * FROM scout_runs ORDER BY id DESC LIMIT ?")
          .bind(limit)
          .all();
    return json(rows.results);
  }

  // ---------- admin API ----------

  // 全量源列表（含禁用的）+ 最近一轮状态
  if (path === "/admin/sources" && request.method === "GET") {
    const rows = await env.DB
      .prepare(
        `SELECT s.*, r.status AS last_status, r.error AS last_error,
                r.items_count AS last_items, r.started_at AS last_started
         FROM scout_sources s
         LEFT JOIN scout_runs r ON r.id = (
           SELECT id FROM scout_runs WHERE source_id = s.id ORDER BY id DESC LIMIT 1)
         ORDER BY s.id`
      )
      .all();
    return json(rows.results);
  }

  const srcMatch = path.match(/^\/admin\/sources\/([\w-]+)$/);
  if (srcMatch && request.method === "PATCH") {
    const body = (await request.json()) as { enabled?: boolean; interval_minutes?: number };
    const sets: string[] = [];
    const binds: unknown[] = [];
    if (typeof body.enabled === "boolean") {
      sets.push("enabled = ?");
      binds.push(body.enabled ? 1 : 0);
    }
    if (typeof body.interval_minutes === "number" && body.interval_minutes >= 15) {
      sets.push("interval_minutes = ?");
      binds.push(Math.floor(body.interval_minutes));
    }
    if (sets.length === 0) return json({ error: "nothing to update" }, 400);
    binds.push(srcMatch[1]);
    const r = await env.DB
      .prepare(`UPDATE scout_sources SET ${sets.join(", ")} WHERE id = ?`)
      .bind(...binds)
      .run();
    if (r.meta.changes === 0) return json({ error: "source not found" }, 404);
    return json({ ok: true });
  }

  // 手动触发单个源（无视间隔与 enabled；强制跑一轮）
  if (path === "/admin/trigger" && request.method === "POST") {
    const body = (await request.json().catch(() => ({}))) as { source?: string };
    if (!body.source) return json({ error: "source required" }, 400);
    if (!getSource(body.source)) return json({ error: "unknown source" }, 404);
    const results = await runDueSources(env, { forceSource: body.source });
    return json(results);
  }

  // CWS 清单扩展 ID（settings 表，重启不丢）
  if (path === "/admin/settings/cws_ids" && request.method === "GET") {
    return json({ value: (await getSetting(env.DB, "cws_extra_ids")) ?? "" });
  }
  if (path === "/admin/settings/cws_ids" && request.method === "PUT") {
    const body = (await request.json()) as { value: string };
    const ids = body.value
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter((s) => /^[a-p]{32}$/.test(s));
    await setSetting(env.DB, "cws_extra_ids", ids.join(","));
    return json({ ok: true, ids });
  }

  return json({ error: "not found" }, 404);
}

export default {
  async scheduled(
    _controller: ScheduledController,
    env: Env,
    ctx: ExecutionContext
  ): Promise<void> {
    ctx.waitUntil(
      (async () => {
        const results = await runDueSources(env);
        const backfill=await backfillObservations(env.DB,Number(await getSetting(env.DB,'observation_backfill_cursor')??0));
        await setSetting(env.DB,'observation_backfill_cursor',String(backfill.next));
        try { await refreshInsights(env.DB); }
        catch(e) { console.error(JSON.stringify({event:'insight_refresh_failed',error:String(e)})); }
        for (const r of results) {
          if (r.error) console.error(JSON.stringify({event:"scout_collection_failed",source:r.source,error:r.error}));
          else console.log(JSON.stringify({event:"scout_collection_succeeded",source:r.source,items:r.items,findings:r.findings}));
        }
        // occasional housekeeping: prune on first tick of the UTC day
        if (new Date().getUTCHours() === 0 && new Date().getUTCMinutes() < TICK_MINUTES) {
          const deleted = await pruneSnapshots(env.DB);
          console.log(`scout: pruned ${deleted} old snapshots`);
        }
      })()
    );
  },

  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    try {
      if (url.pathname === "/admin" || url.pathname === "/admin/") {
        return adminPage(env);
      }
      return await handleApi(request, env, url);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return json({ error: msg }, 500);
    }
  },
};

// keep import used (tree-shake guard for getSource if needed later)
void getSource;
