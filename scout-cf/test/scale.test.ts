// Opt-in local D1 benchmark. Synthetic records never touch a remote service.
import { it,expect } from 'vitest';
import { Miniflare,convertV4MiniflareOptions } from 'miniflare';
import { readFileSync } from 'node:fs';
import { URL as NodeURL } from 'node:url';
import { cpus,platform,arch } from 'node:os';
import { productRoutes } from '../src/products';

it.skipIf(process.env.SCOUT_SCALE !== '1')('10k products and 1m observations stay bounded',async()=>{
 const mf=new Miniflare(convertV4MiniflareOptions({modules:true,script:'export default {fetch(){return new Response("ok")}}',d1Databases:['DB'],compatibilityDate:'2026-11-15'}));
 try {
  const db=await mf.getD1Database('DB');
  for(const file of ['0001_init.sql','0002_admin.sql','0003_finding_delta.sql','0004_research_observations.sql','0005_product_insights.sql','0006_affiliate_evidence.sql']) {
   for(const s of readFileSync(new NodeURL('../migrations/'+file,import.meta.url),'utf8').replace(/--[^\n]*/g,'').split(';').filter(s=>s.trim()))await db.prepare(s).run();
  }
  await db.prepare("INSERT INTO scout_sources(id,name,enabled,spec_hash,interval_minutes) VALUES('scale','Scale',1,'scale-v1',180)").run();
  await db.prepare(`WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<10000)
   INSERT INTO scout_products SELECT printf('p%05d',n),'fixture',CAST(n AS TEXT),'Product '||n,'{"price_cents":2900,"currency_code":"USD","ratings_count":100}', '2026-11-04T15:00:00.000Z','2026-11-17T00:00:00.000Z' FROM seq`).run();
  await db.prepare("INSERT INTO scout_product_sources SELECT product_id,'scale',last_seen_at,1 FROM scout_products").run();
  // Batches of 10 observations/product bound each seed query to 100k rows.
  for(let offset=0;offset<100;offset+=10)await db.prepare(`WITH RECURSIVE seq(n) AS (SELECT ? UNION ALL SELECT n+1 FROM seq WHERE n<?)
   INSERT INTO scout_observations SELECT n+1,product_id,'scale',platform_key,
     strftime('%Y-%m-%dT%H:%M:%fZ','2026-11-17T00:00:00Z',printf('-%d hours',(99-n)*3)),
     'scale-v1',180,json_object('price_cents',2900,'currency_code','USD','ratings_count',n+1)
   FROM scout_products CROSS JOIN seq`).bind(offset,offset+9).run();
  const measure=async(path:string)=>{
   const samples:number[]=[];
   for(let i=0;i<21;i++){
    const start=performance.now();const response=await productRoutes(db,new URL('https://fixture'+path));
    expect(response.status).toBe(200);await response.json();
    if(i)samples.push(performance.now()-start);
   }
   samples.sort((a,b)=>a-b);return {p50_ms:Math.round(samples[9]),p95_ms:Math.round(samples[18]),runs:20};
  };
  const list=await measure('/v1/products?as_of=2026-11-17T00:00:00.000Z');
  await db.prepare(`INSERT INTO scout_product_insights SELECT product_id,100,'2026-11-17T00:00:00.000Z','2026-11-17T00:00:00.000Z','2026-11-17T00:00:00.000Z','2026-11-18T00:00:00.000Z','[{"kind":"sustained_growth","valid_until":"2026-11-18T00:00:00.000Z"}]' FROM scout_products`).run();
  const signals=await measure('/v1/products?view=signals&as_of=2026-11-17T00:00:00.000Z');
  const detail=await measure('/v1/products/detail?id=p00001');
  const plan=await db.prepare("EXPLAIN QUERY PLAN SELECT * FROM scout_observations WHERE product_id='p00001' AND taken_at>='2026-11-01' ORDER BY taken_at DESC LIMIT 800").all();
  console.log('SCOUT_SCALE_RESULT',JSON.stringify({environment:{cpu:cpus()[0]?.model,platform:platform(),arch:arch(),node:process.version},products:10000,observations:1000000,list,signals,detail,peak_node_rss_bytes:process.resourceUsage().maxRSS*1024,query_plan:plan.results}));
  expect(list.p95_ms).toBeLessThanOrEqual(500);
  expect(signals.p95_ms).toBeLessThanOrEqual(500);
  expect(detail.p95_ms).toBeLessThanOrEqual(300);
 }finally{await mf.dispose()}
},180000);
