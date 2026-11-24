import { trends, type Observation } from './trends';
import { buildInsight } from './insights';
import { affiliateEvidence } from './affiliate';
export { trends } from './trends';
export type { Observation } from './trends';

export async function productRoutes(db:D1Database,url:URL):Promise<Response> {
  const send=(value:unknown,status=200)=>Response.json(value,{status});
  const requestedAsOf=url.searchParams.get('as_of');
  const asOf=requestedAsOf && Number.isFinite(Date.parse(requestedAsOf)) ? requestedAsOf : new Date().toISOString();
  if(url.pathname==='/v1/products/detail') {
    const id=url.searchParams.get('id') ?? '';
    const product=await db.prepare('SELECT * FROM scout_products WHERE product_id=?').bind(id).first();
    if(!product) return send({error:'not_found'},404);
    const rawBefore=Number(url.searchParams.get('before') || Number.MAX_SAFE_INTEGER);
    if(!Number.isFinite(rawBefore) || rawBefore<=0 || !Number.isInteger(rawBefore))return send({error:'invalid_cursor'},400);
    const before=Math.min(rawBefore,Number.MAX_SAFE_INTEGER);
    const history=await db.prepare('SELECT * FROM scout_observations WHERE product_id=? AND snapshot_id<? ORDER BY snapshot_id DESC LIMIT 100').bind(id,before).all<Observation>();
    const instance=await db.prepare('SELECT instance_id FROM scout_instance WHERE id=1').first();
    const affiliate=(await affiliateEvidence(db,[id])).get(id) ?? null;
    return send({instance_id:instance!.instance_id,...product,affiliate,payload:JSON.parse(String(product.payload_json)),payload_json:undefined,history:history.results.map(o=>({...o,payload:JSON.parse(o.payload_json),payload_json:undefined})),next:history.results.length===100?history.results.at(-1)!.snapshot_id:null});
  }
  const mode=url.searchParams.get('view') ?? 'all';
  if(!['all','signals','growth','price'].includes(mode))return send({error:'invalid_view'},400);
  const source=url.searchParams.get('source') ?? '';
  const q=url.searchParams.get('q') ?? '';
  const before=url.searchParams.get('before');
  let cursor:[string,string]=[asOf,'\uffff'];
  if(before){try{const c=JSON.parse(before);if(!Array.isArray(c)||c.length!==2||typeof c[0]!=='string'||typeof c[1]!=='string'||!Number.isFinite(Date.parse(c[0])))return send({error:'invalid_cursor'},400);cursor=c as [string,string];}catch{return send({error:'invalid_cursor'},400);}}
  if(q.length>200)return send({error:'query_too_long'},400);
  const historyQuery=mode==='all' ? `SELECT p.*, (SELECT MAX(o.taken_at) FROM scout_observations o WHERE o.product_id=p.product_id AND o.taken_at<=?) AS observed_at FROM scout_products p`
    : `SELECT p.*,i.changed_at AS observed_at FROM scout_products p JOIN scout_product_insights i
        ON i.product_id=p.product_id AND i.rowid=(SELECT z.rowid FROM scout_product_insights z WHERE z.product_id=p.product_id AND z.as_of<=? ORDER BY z.as_of DESC,z.snapshot_id DESC LIMIT 1)
       WHERE i.valid_until>=? AND EXISTS(SELECT 1 FROM json_each(i.reasons_json) WHERE json_extract(value,'$.valid_until')>=?)
        ${mode==='growth' ? "AND EXISTS(SELECT 1 FROM json_each(i.reasons_json) WHERE json_extract(value,'$.kind') IN ('sustained_growth','rank_rise') AND json_extract(value,'$.valid_until')>=?)" : mode==='price' ? "AND EXISTS(SELECT 1 FROM json_each(i.reasons_json) WHERE json_extract(value,'$.kind')='price_change' AND json_extract(value,'$.valid_until')>=?)" : ''}`;
  const products=await db.prepare(`WITH historical AS (${historyQuery}) SELECT * FROM historical p WHERE observed_at IS NOT NULL AND (observed_at<? OR (observed_at=? AND product_id<?))
    AND (?='' OR (SELECT COALESCE(json_extract(z.payload_json,'$.name'),json_extract(z.payload_json,'$.title'),json_extract(z.payload_json,'$.full_name'),z.item_key) FROM scout_observations z WHERE z.product_id=p.product_id AND z.taken_at<=? ORDER BY z.taken_at DESC,z.snapshot_id DESC LIMIT 1) LIKE '%' || ? || '%')
    AND (?='' OR EXISTS(SELECT 1 FROM scout_product_sources s WHERE s.product_id=p.product_id AND s.source_id=? AND EXISTS(SELECT 1 FROM scout_observations x WHERE x.product_id=p.product_id AND x.source_id=s.source_id AND x.taken_at<=?)))
    ORDER BY observed_at DESC,product_id DESC LIMIT 40`)
    .bind(...(mode==='all'?[asOf]:(mode==='signals'?[asOf,asOf,asOf]:[asOf,asOf,asOf,asOf])),cursor[0],cursor[0],cursor[1],q,asOf,q,source,source,asOf).all<Record<string,unknown>>();
  // Batch bounded index reads to avoid 80+ serial D1 round trips per page.
  const histories=products.results.length ? await db.batch<Observation>(products.results.map(p =>
    db.prepare(`SELECT * FROM scout_observations WHERE product_id=? AND taken_at<=? ORDER BY taken_at DESC LIMIT 800`)
      .bind(p.product_id,asOf))) : [];
  const members=await db.prepare(`SELECT s.product_id,s.source_id,
    (SELECT MAX(o.taken_at) FROM scout_observations o WHERE o.product_id=s.product_id AND o.source_id=s.source_id AND o.taken_at<=?) AS last_seen_at
    FROM scout_product_sources s WHERE s.product_id IN (SELECT value FROM json_each(?)) ORDER BY source_id`).bind(asOf,JSON.stringify(products.results.map(p=>p.product_id))).all();
  const entrants=await db.prepare(`SELECT o.product_id,MAX(f.detected_at) AS detected_at,MAX(f.snapshot_id) AS snapshot_id
    FROM scout_observations o JOIN scout_findings f ON f.snapshot_id=o.snapshot_id AND f.item_key=o.item_key
    WHERE o.product_id IN (SELECT value FROM json_each(?)) AND f.kind='new' AND f.detected_at>=? AND f.detected_at<=?
    GROUP BY o.product_id`).bind(JSON.stringify(products.results.map(p=>p.product_id)),new Date(Date.parse(asOf)-7*86400000).toISOString(),asOf).all();
  const aliases=await db.prepare('SELECT alias_id,product_id FROM scout_product_aliases WHERE product_id IN (SELECT value FROM json_each(?))').bind(JSON.stringify(products.results.map(p=>p.product_id))).all();
  const affiliates=await affiliateEvidence(db,products.results.map(p=>String(p.product_id)));
  const items=[];
  for(const [index,p] of products.results.entries()) {
    // bounded per product/source history, not whole-source snapshot JSON scans
    const history=histories[index];
    const observed=history.results[0];
    if(observed){p.payload_json=observed.payload_json;p.last_seen_at=observed.taken_at; const payload=JSON.parse(observed.payload_json);p.title=payload.name??payload.title??payload.full_name??p.title;}
    const entrant=entrants.results.find(e=>e.product_id===p.product_id);
    const insight=buildInsight(history.results,asOf,entrant);
    const windows=insight.windows;
    const reasons=insight.reasons.filter(r=>mode!=='growth'&&mode!=='price' ||
      (mode==='growth'?['sustained_growth','rank_rise'].includes(String(r.kind)):r.kind==='price_change'));
    items.push({...p,affiliate:affiliates.get(String(p.product_id)) ?? null,aliases:aliases.results.filter(a=>a.product_id===p.product_id).map(a=>a.alias_id),payload:JSON.parse(String(p.payload_json)),payload_json:undefined,sources:members.results.filter(s=>s.product_id===p.product_id && s.last_seen_at),trends:windows,reasons,
      gaps:['buyer_unverified','sales_unknown','review_text_missing'], as_of:asOf});
  }
  const instance=await db.prepare('SELECT instance_id FROM scout_instance WHERE id=1').first();
  const pending=await db.prepare(`SELECT COUNT(*) AS count FROM scout_products p WHERE NOT EXISTS(SELECT 1 FROM scout_product_insights i WHERE i.product_id=p.product_id AND i.snapshot_id=(SELECT MAX(o.snapshot_id) FROM scout_observations o WHERE o.product_id=p.product_id))`).first<{count:number}>();
  const last=products.results.at(-1);
  return send({version:1,instance_id:instance!.instance_id,as_of:asOf,stale:false,index_pending:pending?.count??0,items,next:items.length===40?JSON.stringify([last!.observed_at,last!.product_id]):null});
}
