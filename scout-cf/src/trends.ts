import { METRICS } from './contracts';

export interface Observation {
  snapshot_id: number; source_id: string; taken_at: string; spec_hash: string;
  interval_minutes: number; payload_json: string;
}
export function trends(observations: Observation[], asOf: string, days: number, includePrevious=true): unknown[] {
  const end = Date.parse(asOf);
  const target = end - days * 86400000;
  const sources = [...new Set(observations.map(o => o.source_id))];
  const output: unknown[] = [];
  const previous=includePrevious ? trends(observations.filter(o=>Date.parse(o.taken_at)<=target),new Date(target).toISOString(),days,false) as Array<Record<string,unknown>> : [];
  for (const source of sources) {
    const all = observations.filter(o => o.source_id === source).sort((a,b) => Date.parse(a.taken_at)-Date.parse(b.taken_at));
    const last = all.at(-1)!;
    const cadence = last.interval_minutes * 60000;
    const first = all.filter(o => Date.parse(o.taken_at) <= target && target-Date.parse(o.taken_at)<=cadence*1.5 && o.spec_hash===last.spec_hash).at(-1);
    const samples=all.filter(o => Date.parse(o.taken_at)>=Date.parse(first?.taken_at ?? all[0].taken_at));
    const complete=!!first && last.spec_hash !== 'legacy-unknown' && end-Date.parse(last.taken_at)<=cadence*1.5 && samples.every((o,i)=>o.spec_hash===last.spec_hash && (!i || Date.parse(o.taken_at)-Date.parse(samples[i-1].taken_at)<=cadence*1.5));
    const before=JSON.parse((first ?? all[0]).payload_json) as Record<string,unknown>;
    const after=JSON.parse(last.payload_json) as Record<string,unknown>;
    for(const [metric,definition] of Object.entries(METRICS)) {
      const a=before[metric], b=after[metric];
      if(typeof a!=='number' || typeof b!=='number' || !Number.isFinite(a) || !Number.isFinite(b)) continue;
      const metricComplete=complete && samples.every(o=>{const p=JSON.parse(o.payload_json);return typeof p[metric]==='number' && Number.isFinite(p[metric]) && (metric!=='price_cents' || p.currency_code===after.currency_code)});
      if(metric==='price_cents' && (typeof after.currency_code!=='string' || !after.currency_code || before.currency_code!==after.currency_code)) continue;
      const delta=b-a;
      output.push({metric, unit:definition.unit, source_id:source, days, complete:metricComplete,
        from:a,to:b,delta, percent: metricComplete && a>0 && definition.kind==='counter' ? delta/a*100 : null,
        rank_improvement: definition.kind==='rank' ? -delta : null,
        correction:definition.kind==='counter' && delta<0,
        from_at:(first ?? all[0]).taken_at,to_at:last.taken_at,samples:samples.length,
        snapshot_ids:[(first ?? all[0]).snapshot_id,last.snapshot_id],algorithm:'window-v1',
        previous_window:previous.find(t=>t.source_id===source&&t.metric===metric)??null,
        series:samples.filter((_,i)=>i%Math.max(1,Math.ceil(samples.length/30))===0 || i===samples.length-1).map(o=>({at:o.taken_at,value:JSON.parse(o.payload_json)[metric]})),
        sustained: metricComplete && definition.kind==='counter' && samples.length>=4 && delta>0 && samples.slice(1).every((o,i)=>Number(JSON.parse(o.payload_json)[metric])>=Number(JSON.parse(samples[i].payload_json)[metric])) && samples.slice(1).filter((o,i)=>Number(JSON.parse(o.payload_json)[metric])>Number(JSON.parse(samples[i].payload_json)[metric])).length>=3,
      });
    }
  }
  return output;
}
