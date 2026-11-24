import { afterEach, describe, expect, it, vi } from 'vitest';
import { plainText } from '../src/text';
import { appStoreSource, feedSource } from '../src/sources';
import type { Env } from '../worker-configuration';

afterEach(() => vi.unstubAllGlobals());
const intro = 'Ready to ship? One agent tests, translates and audits it';
const encoded = `&lt;p&gt;${intro}&lt;/p&gt; &lt;p&gt;&lt;a href=&quot;https://www.producthunt.com/products/kadeep-ai?utm_medium=rss&amp;amp;utm_campaign=feed&quot;&gt;Discussion&lt;/a&gt; | &lt;a href=&quot;https://www.producthunt.com/r/p/1319952&quot;&gt;Link&lt;/a&gt;&lt;/p&gt;`;

describe('feed text normalization', () => {
  it('decodes layered HTML before removing tags and feed navigation', () => {
    expect(plainText(encoded, true)).toBe(intro);
    expect(plainText(encoded.replaceAll('&', '&amp;'), true)).toBe(intro);
    expect(plainText('A&nbsp;&amp;&nbsp;B &#x1f680; &#169;')).toBe('A & B 🚀 ©');
    expect(plainText('<p>A</p><p>B</p><script>bad()</script><style>bad</style>')).toBe('A B');
    expect(plainText('Cost < 10 & save > 2')).toBe('Cost < 10 & save > 2');
  });
  it('cleans real Atom summary fields on the collection path', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(`<feed><entry><id>post-1</id><title>KaDeep AI</title><summary>${encoded}</summary><link href="https://example.com"/></entry></feed>`)));
    const items = await feedSource('producthunt-daily', {url:'https://example.com/feed',intervalMinutes:60}).fetch({} as Env, {http:fetch,env:{} as Env});
    expect(items[0].summary).toBe(intro);
  });
  it('handles RSS CDATA and nested Atom XHTML with the same parser', async () => {
    for (const body of [
      `<rss><channel><item><guid>post-1</guid><title>KaDeep AI</title><description><![CDATA[<p>${intro}</p><p><a href="/discussion">Discussion</a> | <a href="/link">Link</a></p>]]></description></item></channel></rss>`,
      `<feed><entry><id>post-1</id><title>KaDeep AI</title><summary type="xhtml"><div><p>${intro}</p><p><a href='/discussion'>Discussion</a> | <a href='/link'>Link</a></p></div></summary></entry></feed>`,
    ]) {
      vi.stubGlobal('fetch', vi.fn(async () => new Response(body)));
      const items = await feedSource('test', {url:'https://example.com/feed',intervalMinutes:60}).fetch({} as Env, {http:fetch,env:{} as Env});
      expect(items[0].summary).toBe(intro);
    }
  });
});

describe('App Store chart enrichment', () => {
  const chart = {feed:{results:[{id:'983156458',name:'Microsoft Authenticator',artistName:'Microsoft',genres:[{name:'Productivity'}]},{id:'284882215',name:'Facebook'}]}};
  const lookup = {results:[
    {trackId:284882215,description:'Connect with friends.',price:0,currency:'USD'},
    {trackId:983156458,description:'<p>Secure account sign-ins.</p>',price:0,currency:'USD',averageUserRating:4.7,userRatingCount:100},
  ]};
  const collect = () => appStoreSource().fetch({} as Env, {http:fetch,env:{} as Env});
  it('joins by immutable ID, preserves chart order and does one lookup', async () => {
    const http = vi.fn(async (input: string | URL | Request) => new Response(JSON.stringify(String(input).includes('itunes.apple.com') ? lookup : chart)));
    vi.stubGlobal('fetch', http);
    const items = await collect();
    expect(http).toHaveBeenCalledTimes(2);
    expect(items[0]).toMatchObject({id:'983156458',rank:1,description:'Secure account sign-ins.',rating:4.7,rating_count:100,price_cents:0,currency_code:'USD',genres_0_name:'Productivity'});
    expect(items[1]).toMatchObject({id:'284882215',rank:2,description:'Connect with friends.'});
    expect(new URL(String(http.mock.calls[1][0])).searchParams.get('id')).toBe('983156458,284882215');
  });
  it('partial or failed enrichment never publishes a thinner baseline', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => new Response(JSON.stringify(String(input).includes('itunes.apple.com') ? {results:[lookup.results[0]]} : chart))));
    await expect(collect()).rejects.toThrow('appstore_lookup_incomplete');
    vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => String(input).includes('itunes.apple.com') ? new Response('',{status:503}) : new Response(JSON.stringify(chart))));
    await expect(collect()).rejects.toThrow('appstore_lookup_http_503');
  });
  it('bounds the lookup response before parsing JSON', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: string | URL | Request) => String(input).includes('itunes.apple.com') ? new Response(' '.repeat(2 * 1024 * 1024 + 1)) : new Response(JSON.stringify(chart))));
    await expect(collect()).rejects.toThrow('appstore_lookup_over_limit');
  });
});
