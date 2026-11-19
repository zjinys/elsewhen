import {it,expect} from 'vitest';
import {buildInsight} from '../src/insights';
import type {Observation} from '../src/trends';
const sample=(day:number,price:number,scope='v1'):Observation=>({snapshot_id:day+1,source_id:'s',taken_at:new Date(Date.UTC(2026,10,day+1)).toISOString(),spec_hash:scope,interval_minutes:1440,payload_json:JSON.stringify({price_cents:price,currency_code:'USD',ratings_count:day+1})});
it('quiet refresh keeps the last substantive update and price evidence',()=>{
 const rows=[sample(4,200),sample(3,200),sample(2,100)];
 // Hold the count stable to isolate price-only significance.
 for(const row of rows){const p=JSON.parse(row.payload_json);p.ratings_count=1;row.payload_json=JSON.stringify(p)}
 const insight=buildInsight(rows,rows[0].taken_at);
 expect(insight.changedAt).toBe(rows[1].taken_at);
 expect(insight.reasons[0]).toMatchObject({kind:'price_change',from:100,to:200});
});
it('parser resets do not become price changes, and old discrete events expire',()=>{
 const rows=[sample(4,200,'v2'),sample(3,100)];
 expect(buildInsight(rows,rows[0].taken_at).reasons).toHaveLength(0);
 const stale=[sample(3,200),sample(2,100)];
 expect(buildInsight(stale,'2026-11-25T00:00:00.000Z').reasons).toHaveLength(0);
});

it('prices without a known currency never produce a comparable price signal',()=>{
 const rows=[sample(4,200),sample(3,100)];
 for(const row of rows){const payload=JSON.parse(row.payload_json);delete payload.currency_code;row.payload_json=JSON.stringify(payload)}
 expect(buildInsight(rows,rows[0].taken_at).reasons).toHaveLength(0);
 expect(buildInsight(rows,rows[0].taken_at).windows.some(w=>w.metric==='price_cents')).toBe(false);
});
