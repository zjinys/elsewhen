import { afterAll,beforeAll,describe,it,expect } from 'vitest';
import { Miniflare,convertV4MiniflareOptions } from 'miniflare';
import { URL as NodeURL } from 'node:url';
import { readFileSync } from 'node:fs';
import { acquireSource,upsertSource,recordRun,getLastSnapshot } from '../src/store';
import { diffItems } from '../src/diff';
import { findingPage } from '../src/feed';
import { trends,productRoutes } from '../src/products';
import { productIdentity,validateCollection } from '../src/contracts';
import { refreshAffiliates, affiliateEvidence } from '../src/affiliate';
import worker from '../src/index';
import {cwsDiscoverySource, githubDiscoverySource} from '../src/sources';
import type {Env} from '../worker-configuration';

let mf:Miniflare, db:D1Database;
beforeAll(async()=>{
 mf=new Miniflare(convertV4MiniflareOptions({modules:true,script:'export default {fetch(){return new Response("ok")}}',d1Databases:['DB'],compatibilityDate:'2026-11-15'}));
 db=await mf.getD1Database('DB');
 for(const file of ['0001_init.sql','0002_admin.sql','0003_finding_delta.sql','0004_research_observations.sql','0005_product_insights.sql','0006_affiliate_evidence.sql']) {
   const sql=readFileSync(new NodeURL('../migrations/'+file,import.meta.url),'utf8').replace(/--[^\n]*/g,'');
   for(const statement of sql.split(';').filter(s=>s.trim())) await db.prepare(statement).run();
 }
 await upsertSource(db,{id:'gumroad-notion',name:'Notion',enabled:true,specHash:'s1',intervalMinutes:720});
},30000);
afterAll(async()=>{await mf?.dispose()});
describe('published observations',()=>{
 it('rejects partial and empty collection',()=>{
   expect(()=>validateCollection([])).toThrow();
   expect(()=>validateCollection([{id:'x',error:'parse failed'}])).toThrow();
 });
 it('fences stale collectors and atomically publishes snapshots',async()=>{
   const token=await acquireSource(db,'gumroad-notion'); expect(token).toBeTruthy();
   expect(await acquireSource(db,'gumroad-notion')).toBeNull();
   await expect(recordRun(db,'gumroad-notion','2026-11-01T00:00:00Z',[{id:'a',name:'A'}],[],'2026-11-01T00:00:00Z','stale','s1',720)).rejects.toThrow();
   expect(await getLastSnapshot(db,'gumroad-notion')).toBeNull();
   await recordRun(db,'gumroad-notion','2026-11-01T00:00:00Z',[{id:'a',name:'A',ratings_count:5}],[],'2026-11-01T00:00:00Z',token!,'s1',720);
   const t2=await acquireSource(db,'gumroad-notion');
   const current=[{id:'a',name:'A',ratings_count:9}];
   await recordRun(db,'gumroad-notion','2026-11-08T00:00:00Z',current,diffItems(current,(await getLastSnapshot(db,'gumroad-notion'))!.data),'2026-11-08T00:00:00Z',t2!,'s1',720);
   const products=await db.prepare('SELECT * FROM scout_products').all();expect(products.results).toHaveLength(1);
   const obs=await db.prepare('SELECT * FROM scout_observations').all(); expect(obs.results).toHaveLength(2);
   const response=await productRoutes(db,new URL('https://test/v1/products'));expect(response.status).toBe(200);
 });
 it('resumes a fixed bound across >2500 entries and concurrent arrivals',async()=>{
   await db.prepare(`INSERT INTO scout_findings(source_id,snapshot_id,kind,item_key,payload_json,detected_at)
    SELECT 'gumroad-notion',1,'new',value,'{}','2026-11-17' FROM json_each(?)`).bind(JSON.stringify(Array.from({length:2501},(_,i)=>String(i)))).run();
   let page:any=await findingPage(db,new URL('https://test/v1/findings?limit=250'));
   const upper=page.upper;const seen=page.items.map((x:any)=>x.id);
   await db.prepare(`INSERT INTO scout_findings(source_id,snapshot_id,kind,item_key,payload_json,detected_at) VALUES('gumroad-notion',1,'new','late','{}','1900-01-01')`).run();
   while(page.has_more){page=await findingPage(db,new URL(`https://test/v1/findings?limit=250&after=${page.next}&upper=${upper}`));seen.push(...page.items.map((x:any)=>x.id));}
   expect(new Set(seen).size).toBe(2502);expect(Math.max(...seen)).toBe(upper);
   const next:any=await findingPage(db,new URL(`https://test/v1/findings?after=${upper}`));expect(next.items).toHaveLength(1);
 });
});
describe('truthful trends',()=>{
 const sample=(day:number,value:number)=>({snapshot_id:day+1,source_id:'s',taken_at:new Date(Date.UTC(2026,10,day+1)).toISOString(),spec_hash:'v1',interval_minutes:1440,payload_json:JSON.stringify({ratings_count:value,rank:20-day})});
 it('shows sustained comparable windows and rank movement',()=>{
  const values=Array.from({length:8},(_,i)=>sample(i,10+i));
  const result:any[]=trends(values,values[7].taken_at,7);
  expect(result.find(t=>t.metric==='ratings_count')).toMatchObject({complete:true,delta:7,sustained:true});
  expect(result.find(t=>t.metric==='rank')).toMatchObject({rank_improvement:7,percent:null});
 });
 it('missing observations do not become complete growth',()=>{
  const values=[sample(0,10),sample(7,90)];
  expect((trends(values,values[1].taken_at,7) as any[])[0].complete).toBe(false);
 });
 it('deduplicates products across searches without merging other platforms',()=>{
  expect(productIdentity('gumroad-notion',{id:'a'})).toEqual(productIdentity('gumroad-planner',{id:'a'}));
  expect(productIdentity('github-rust',{id:'a'})).not.toEqual(productIdentity('gumroad-notion',{id:'a'}));
 });
 it('uses GitHub immutable numeric identity across repository renames',()=>{
  expect(productIdentity('github-rust',{id:'123',full_name:'old/name'})).toEqual(productIdentity('github-dart',{id:'123',full_name:'new/name'}));
 });
});

describe('version and coverage edge cases',()=>{
 const sample=(day:number,payload:Record<string,unknown>,spec='v1')=>({snapshot_id:day+1,source_id:'s',taken_at:new Date(Date.UTC(2026,10,day+1)).toISOString(),spec_hash:spec,interval_minutes:1440,payload_json:JSON.stringify(payload)});
 it('does not invent percentages at zero or infer growth across shape changes',()=>{
  const zero=Array.from({length:8},(_,i)=>sample(i,{ratings_count:i}));
  expect((trends(zero,zero[7].taken_at,7) as any[])[0].percent).toBeNull();
  zero[4].spec_hash='v2';expect((trends(zero,zero[7].taken_at,7) as any[])[0].complete).toBe(false);
 });
 it('missing metric and currency changes invalidate numeric comparisons',()=>{
  const rows=Array.from({length:8},(_,i)=>sample(i,{ratings_count:5+i,price_cents:100,currency_code:'USD'}));
  rows[3]=sample(3,{price_cents:100,currency_code:'EUR'});
  const result=trends(rows,rows[7].taken_at,7) as any[];
  expect(result.every(t=>!t.complete)).toBe(true);
 });
 it('replays retained snapshots without creating duplicate products or observations',async()=>{
  const {backfillObservations}=await import('../src/store');
  const before=await db.prepare('SELECT COUNT(*) AS n FROM scout_observations').first<{n:number}>();
  await backfillObservations(db);await backfillObservations(db);
  expect((await db.prepare('SELECT COUNT(*) AS n FROM scout_observations').first<{n:number}>())!.n).toBe(before!.n);
 });
});

describe('product paging stability',()=>{
 it('keeps the as-of boundary when a previously unvisited product updates',async()=>{
  const items=Array.from({length:45},(_,i)=>({id:`stable-${i}`,name:`Stable ${i}`,ratings_count:i}));
  const lease=await acquireSource(db,'gumroad-notion');
  await recordRun(db,'gumroad-notion','2026-11-16T00:00:00Z',items,[],'2026-11-16T00:00:00Z',lease!,'s2',720);
  const first=await (await productRoutes(db,new URL('https://test/v1/products?as_of=2026-11-17T00:00:00Z'))).json() as any;
  expect(first.items).toHaveLength(40);expect(first.next).toBeTruthy();
  const lease2=await acquireSource(db,'gumroad-notion');
  await recordRun(db,'gumroad-notion','2026-11-18T00:00:00Z',items,[],'2026-11-18T00:00:00Z',lease2!,'s2',720);
  const url=new URL('https://test/v1/products');url.searchParams.set('as_of',first.as_of);url.searchParams.set('before',first.next);
  const next=await (await productRoutes(db,url)).json() as any;
  expect(next.items.length).toBeGreaterThan(0);
  const ids=[...first.items,...next.items].map((i:any)=>i.product_id);expect(new Set(ids).size).toBe(46);
  expect(next.items.every((i:any)=>i.last_seen_at<='2026-11-17T00:00:00Z')).toBe(true);
 });
});

describe('global discovery index',()=>{
 it('finds real signals beyond quiet pages and keeps first baselines quiet',async()=>{
  const {refreshInsights}=await import('../src/insights');
  await upsertSource(db,{id:'gumroad-scale',name:'Scale',enabled:true,specHash:'v',intervalMinutes:720});
  const baseline=Array.from({length:81},(_,i)=>({id:`global-${i}`,name:`Global ${i}`,price_cents:100,currency_code:'USD'}));
  let lease=await acquireSource(db,'gumroad-scale');
  await recordRun(db,'gumroad-scale','2026-11-23T00:00:00.000Z',baseline,[],'2026-11-23T00:00:00.000Z',lease!,'v',720);
  const changed=baseline.map((p,i)=>i===0?{...p,price_cents:200}:p);
  lease=await acquireSource(db,'gumroad-scale');
  await recordRun(db,'gumroad-scale','2026-11-24T00:00:00.000Z',changed,diffItems(changed,baseline),'2026-11-24T00:00:00.000Z',lease!,'v',720);
  const asOf='2026-11-25T00:00:00.000Z';
  for(let i=0;i<5;i++)await refreshInsights(db,asOf);
  const request=async(view:string)=>await (await productRoutes(db,new URL(`https://test/v1/products?source=gumroad-scale&view=${view}&as_of=${asOf}`))).json() as any;
  const signals=await request('signals');expect(signals.items).toHaveLength(1);expect(signals.next).toBeNull();
  expect(signals.items[0].payload.price_cents).toBe(200);
  expect((await request('price')).items).toHaveLength(1);
  expect((await request('growth')).items).toHaveLength(0);
  expect((await request('all')).items).toHaveLength(40);
 });
});

it('upgrades legacy GitHub names to numeric identity without losing history',async()=>{
 await upsertSource(db,{id:'github-migration',name:'GitHub',enabled:true,specHash:'v',intervalMinutes:720});
 let lease=await acquireSource(db,'github-migration');
 await recordRun(db,'github-migration','2026-11-26T00:00:00.000Z',[{id:'old/repo',full_name:'old/repo',stargazers_count:10}],[],'2026-11-26',lease!,'old',720);
 lease=await acquireSource(db,'github-migration');
 await recordRun(db,'github-migration','2026-11-27T00:00:00.000Z',[{id:'12345',full_name:'old/repo',stargazers_count:11}],[],'2026-11-27',lease!,'v',720);
 const products=await db.prepare("SELECT * FROM scout_products WHERE platform='github'").all();
 expect(products.results).toHaveLength(1);expect(products.results[0].platform_key).toBe('12345');
 expect((await db.prepare('SELECT * FROM scout_observations WHERE product_id=?').bind(products.results[0].product_id).all()).results).toHaveLength(2);
});

it('affiliate enrichment preserves observations, fixed evidence and failed-check history', async()=>{
 const source='producthunt-affiliate-test';
 await upsertSource(db,{id:source,name:'PH',enabled:true,specHash:'v',intervalMinutes:60});
 const token=await acquireSource(db,source);
 const items=[{id:'cash',title:'Cash product',website_url:'https://vendor.com/'},{id:'unknown',title:'Unknown product',website_url:'https://other.com/'}];
 await recordRun(db,source,'2026-12-10T00:00:00Z',items,[],'2026-12-10T00:00:00Z',token!,'v',60);
 const original=await db.prepare('SELECT payload_json FROM scout_products WHERE product_id=?').bind('["producthunt","cash"]').first();
 const http=(async(input:string|URL|Request)=>new Response(String(input)==='https://vendor.com/'?'<a href="/affiliate">Affiliate program</a>':String(input)==='https://vendor.com/affiliate'?'<h1>Affiliate Program</h1><p>Join and earn 20% commission, up to $150 per customer.</p>':'<h1>Home</h1>',{headers:{'content-type':'text/html'}})) as typeof fetch;
 await refreshAffiliates(db,http,new Date('2026-12-10T01:00:00Z'));
 const evidence=(await affiliateEvidence(db,['["producthunt","cash"]'],new Date('2026-12-10T02:00:00Z'))).get('["producthunt","cash"]') as any;
 expect(evidence).toMatchObject({status:'confirmed',reward_type:'cash',stale:false});
 const url=new URL('https://test/v1/products?view=all&source=producthunt-affiliate-test&as_of=2026-12-10T02:00:00Z');
 expect(((await (await productRoutes(db,url)).json()) as any).items.find((p:any)=>p.title==='Cash product').affiliate).toMatchObject({status:'confirmed'});
 expect(await db.prepare('SELECT payload_json FROM scout_products WHERE product_id=?').bind('["producthunt","cash"]').first()).toEqual(original);
 await refreshAffiliates(db,(async()=>new Response('',{status:503})) as typeof fetch,new Date('2026-12-18T01:00:00Z'));
 const stale=(await affiliateEvidence(db,['["producthunt","cash"]'],new Date('2026-12-18T02:00:00Z'))).get('["producthunt","cash"]') as any;
 expect(stale).toMatchObject({status:'confirmed',stale:true,check_outcome:'http_503'});
 expect(stale.evidence_quote).toContain('$150');
 expect(stale.checked_at).toBe(evidence.checked_at);
 url.searchParams.set('as_of','2026-12-18T02:00:00Z');
 expect(((await (await productRoutes(db,url)).json()) as any).items.find((p:any)=>p.title==='Cash product').affiliate.stale).toBe(true);
});

it('retires presets without deleting history and lists only current source specs', async () => {
 const oldIds = ['github-dart', 'github-rust', 'cws-watchlist'];
 for (const id of oldIds) await upsertSource(db, {id, name: id, enabled: true, specHash: 'old', intervalMinutes: 720});
 const lease = await acquireSource(db, oldIds[0]);
 await recordRun(db, oldIds[0], '2026-12-20T00:00:00Z', [{id:'retained',full_name:'old/retained'}], [], '2026-12-20', lease!, 'old', 720);
 const before = await getLastSnapshot(db, oldIds[0]);
 const sql = readFileSync(new NodeURL('../migrations/0007_discovery_sources.sql', import.meta.url), 'utf8').replace(/--[^\n]*/g, '');
 for (const statement of sql.split(';').filter(s => s.trim())) await db.prepare(statement).run();
 for (const id of oldIds) expect((await db.prepare('SELECT enabled FROM scout_sources WHERE id=?').bind(id).first())!.enabled).toBe(0);
 expect(await getLastSnapshot(db, oldIds[0])).toEqual(before);
 for (const spec of [githubDiscoverySource(), cwsDiscoverySource()]) await upsertSource(db, spec);
 const env = {DB: db} as Env;
 for (const path of ['/sources','/admin/sources']) {
   const response = await worker.fetch(new Request('https://test'+path), env);
   expect(response.status).toBe(200);
   const rows = await response.json() as {id:string}[];
   expect(rows.map(r => r.id)).toEqual(expect.arrayContaining(['github-newcomers','cws-discover']));
   expect(rows.some(r => oldIds.includes(r.id))).toBe(false);
 }
 const response = await worker.fetch(new Request('https://test/admin/sources/github-dart', {method:'PATCH',body:JSON.stringify({enabled:true})}), env);
 expect(response.status).toBe(404);
 const detail = await productRoutes(db, new URL('https://test/v1/products/detail?id='+encodeURIComponent('["github","retained"]')));
 expect(detail.status).toBe(200);
 expect((await detail.json() as any).history).toHaveLength(1);
});

it('accepts optional CWS store links and bounds extra tracking without requiring IDs', async () => {
 const env = {DB: db} as Env;
 const put = (value: unknown) => worker.fetch(new Request('https://test/admin/settings/cws_ids', {method:'PUT',body:JSON.stringify({value})}), env);
 const id = 'a'.repeat(32);
 const response = await put(`https://chromewebstore.google.com/detail/extension/${id}?hl=en\n${id}`);
 expect(await response.json()).toMatchObject({ids:[id]});
 expect((await put(null)).status).toBe(400);
 const ids = Array.from({length:21},(_,i)=>'a'.repeat(30)+String.fromCharCode(97+Math.floor(i/16),97+i%16));
 expect((await put(ids.join(','))).status).toBe(400);
 expect(await (await put('')).json()).toMatchObject({ids:[]});
});
