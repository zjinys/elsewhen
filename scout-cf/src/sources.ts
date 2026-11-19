/**
 * Source specs. On Workers there is no filesystem, so sources are code.
 * Same contract as the Python service: every source has a stable id, an
 * interval, and a fetch() that returns normalized items (each with `id`).
 */

export interface SourceSpec {
  id: string;
  name: string;
  enabled: boolean;
  intervalMinutes: number;
  /**
   * 采集形状指纹：id + 字段清单（+ 采集逻辑版本）。
   *
   * 它参与 diff 的基线判定——**形状变了就重置基线**。改字段清单一事
   * 本身不是「数据世界发生了什么」，若照常比对 itemHash，改完第一轮
   * 会把每条都报成 changed（hash 基于字段值，字段多了 hash 必变），
   * 用户看到几十条假变化。见 index.ts 的 baselineReset 分支。
   */
  specHash: string;
  fetch: (env: Env, ctx: FetchCtx) => Promise<Item[]>;
}

export interface FetchCtx {
  /** fetch with scout defaults (UA, timeout-ish via signal) */
  http: typeof fetch;
  env: Env;
}

export type Item = Record<string, unknown> & { id: string };

import type { Env } from "../worker-configuration";
function hashSpec(id: string, shape?: string): string {
  // cheap stable hash of the id (+ 采集形状) is enough for display purposes
  let h = 0x811c9dc5;
  const s = shape ? `${id}|${shape}` : id;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16).padStart(8, "0") + "-v2";
}

export function makeSpec(
  s: Omit<SourceSpec, "specHash" | "enabled" | "name"> &
    Partial<Pick<SourceSpec, "enabled" | "name">> & { shape?: string }
): SourceSpec {
  const { shape, ...rest } = s;
  return {
    ...rest,
    name: s.name ?? s.id,
    enabled: s.enabled ?? true,
    specHash: hashSpec(s.id, shape),
  };
}

// ---------- shared http helper ------------------------------------------

export function scoutFetch(input: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers);
  if (!headers.has("User-Agent")) {
    headers.set(
      "User-Agent",
      "scout-cf/0.1 (+personal data collector)"
    );
  }
  // Workers 平台只允许 follow/manual；manual + 手动检查 3xx 等效于禁重定向
  return fetch(input, { ...init, headers, signal: init?.signal ?? AbortSignal.timeout(25000), redirect: "manual" });
}

/**
 * 把 `fields` 里的点路径读出来，写成人能读的扁平 key。
 *
 * `license.spdx_id` → `license_spdx_id`（不是 `spdx_id`——那样丢了「这是
 * 许可证」的语义）；`ratings.average` → `ratings_average`。
 *
 * 拆成扁平 key 的另一个原因：**delta 才能给数值差**。整块 `ratings` 对象
 * 变化时只能给出 from/to 两个对象，看不出评分涨了 0.1；拆开就能算 diff。
 */
export function assignFields(
  target: Record<string, unknown>,
  src: unknown,
  fields: string[]
): void {
  for (const f of fields) {
    const v = dotPath(src, f);
    if (v === undefined || v === null) continue;
    target[f.replace(/\./g, "_")] = v;
  }
}

// ---------- JSON dot-path (same semantics as Python) ---------------------

export function dotPath(obj: unknown, path: string): unknown {
  let cur: unknown = obj;
  for (const part of path.split(".")) {
    if (cur && typeof cur === "object" && part in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[part];
    } else if (Array.isArray(cur) && /^\d+$/.test(part) && Number(part) < cur.length) {
      cur = cur[Number(part)];
    } else {
      return undefined;
    }
  }
  return cur;
}

// ---------- sources ------------------------------------------------------

async function fetchJsonSource(env: Env, cfg: JsonSourceCfg): Promise<Item[]> {
  const headers: Record<string, string> = {};
  if (cfg.tokenEnv && env[cfg.tokenEnv as keyof Env]) {
    headers["Authorization"] = `Bearer ${env[cfg.tokenEnv as keyof Env] as string}`;
  }
  if (cfg.headers) Object.assign(headers, cfg.headers);

  const url = new URL(cfg.url);
  for (const [k, v] of Object.entries(cfg.params ?? {})) {
    url.searchParams.set(k, v);
  }
  const resp = await scoutFetch(url.toString(), { headers });
  if (resp.status >= 300 && resp.status < 400) {
    throw new Error(`unexpected redirect ${resp.status} (redirects disabled)`);
  }
  if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  const data = (await resp.json()) as unknown;
  if (data && typeof data === 'object' && (data as Record<string,unknown>).incomplete_results === true) {
    throw new Error('collection_incomplete_results');
  }

  const raw = dotPath(data, cfg.itemsPath);
  if (!Array.isArray(raw)) throw new Error(`items_path ${cfg.itemsPath} is not a list`);
  const out: Item[] = [];
  for (let idx = 0; idx < raw.length; idx++) {
    const r = raw[idx];
    if (!r || typeof r !== "object") throw new Error("collection_invalid_item");
    const idVal = dotPath(r, cfg.itemId);
    if (idVal === undefined || idVal === null) throw new Error("collection_missing_stable_id");
    const item: Item = { id: String(idVal) };
    // 榜单类源（App Store 排行榜）：数组下标就是排名，排名变化是
    // 榜单类源最重要的信号，而 API 不直接给这个字段。
    if (cfg.withRank) item.rank = idx + 1;
    if (cfg.fields) assignFields(item, r, cfg.fields);
    else Object.assign(item, r);
    out.push(item);
  }
  return out;
}

interface JsonSourceCfg {
  url: string;
  params?: Record<string, string>;
  headers?: Record<string, string>;
  tokenEnv?: string;
  itemsPath: string;
  itemId: string;
  fields?: string[];
  /** 数组下标即排名（榜单类源）：写入 item.rank。 */
  withRank?: boolean;
}

export function jsonSource(
  id: string,
  cfg: JsonSourceCfg & { intervalMinutes: number; name?: string }
): SourceSpec {
  return makeSpec({
    id,
    name: cfg.name ?? id,
    intervalMinutes: cfg.intervalMinutes,
    // 字段清单一变，specHash 就变 → 服务端重置基线而不是报全量 changed。
    shape: JSON.stringify({url:cfg.url,params:cfg.params??{},itemsPath:cfg.itemsPath,itemId:cfg.itemId,fields:cfg.fields??[],withRank:cfg.withRank??false}),
    fetch: async (env) => fetchJsonSource(env, cfg),
  });
}

// ---------- Inertia page (data-page JSON) -------------------------------

/** Gumroad 等 Laravel/Inertia 站点：页面数据在 <div data-page="{json}"> 里。
 * 比 CSS 选择器稳：结构化 JSON，不依赖 DOM 层级。 */
async function fetchInertiaSource(
  env: Env,
  cfg: InertiaSourceCfg
): Promise<Item[]> {
  const headers: Record<string, string> = {
    Accept: "text/html, application/xhtml+xml",
  };
  if (cfg.headers) Object.assign(headers, cfg.headers);
  const url = new URL(cfg.url);
  for (const [k, v] of Object.entries(cfg.params ?? {})) url.searchParams.set(k, v);
  const resp = await scoutFetch(url.toString(), { headers });
  if (resp.status >= 300 && resp.status < 400) {
    throw new Error(`unexpected redirect ${resp.status}`);
  }
  if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  const html = await resp.text();

  const m = html.match(/data-page="([^"]+)"/);
  if (!m) throw new Error("no data-page attribute (not an Inertia page or bot-walled)");
  const page = JSON.parse(
    m[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/&#x27;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
  ) as Record<string, unknown>;
  const raw = dotPath(page, cfg.itemsPath);
  if (!Array.isArray(raw)) throw new Error(`items_path ${cfg.itemsPath} not found in data-page`);
  const out: Item[] = [];
  for (const r of raw) {
    if (!r || typeof r !== "object") throw new Error("collection_invalid_item");
    const idVal = dotPath(r, cfg.itemId);
    if (idVal === undefined || idVal === null) throw new Error("collection_missing_stable_id");
    const item: Item = { id: String(idVal) };
    if (cfg.fields) assignFields(item, r, cfg.fields);
    else Object.assign(item, r as Record<string, unknown>);
    out.push(item);
  }
  return out;
}

interface InertiaSourceCfg {
  url: string;
  params?: Record<string, string>;
  headers?: Record<string, string>;
  itemsPath: string;
  itemId: string;
  fields?: string[];
}

export function inertiaSource(
  id: string,
  cfg: InertiaSourceCfg & { intervalMinutes: number; name?: string }
): SourceSpec {
  return makeSpec({
    id,
    name: cfg.name ?? id,
    intervalMinutes: cfg.intervalMinutes,
    shape: JSON.stringify({url:cfg.url,params:cfg.params??{},itemsPath:cfg.itemsPath,itemId:cfg.itemId,fields:cfg.fields??[]}),
    fetch: async (env) => fetchInertiaSource(env, cfg),
  });
}

// ---------- RSS / Atom ---------------------------------------------------

function clean(s: string | null): string | null {
  if (!s) return null;
  const v = s
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return v || null;
}

/** Minimal XML node view over workerd's XMLParser (2024+). */
interface XNode {
  name: string;
  children: XNode[];
  text?: string;
  attrs?: Record<string, string>;
}

function findChild(n: XNode, tag: string): XNode | null {
  return n.children.find((c) => c.name.split(":").pop() === tag) ?? null;
}

function nodeText(n: XNode | null): string | null {
  if (!n) return null;
  return (n.text ?? n.children.map((c) => c.text ?? "").join("").trim()) || null;
}

function atomItem(entry: XNode, idField: string): Item | null {
  const title = clean(nodeText(findChild(entry, "title")));
  const linkNode = findChild(entry, "link");
  const link = linkNode?.attrs?.href ?? nodeText(linkNode);
  const summary = clean(
    nodeText(findChild(entry, "summary")) ??
      nodeText(findChild(entry, "description")) ??
      nodeText(findChild(entry, "content"))
  );
  const published =
    nodeText(findChild(entry, "published")) ??
    nodeText(findChild(entry, "pubDate")) ??
    nodeText(findChild(entry, "updated"));
  // Atom 的 updated 与 published 是两个不同信号：PH 的 updated 会随
  // 「排名/票数刷新」变化，而 published 是发布时刻。都留下，让 changed
  // 能区分「内容改了」和「热度变了」。
  const updated = nodeText(findChild(entry, "updated"));
  // author 是「谁做的」——PH 上就是 Maker，判断需求方/供给方很关键。
  const authorNode = findChild(entry, "author");
  const author = clean(
    authorNode
      ? nodeText(findChild(authorNode, "name")) ?? nodeText(authorNode)
      : null
  );

  const item: Record<string, unknown> = {};
  if (title) item.title = title;
  if (link) item.link = link;
  if (summary) item.summary = summary;
  if (published) item.published = published;
  if (updated) item.updated = updated;
  if (author) item.author = author;

  const key =
    idField === "link"
      ? link
      : idField === "title"
        ? title
        : nodeText(findChild(entry, "id")) ?? nodeText(findChild(entry, "guid"));
  if (!key) return null;
  item.id = key;
  return item as Item;
}

async function fetchFeed(env: Env, cfg: { url: string; itemId?: string }): Promise<Item[]> {
  const resp = await scoutFetch(cfg.url);
  if (resp.status >= 300 && resp.status < 400) {
    throw new Error(`unexpected redirect ${resp.status} (redirects disabled)`);
  }
  if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  const body = await resp.text();

  // workerd exposes XMLParser (namespace XMLParser) since 2024; in plain
  // node/vitest it falls back to a tiny regex tokenizer for tests.
  const root = parseXml(body);
  if (!root) throw new Error("unparseable feed");
  const isAtom = root.name === "feed";
  const entryTag = isAtom ? "entry" : "item";
  const entries = (root.children ?? []).filter(
    (c) => c.name.split(":").pop() === entryTag || (isAtom ? false : c.name === "item")
  );
  // rss: entries live under channel
  const holder = isAtom
    ? root
    : (root.children ?? []).find((c) => c.name === "channel") ?? root;
  const list = isAtom
    ? (root.children ?? []).filter((c) => c.name.split(":").pop() === "entry")
    : (holder.children ?? []).filter((c) => c.name === "item");

  const idField = cfg.itemId ?? "id";
  const out: Item[] = [];
  for (const e of list) {
    const item = atomItem(e, idField);
    if (item) out.push(item);
  }
  return out;
}

/** XMLParser (workerd) with regex fallback for vitest. */
function parseXml(body: string): XNode | null {
  type Parsed = { root: XNode };
  const g = globalThis as unknown as {
    XMLParser?: new () => { parse: (s: string) => Parsed };
  };
  if (g.XMLParser) {
    return new g.XMLParser().parse(body).root;
  }
  // fallback: stack-based tag tokenizer good enough for well-formed feeds
  const stack: XNode[] = [];
  let root: XNode | null = null;
  const re = /<\/?([A-Za-z][\w:.-]*)((\s+[\w:.-]+="[^"]*")*)\s*\/?>|([^<]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(body))) {
    const [full, tagName, attrs, , text] = m;
    if (text !== undefined) {
      const top = stack[stack.length - 1];
      if (top) top.text = (top.text ?? "") + text;
      continue;
    }
    if (full.startsWith("</")) {
      stack.pop();
      continue;
    }
    const node: XNode = { name: tagName, children: [], attrs: {} };
    const attrRe = /([\w:.-]+)="([^"]*)"/g;
    let am: RegExpExecArray | null;
    while ((am = attrRe.exec(attrs ?? ""))) node.attrs![am[1]] = am[2];
    const parent = stack[stack.length - 1];
    if (parent) parent.children.push(node);
    else if (!root) root = node;
    if (!full.endsWith("/>")) stack.push(node);
  }
  return root;
}

/** Atom/RSS 抽取的字段集：改动 atomItem 的输出就要 bump 这个版本号。 */
const FEED_SHAPE = "atom:v2(title,link,summary,published,updated,author)";

export function feedSource(
  id: string,
  cfg: { url: string; itemId?: string; intervalMinutes: number; name?: string }
): SourceSpec {
  return makeSpec({
    id,
    name: cfg.name ?? id,
    intervalMinutes: cfg.intervalMinutes,
    shape: JSON.stringify({parser:FEED_SHAPE,url:cfg.url,itemId:cfg.itemId??'id'}),
    fetch: async (_env) => fetchFeed(_env, cfg),
  });
}

// ---------- Chrome Web Store (watchlist) -------------------------------

/** CWS 详情页：数据在 AF_initDataCallback 的 ds:1 JSON 里，页面无 JS 渲染也能
 * 提到 name/rating/ratingCount/userCount。监控形态 = 竞品清单：每个扩展一个
 * item，id = 扩展 ID，追踪安装量与评分的时间轨迹。 */
import { getSetting } from "./store";

async function fetchCwsWatchlist(
  env: Env,
  cfg: { extensionIds: string[] }
): Promise<Item[]> {
  // 管理页保存的额外竞品 ID（settings 表）与代码内置清单合并去重
  const extraRaw = await getSetting(env.DB, "cws_extra_ids");
  const ids = [...new Set([...cfg.extensionIds, ...(extraRaw ? extraRaw.split(",") : [])])];
  const out: Item[] = [];
  for (const extId of ids) {
    // 直接用 slug 形式 URL 免跳转；slug 不参与匹配，仅是路径要求。
    // 无 slug/任意 slug 的 URL 可能 301/302 补全 slug：同域跳转跟随一次。
    const url = `https://chromewebstore.google.com/detail/x/${extId}?hl=en`;
    let resp = await scoutFetch(url);
    let finalUrl = url;
    if (resp.status >= 300 && resp.status < 400) {
      const loc = resp.headers.get("location");
      if (loc) {
        const next = new URL(loc, "https://chromewebstore.google.com/");
        if (next.origin === "https://chromewebstore.google.com") {
          finalUrl = next.toString();
          resp = await scoutFetch(finalUrl);
        }
      }
    }
    if (!resp.ok) {
      // 单个失败不拖垮整批（下架/地区限制等），留痕后继续
      out.push({ id: extId, error: `HTTP ${resp.status}` });
      continue;
    }
    out.push(parseCwsDetail(await resp.text(), finalUrl, extId));
  }
  return out;
}

/** 从一段 HTML 里取第一行纯文本：去标签、把 `<br>` 当换行、解常见实体。
 * 侧栏的 Developer 行是「公司名<br>街道<br>城市邮编<br>国家」的多行地址，
 * 只取第一行——否则改一次地址就刷出假 changed。 */
function firstTextLine(fragment: string): string {
  const lines = fragment
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .split("\n")
    .map((l) => l.trim().replace(/\s+/g, " "));
  return lines.find((l) => l.length > 0) ?? "";
}

/** CWS 详情侧栏的 key/value 行。两种形态实测并存：
 *  - `<div class="QDHp8e">Offered by</div><div>Raymond Hill (gorhill)</div>`
 *  - `<div class="QDHp8e">Developer</div><div><div class="yyjN4">
 *     <div class="mdSapd">Consensys Software Inc.<br>5049 Edwards Ranch Rd…</div>`
 * 值可能嵌在多层 div 里，所以不能要求「value 里没有标签」（旧正则正是
 * 因此把 MetaMask 的开发者取成空串丢掉了）。这里只看 key 之后的片段，
 * 取第一行文本。 */
export function parseCwsSidebar(html: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of html.matchAll(/<div class="QDHp8e">([^<]{1,40})<\/div>/g)) {
    const key = m[1].trim().toLowerCase();
    if (out[key]) continue; // 同名行只取第一处
    // 值区间：从 key 之后到本行结束（`</li>` / 下一个 key 行 / 800 字符封顶）。
    // 不截断会把后面的 Size、Languages 一起吞进来。
    const after = html.slice(m.index! + m[0].length);
    const stop = after.search(/<\/li>|<li\b|<div class="QDHp8e">/);
    const region = (stop >= 0 ? after.slice(0, stop) : after).slice(0, 800);
    const first = firstTextLine(region);
    if (first) out[key] = first;
  }
  return out;
}

/** CWS 详情页解析（纯函数，便于针对三种页面形态做单测）：
 *  - 正常页：/detail/<slug>/<id>，canonical 指向自身
 *  - 已下架：跳首页 /，页面里全是商店卡片（含 87 条别人的数据）
 *  - ID 不存在：/detail/empty-title/<id>，页面无任何 lh3 数据
 * 只看「有没有匹配到数据」不够——首页也匹配得到，但那是邻居的数据。 */
export function parseCwsDetail(
  html: string,
  finalUrl: string,
  extId: string
): Item {
  const canonical = html.match(/<link rel="canonical" href="([^"]*)"/)?.[1];
  const pathname = new URL(finalUrl).pathname;
  const canX = canonical?.match(/\/detail\/([^/?]+)\/([a-z]{32})/);
  const titleM = html.match(/<title>([^<]{1,120})<\/title>/);
  const titleName = titleM?.[1].replace(/\s*-\s*Chrome Web Store\s*$/, "").trim();
  // 真正的详情页一定有 canonical、且 title 是「<名字> - Chrome Web Store」。
  // 已下架 / ID 不存在的页面两种情况实测都出现过：
  //   跳首页 /（canonical 指向商店首页，页面含 87 张别人的卡片）
  //   停在 /detail/empty-title/<id>（**没有 canonical**，title 只是商店名，
  //   但页面仍有 700KB 的推荐卡片，含邻居的 name/rating）
  // 因此「匹配到了数据」绝不能当成「这是它的数据」——这一条曾让 uBlock
  // Origin 长期显示 uBlock Origin Lite 的评分与安装量，且因数据恒定不变
  // 永不产生 finding，是个静默的假条目。
  if (
    pathname === "/" ||
    canonical === "https://chromewebstore.google.com/" ||
    !canX ||
    !titleName ||
    titleName === "Chrome Web Store"
  ) {
    return { id: extId, error: "not available (unpublished or unknown id)" };
  }
  if (canX[2] !== extId) {
    return { id: extId, error: `page mismatch (canonical points to ${canX[2]})` };
  }
  const item: Item = { id: extId };
  const name = titleName;
  item.name = name;
  // rating / rating_count：形状 "<img>","<name>",<rating>,<count>,"<img>"。
  // 必须用已校验的 name 当锚点——页面靠前位置是「相关扩展」列表，
  // 首个泛匹配会拿到邻居的数据（实测 MetaMask 页首个命中是它自己，
  // 但首页首个命中是别的扩展）。rating 可为长小数，count 可为 0。
  const esc = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const rM = html.match(
    new RegExp(`"((?:[^"\\\\]|\\\\.){1,200})","${esc}",([\\d.]+),(\\d+)`)
  );
  if (!rM) return { id: extId, error: "pattern miss (rating anchor not found)" };
  item.rating = Math.round(parseFloat(rM[2]) * 100) / 100;
  item.rating_count = parseInt(rM[3], 10);
  // 详情侧栏（Offered by / Developer / Updated / Size）——同一页已有，
  // 零额外请求。开发者是「谁在竞争」，更新日期是「还维护吗」，体积影响
  // 安装转化，三者都是竞品判断的实际输入。
  // 开发者优先取 `Offered by`（干净的单行），退回 `Developer` 的首行
  // （后面跟着完整邮政地址，见 firstTextLine）。
  const rows = parseCwsSidebar(html);
  const dev = rows["offered by"] ?? rows["developer"];
  if (dev) item.developer = dev;
  if (rows["updated"]) item.updated = rows["updated"];
  if (rows["size"]) item.size = rows["size"];
  // user count：名字锚点后 3000 字符内，两种已知形状：
  //   uBlock:  ,1,1,22000000,null,   MetaMask: ,1,null,12000000,1,
  const anchor = html.match(new RegExp(`"${esc}",([\\d.]+),(\\d+),`));
  const tailStart = anchor ? anchor.index! + anchor[0].length : rM.index! + rM[0].length;
  const tail = html.slice(tailStart, tailStart + 3000);
  const um =
    tail.match(/,1,null,(\d{6,10}),(?:1|null),/) ?? tail.match(/,1,1,(\d{6,10}),null,/);
  if (um) item.users = parseInt(um[1], 10);
  return item;
}



interface JsonSourceCfg {
  url: string;
  params?: Record<string, string>;
  headers?: Record<string, string>;
  tokenEnv?: string;
  itemsPath: string;
  itemId: string;
  fields?: string[];
  /** 数组下标即排名（榜单类源）：写入 item.rank。 */
  withRank?: boolean;
}

const CWS_SHAPE = "cws:v5(name,rating,rating_count,developer,updated,size)";

export function cwsSource(
  id: string,
  cfg: { extensionIds: string[]; intervalMinutes: number; name?: string }
): SourceSpec {
  return makeSpec({
    id,
    name: cfg.name ?? id,
    intervalMinutes: cfg.intervalMinutes,
    shape: CWS_SHAPE,
    fetch: async (env) => fetchCwsWatchlist(env, cfg),
  });
}

// ---------- registry -----------------------------------------------------

/**
 * 各源的字段清单。原则：**把信号留下来，把噪音砍掉**。
 *
 * 采集端只保留能回答「这个变化值不值得看」的字段——排名/热度用于判断
 * 影响面，活跃度（pushed_at）用于判断是否还活着，许可证/价格用于判断
 * 商业化形态。文件名、URL 模板、node_id 这类 API 噪音不收（既占体积，
 * 又会让 itemHash 因无关字段变化而误报 changed）。
 */

/**
 * GitHub：星数是主信号，但从星数看不出「项目还活着吗」「是不是官方的」。
 * forks 反映二次开发热度，open_issues 反映维护压力，pushed_at 反映活跃度，
 * license 决定能否商用——这四个是判断「值不值得跟」的关键，且都在
 * 同一个 search 响应里，零额外请求。
 */
const GITHUB_FIELDS = [
  "full_name",
  "html_url",
  "description",
  "stargazers_count",
  "forks_count",
  "open_issues_count",
  "language",
  "topics",
  "license.spdx_id",
  "pushed_at",
  "created_at",
  "archived",
];

/** App Store 榜：RSS 只给 releaseDate/genres/artworkUrl100，排名由数组下标
 * 隐式给出——下标变化（榜内升降）本身就是海外榜单最重要的信号，所以额外
 * 记录 rank（由条目在 feed 中的位置决定，见 fetchJsonSource 的 rank 支持）。 */
const APPSTORE_FIELDS = [
  "name",
  "artistName",
  "url",
  "releaseDate",
  // 只取主分类（genres.0.name）：完整 genres 带 genreId 和 url，噪音大；
  // artworkUrl100 同理不收（图标 CDN 换签名会误报 changed）。
  "genres.0.name",
];

/**
 * Gumroad：价格是竞品定价策略的核心观测值，seller 是「谁在做这个生意」，
 * ratings 仅记录评价，不能推算销量；is_pay_what_you_want / quantity_remaining 反映
 * 促销与库存策略。只收这些，不收 buyer_currency_* 那组汇率噪音
 * （它按买家 IP 变化，会让每条都误报 changed）。
 */
const GUMROAD_FIELDS = [
  "name",
  "permalink",
  "url",
  "price_cents",
  "currency_code",
  // 拆成两个数值：整块对象变化时 delta 给不出 diff，拆开才能看到
  // 「评分从 4.7 涨到 4.9」「评价数 +12」。
  "ratings.average",
  "ratings.count",
  "seller.name",
  "seller.id",
  "is_pay_what_you_want",
  "quantity_remaining",
  "native_type",
];
// thumbnail_url 刻意不收：CDN 换签名参数就等于「内容变了」，会制造大量
// 无意义的 changed。同样的理由见 APPSTORE_FIELDS。



// Monitored scopes. Availability is checked per run; a listed scope is not a business claim.
export const SOURCES: SourceSpec[] = [
  // GitHub 仓库热榜（按语言轮换）— JSON API，token 提高配额
  jsonSource("github-dart", {
    url: "https://api.github.com/search/repositories",
    params: { q: "language:dart stars:>100 sort:stars", per_page: "30" },
    tokenEnv: "GH_TOKEN",
    itemsPath: "items",
    itemId: "id",
    fields: GITHUB_FIELDS,
    intervalMinutes: 720,
    name: "GitHub Dart trending",
  }),
  jsonSource("github-rust", {
    url: "https://api.github.com/search/repositories",
    params: { q: "language:rust stars:>500 sort:stars", per_page: "30" },
    tokenEnv: "GH_TOKEN",
    itemsPath: "items",
    itemId: "id",
    fields: GITHUB_FIELDS,
    intervalMinutes: 720,
    name: "GitHub Rust trending",
  }),

  // Product Hunt 官方 Atom feed（网页被 Cloudflare 盾但 feed 直连 200）
  feedSource("producthunt-daily", {
    url: "https://www.producthunt.com/feed",
    intervalMinutes: 360,
    name: "Product Hunt daily",
  }),

  // Gumroad discover 搜索结果（Inertia data-page JSON，Workers 出口 200）
  // 监控多个品类关键词，每个关键词一条源，稳定 ID = permalink
  inertiaSource("gumroad-notion", {
    url: "https://gumroad.com/discover",
    params: { query: "notion template" },
    itemsPath: "props.search_results.products",
    itemId: "permalink",
    fields: GUMROAD_FIELDS,
    intervalMinutes: 720,
    name: "Gumroad: notion template",
  }),
  inertiaSource("gumroad-digital-planner", {
    url: "https://gumroad.com/discover",
    params: { query: "digital planner" },
    itemsPath: "props.search_results.products",
    itemId: "permalink",
    fields: GUMROAD_FIELDS,
    intervalMinutes: 720,
    name: "Gumroad: digital planner",
  }),

  // Chrome Web Store 预设监控清单；指标只描述被监控扩展，不能代表整个市场。
  cwsSource("cws-watchlist", {
    // 无效 ID 或部分抓取使本轮失败，不据此发布整个集合的消失事件。
    extensionIds: [
      "ddkjiahejlhfcafbddmgiahcphecmpfh", // uBlock Origin Lite
      "nkbihfbeogaeaoehlefnkodbefgpgknn", // MetaMask
      // 已移除 cjpalhdlnbpafiamejdnhjbkeiagm（uBlock Origin）：该扩展已下架，
      // 详情页停在 /detail/empty-title/<id>，每轮只会留一条 not available。
      // 重新纳入前先确认 ID 能打开 /detail/<slug>/<id>（不是 empty-title）。
    ],
    intervalMinutes: 720,
    name: "CWS extension watchlist",
  }),

  // App Store 美国 free 榜（Apple 官方 RSS JSON）
  jsonSource("appstore-us-top-free", {
    url: "https://rss.marketingtools.apple.com/api/v2/us/apps/top-free/25/apps.json",
    itemsPath: "feed.results",
    itemId: "id",
    fields: APPSTORE_FIELDS,
    withRank: true,
    intervalMinutes: 720,
    name: "App Store US top free",
  }),
];

export function getSource(id: string): SourceSpec | undefined {
  return SOURCES.find((s) => s.id === id);
}
