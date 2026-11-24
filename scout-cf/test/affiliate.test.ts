import { describe, it, expect } from 'vitest';
import { analyzeProgram, checkAffiliate, programLinks, publicUrl } from '../src/affiliate';

const program = '<h1>Affiliate Program</h1><p>Join our program and earn 20% commission on paid subscriptions, capped at $150 per customer.</p>';
const response = (body:string) => new Response(body,{headers:{'content-type':'text/html'}});
describe('official affiliate evidence',()=>{
  it('requires explicit cash terms and program heading',()=>{
    expect(analyzeProgram(program)).toMatchObject({status:'confirmed',reward_type:'cash'});
    expect(analyzeProgram('<h1>Affiliate software</h1><p>Create your own affiliate program.</p>')).toMatchObject({status:'unknown'});
    expect(analyzeProgram('<h1>Referral program</h1><p>Join and earn 10 credits.</p>')).toMatchObject({status:'unknown',reward_type:'credit'});
    expect(analyzeProgram('<h1>Affiliate program</h1><p>We no longer offer an affiliate program.</p>')).toMatchObject({status:'unavailable'});
    expect(analyzeProgram('<h1>Partner integrations</h1><p>Earn commission by using our software</p>')).toMatchObject({status:'unknown'});
  });
  it('only follows actual labeled links, not keywords or guessed paths',()=>{
    expect(programLinks('<p>Affiliate platform</p><a href="/blog">Blog</a><a href="/partners">Affiliate program</a>','https://vendor.com')).toEqual(['https://vendor.com/partners']);
  });
  it('follows PH redirect, requires official endorsement of third party',async()=>{
    const urls:string[]=[];
    const http=(async(input:string|URL|Request)=>{
      const url=String(input);urls.push(url);
      if(url.includes('producthunt.com'))return new Response(null,{status:302,headers:{location:'https://vendor.com/'}});
      if(url==='https://vendor.com/')return response('<a href="https://vendor.partnerplatform.com/affiliate">Affiliate program</a>');
      return response(program);
    }) as typeof fetch;
    const result=await checkAffiliate('https://www.producthunt.com/r/p/123',http);
    expect(result).toMatchObject({status:'confirmed',official_website:'https://vendor.com/',source_url:'https://vendor.partnerplatform.com/affiliate'});
    expect(urls).toHaveLength(3);
    expect(result.evidence_quote).toContain('capped at $150');
  });
  it('no result, challenge, bad HTTP and credit rewards never mean no program',async()=>{
    for(const res of [response('<h1>Home</h1>'),response('Just a moment captcha'),new Response('',{status:403})]){
      expect((await checkAffiliate('https://vendor.com', (async()=>res) as typeof fetch)).status).toBe('unknown');
    }
  });
  it('blocks private addresses and redirect loops within request budget',async()=>{
    for(const url of ['http://vendor.com','https://127.0.0.1','https://[::1]','https://localhost','https://0x7f000001','https://user:pass@vendor.com']) expect(()=>publicUrl(url)).toThrow();
    let count=0;
    const result=await checkAffiliate('https://vendor.com',(async()=>{count++;return new Response(null,{status:302,headers:{location:'https://vendor.com/again'}});}) as typeof fetch);
    expect(count).toBe(6);expect(result.status).toBe('unknown');
  });
});
