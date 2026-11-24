import { parseDocument, DomUtils } from 'htmlparser2';
import { plainText } from './text';

export interface AffiliateEvidence {
  status: 'confirmed' | 'unavailable' | 'unknown';
  reward_type: 'cash' | 'credit' | 'unknown';
  official_website: string | null;
  program_url: string | null;
  source_url: string | null;
  evidence_quote: string | null;
  checked_at: string;
  valid_until: string;
  check_outcome: string;
}

export function publicUrl(raw: string): URL {
  const url = new URL(raw);
  const host = url.hostname.toLowerCase();
  if (url.protocol !== 'https:' || url.username || url.password ||
      (url.port && url.port !== '443') || !host.includes('.') ||
      /^[\d.]+$/.test(host) || host.includes(':') || host.startsWith('[') ||
      /(?:^|\.)(?:localhost|local|internal|test|invalid)$/.test(host)) {
    throw new Error('unsafe_url');
  }
  url.hash = '';
  return url;
}

const programLabel = /\b(?:affiliates?|referral\s+program|partner\s+program)\b|联盟计划|推广计划|返佣/i;
const negative = /(?:do not|don't|no longer|not currently|does not|doesn't).{0,45}(?:offer|have|run|support).{0,35}(?:affiliate|referral)|(?:affiliate|referral)\s+program.{0,35}(?:closed|discontinued|not available)|(?:暂停|关闭|不提供).{0,12}(?:返佣|联盟计划)/i;
const invitation = /\b(?:join|become|apply|sign\s*up|start earning|refer.{0,30}earn|you.{0,20}earn)\b|申请|加入|注册/i;
const money = /\b(?:commission|cash|payout|paypal|revenue share|subscription payments)\b|佣金|返佣|现金/i;

export function analyzeProgram(html: string): Pick<AffiliateEvidence, 'status' | 'reward_type' | 'evidence_quote'> {
  // Ignore scripts and navigation; require evidence from a program-specific heading.
  const doc = parseDocument(html);
  const headings = DomUtils.findAll(n => /^h[1-3]$/.test(n.name), doc.children);
  if (!headings.some(n => programLabel.test(DomUtils.textContent(n)))) {
    return {status:'unknown', reward_type:'unknown', evidence_quote:null};
  }
  const text = plainText(html);
  const denial = text.match(negative);
  if (denial) {
    return {status:'unavailable', reward_type:'unknown', evidence_quote:text.slice(Math.max(0, denial.index! - 50), denial.index! + 240)};
  }
  const paragraphs = DomUtils.findAll(n => ['p','li','h1','h2','h3'].includes(n.name), doc.children)
    .map(n => plainText(DomUtils.getOuterHTML(n))).filter(Boolean);
  const quote = paragraphs.find(p => p.length <= 1500 && invitation.test(p) && money.test(p));
  if (quote) return {status:'confirmed',reward_type:'cash',evidence_quote:quote.slice(0,800)};
  const credit = paragraphs.find(p => invitation.test(p) && /\bcredits?\b|使用额度|积分/i.test(p));
  return {status:'unknown',reward_type:credit ? 'credit' : 'unknown',evidence_quote:credit?.slice(0,800) ?? null};
}

export function programLinks(html: string, base: string): string[] {
  const doc = parseDocument(html);
  const links = DomUtils.findAll(n => n.name === 'a' && !!n.attribs.href, doc.children);
  return [...new Set(links.filter(n => programLabel.test(DomUtils.textContent(n)))
    .flatMap(n => {try {return [publicUrl(new URL(n.attribs.href, base).href).href];} catch {return [];}}))].slice(0,2);
}

/** At most six public HTTPS requests and 512 KiB per page, including redirects. */
export async function checkAffiliate(website: string, http: typeof fetch = fetch, now = new Date()): Promise<AffiliateEvidence> {
  let budget = 6;
  async function read(raw: string, redirects = true): Promise<{url:string;html:string}> {
    let url = publicUrl(raw);
    while (budget-- > 0) {
      const response = await http(url.href, {redirect:'manual',signal:AbortSignal.timeout(8000),headers:{'User-Agent':'scout-cf/0.1 (+public product research)'}});
      if (response.status >= 300 && response.status < 400 && redirects) {
        const location = response.headers.get('location');
        await response.body?.cancel();
        if (!location) throw new Error('redirect_without_location');
        url = publicUrl(new URL(location,url).href);
        continue;
      }
      if (!response.ok) {await response.body?.cancel(); throw new Error(`http_${response.status}`);}
      if (!(response.headers.get('content-type') ?? '').includes('text/html')) {await response.body?.cancel(); throw new Error('not_html');}
      const reader = response.body!.getReader();
      const decoder = new TextDecoder(); let html = ''; let size = 0;
      try {
        while (true) {
          const {done,value} = await reader.read(); if (done) break;
          size += value.byteLength;
          if (size > 512*1024) {await reader.cancel(); throw new Error('page_over_limit');}
          html += decoder.decode(value,{stream:true});
        }
      } finally {reader.releaseLock();}
      html += decoder.decode();
      if (/just a moment|cf-chl-|captcha/i.test(html.slice(0,10000))) throw new Error('challenge');
      return {url:url.href,html};
    }
    throw new Error('request_budget');
  }
  const result: AffiliateEvidence = {status:'unknown',reward_type:'unknown',official_website:null,program_url:null,source_url:null,evidence_quote:null,checked_at:now.toISOString(),valid_until:new Date(now.getTime()+7*86400000).toISOString(),check_outcome:'not_found'};
  try {
    const home = await read(website);
    const homeHost = new URL(home.url).hostname;
    if (homeHost==='producthunt.com' || homeHost.endsWith('.producthunt.com')) throw new Error('website_unresolved');
    result.official_website = home.url;
    if (/\/(?:affiliates?|referral(?:-program)?)(?:\/|$)/i.test(new URL(home.url).pathname)) {
      const parsed = analyzeProgram(home.html);
      if (parsed.status !== 'unknown') return {...result,...parsed,program_url:home.url,source_url:home.url,check_outcome:'checked'};
    }
    const links = programLinks(home.html,home.url);
    for (const link of links) {
      if (budget <= 0) break;
      let page: {url:string;html:string};
      try {page = await read(link);} catch (e) {
        result.check_outcome = e instanceof Error ? e.message : 'failed';
        continue;
      }
      const parsed = analyzeProgram(page.html);
      if (parsed.status !== 'unknown' || parsed.reward_type === 'credit') {
        return {...result,...parsed,program_url:page.url,source_url:page.url,check_outcome:'checked'};
      }
    }
  } catch (e) {result.check_outcome = e instanceof Error ? e.message : 'failed';}
  return result;
}

export async function refreshAffiliates(db: D1Database, http: typeof fetch = fetch, now = new Date()): Promise<void> {
  const iso = now.toISOString();
  await db.prepare(`INSERT OR IGNORE INTO scout_affiliate_checks(product_id,next_check_at)
    SELECT product_id,? FROM scout_products p WHERE platform='producthunt'
    AND NOT EXISTS(SELECT 1 FROM scout_affiliate_checks c WHERE c.product_id=p.product_id)
    ORDER BY last_seen_at DESC LIMIT 100`).bind(iso).run();
  const rows = await db.prepare(`SELECT c.product_id,p.payload_json FROM scout_affiliate_checks c JOIN scout_products p USING(product_id)
    WHERE c.next_check_at<=? AND (c.lease_until IS NULL OR c.lease_until<?) ORDER BY c.next_check_at,c.product_id LIMIT 2`)
    .bind(iso,now.getTime()).all<{product_id:string;payload_json:string}>();
  for (const row of rows.results) {
    const lease = crypto.randomUUID();
    const claimed = await db.prepare(`UPDATE scout_affiliate_checks SET lease_token=?,lease_until=? WHERE product_id=? AND (lease_until IS NULL OR lease_until<?)`)
      .bind(lease,now.getTime()+120000,row.product_id,now.getTime()).run();
    if (!claimed.meta.changes) continue;
    const payload = JSON.parse(row.payload_json);
    const website = payload.website_url ?? payload.website;
    const evidence = typeof website === 'string' ? await checkAffiliate(website,http,now) : null;
    const outcome = evidence?.check_outcome ?? 'website_missing';
    await db.prepare(`UPDATE scout_affiliate_checks SET document_json=CASE WHEN ?='checked' OR document_json IS NULL THEN ? ELSE document_json END,
      outcome=?,checked_at=?,next_check_at=?,lease_token=NULL,lease_until=NULL WHERE product_id=? AND lease_token=?`)
      .bind(outcome,JSON.stringify(evidence),outcome,iso,new Date(now.getTime()+(outcome==='checked'?7:1)*86400000).toISOString(),row.product_id,lease).run();
  }
}

export async function affiliateEvidence(db:D1Database, ids:string[], now = new Date()):Promise<Map<string,unknown>> {
  if (!ids.length) return new Map();
  const rows=await db.prepare(`SELECT * FROM scout_affiliate_checks WHERE product_id IN (SELECT value FROM json_each(?))`).bind(JSON.stringify(ids)).all<{product_id:string;document_json:string|null;outcome:string;checked_at:string}>();
  return new Map(rows.results.map(row=> {
    const value = row.document_json ? JSON.parse(row.document_json) : null;
    return [row.product_id,{...value,status:value?.status ?? 'unknown',last_check_at:row.checked_at,check_outcome:row.outcome,
      stale:!value || value.valid_until<now.toISOString() || row.outcome!=='checked'}];
  }));
}
