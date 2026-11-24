import {afterEach, describe, expect, it, vi} from 'vitest';
import {cwsDiscoverySource, githubDiscoverySource, parseCwsDiscovery} from '../src/sources';
import {validateCollection} from '../src/contracts';

afterEach(() => {vi.unstubAllGlobals(); vi.useRealTimers();});
const a = 'a'.repeat(32), b = 'b'.repeat(32);
const card = (id = a, name = 'Useful &amp; simple', description = 'Save time &amp; organize tabs.') =>
  `<div data-item-id="${id}"><a href="./detail/useful/${id}" aria-labelledby="name-${id}" aria-describedby="rating-${id} desc-${id}"></a>
   <p id="name-${id}" role="heading">${name}</p><span id="rating-${id}"><span role="img" aria-label="Average rating 4.6 out of 5 stars.">4.6</span></span>
   <p id="desc-${id}">${description}</p></div>`;
const env = (value: string | null = null) => ({DB: {prepare: () => ({bind: () => ({first: async () => value === null ? null : {value}})})}} as never);

describe('discovery sources', () => {
  it('queries recent GitHub projects across languages with a baseline stable across days', async () => {
    vi.useFakeTimers(); vi.setSystemTime(new Date('2026-11-20T12:00:00Z'));
    const request = vi.fn(async (_url: string) => Response.json({items: [{id: 123, full_name: 'owner/project', language: 'Python', stargazers_count: 100}]}));
    vi.stubGlobal('fetch', request);
    const spec = githubDiscoverySource();
    expect(await spec.fetch(env(), {} as never)).toMatchObject([{id: '123', language: 'Python'}]);
    const url = new URL(request.mock.calls[0][0] as unknown as string);
    expect(url.searchParams.get('q')).toBe('created:>=2026-08-22 stars:>50 archived:false fork:false');
    expect(url.searchParams.get('sort')).toBe('stars');
    expect(url.searchParams.get('order')).toBe('desc');
    vi.setSystemTime(new Date('2026-11-21T12:00:00Z'));
    expect(githubDiscoverySource().specHash).toBe(spec.specHash);
    await spec.fetch(env(), {} as never);
    expect(new URL(request.mock.calls[1][0] as unknown as string).searchParams.get('q')).toContain('2026-08-23');
  });

  it('discovers useful card text without requiring IDs or extra detail requests', async () => {
    const request = vi.fn(async () => new Response(card() + card(b, 'Another extension', 'A different purpose.') + card()));
    vi.stubGlobal('fetch', request);
    const items = await cwsDiscoverySource().fetch(env(), {} as never);
    validateCollection(items);
    expect(request).toHaveBeenCalledTimes(1);
    expect(items).toHaveLength(2);
    expect(items[0]).toEqual({id: a, name: 'Useful & simple', description: 'Save time & organize tabs.', rating: 4.6,
      url: `https://chromewebstore.google.com/detail/useful/${a}`});
    expect(items[0]).not.toHaveProperty('rank');
    expect(items[0]).not.toHaveProperty('published');
  });

  it('fails closed on a bot wall, missing card fields, or a mismatching card link', () => {
    expect(() => parseCwsDiscovery('<html>Check your browser</html>')).toThrow('invalid_collection');
    expect(() => parseCwsDiscovery(card(b) + card(a, 'Name', ''))).toThrow('card_incomplete');
    expect(() => parseCwsDiscovery(card().replace(`./detail/useful/${a}`, `./detail/useful/${b}`))).toThrow('card_link_missing');
  });

  it('follows a canonical same-scope redirect but rejects another category or a login wall', async () => {
    const request = vi.fn()
      .mockResolvedValueOnce(new Response(null, {status:302,headers:{location:'/category/extensions/?hl=en'}}))
      .mockResolvedValueOnce(new Response(card()));
    vi.stubGlobal('fetch', request);
    expect(await cwsDiscoverySource().fetch(env(), {} as never)).toHaveLength(1);
    expect(request).toHaveBeenCalledTimes(2);
    for (const location of ['https://accounts.google.com/login','/unsupported','/category/themes','/']) {
      request.mockReset().mockResolvedValue(new Response(null, {status:302,headers:{location}}));
      await expect(cwsDiscoverySource().fetch(env(), {} as never)).rejects.toThrow('redirect_outside_scope');
      expect(request).toHaveBeenCalledTimes(1);
    }
  });

  it('adds optional existing watchlist entries and rejects partial detail results', async () => {
    const request = vi.fn(async (url: string) => url.includes('/category/') ? new Response(card()) : new Response('', {status: 404}));
    vi.stubGlobal('fetch', request);
    const items = await cwsDiscoverySource().fetch(env(`${a},${b}`), {} as never);
    expect(request).toHaveBeenCalledTimes(2);
    expect(items[1]).toMatchObject({id: b, error: 'HTTP 404'});
    expect(() => validateCollection(items)).toThrow('incomplete');
  });
});
