/** Opt-in public-source checks; does not deploy or read a personal database. */
import { it,expect } from 'vitest';
import { SOURCES } from '../src/sources';
import { validateCollection } from '../src/contracts';
import type { Env } from '../worker-configuration';
const live=process.env.SCOUT_LIVE_SOURCES==='1';
for(const id of ['gumroad-notion','appstore-us-top-free','cws-discover']){
 it.skipIf(!live)(`public source contract: ${id}`,async()=>{
  const spec=SOURCES.find(s=>s.id===id)!;
  const env={DB:{prepare:()=>({bind:()=>({first:async()=>null})})}} as unknown as Env;
  const items=await spec.fetch(env,{http:fetch,env});
  validateCollection(items);expect(items.length).toBeGreaterThan(0);
  console.log('SOURCE_COVERAGE',JSON.stringify({source:id,count:items.length,fields:Object.keys(items[0])}));
  if(id.startsWith('appstore')) {
    expect(items[0].rank).toBe(1);
    expect(items.every(item=>typeof item.description==='string' && item.description.length>0)).toBe(true);
  }
 },40000);
}
