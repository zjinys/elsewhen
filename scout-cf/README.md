# scout-cf — Elsewhen 情报采集（Cloudflare Workers 版）

Python 版 scout（`../scout/`，部署在 Tower 8799）的 TS 重写，跑在 Cloudflare Workers 上。
出口在海外，解锁了 Tower 直连不通的商业情报源（Gumroad、CWS 等）。

## 架构

| Python 版 | CF 版 |
|---|---|
| APScheduler 60s tick | Cron Triggers `*/10 * * * *`（源按各自 interval 到期才采）|
| SQLite | D1 `elsewhen-scout`（同一套四表：sources/snapshots/findings/runs）|
| FastAPI + Bearer | Worker fetch handler + Bearer（同形状 API）|
| sources.toml | `src/sources.ts` 内的 `SOURCES` 注册表（spec 即代码）|
| httpx 执行器 + redirect 拒绝 | `scoutFetch`（`redirect: "manual"` + 手动检查 3xx）|

快照 append-only、内容哈希 diff（new/changed/gone）、90 天全量+更早每日抽样、
单事务提交（D1 用 `batch()` 模拟）——设计不变。

## 命令

```bash
npx vitest run                  # 单测（diff/registry）
npx tsc --noEmit                # 类型检查
npx wrangler deploy             # 部署（需 CLOUDFLARE_API_TOKEN）
npx wrangler tail               # 看实时日志（cron 采集输出 "scout: <id>: ..."）
npx wrangler d1 migrations apply elsewhen-scout --remote
npx wrangler d1 execute elsewhen-scout --remote --json --command "SELECT ..."
```

凭据：`CLOUDFLARE_API_TOKEN`（beadloom 的 `.env` 里有）；Workers Secrets 已设
`SCOUT_TOKEN`（API bearer）与 `GH_TOKEN`（GitHub search 配额）。

## API（Bearer SCOUT_TOKEN）

Affiliate 补充采集需要迁移 `0006_affiliate_evidence.sql`：每次 cron 最多检查两个 PH 产品，沿官方 Link 到官网后跟随最多两个推广入口，保留条款原文/URL/核查时间。最多六请求、8 秒/请求、512 KiB/页，七天复查，一天重试未知结果；失败保留旧证据并标过期，不影响榜单基线。Affiliate 仅作为 PH 产品卡片属性展示，不提供单独查询条件；详情与列表的 `affiliate` 对象支持 confirmed/unavailable/unknown。仅额度奖励不标为可推广，未查到不代表没有。

`GET /health`（免鉴权）· `/sources` · `/findings?source=&kind=&since=&limit=` ·
`/findings/{id}`（含 item 时间线）· `/snapshots/{id}` · `/runs?source=&limit=`
· `/probe?url=https://…[&full=1][&headers={json}]`（源开发调试）
· `/admin`（**管理页**，浏览器打开，token 存 localStorage）

鉴权现状：**单 token**（`SCOUT_TOKEN`，精确相等比较），一个部署只有一个有效
凭证，所有设备/人共用、无法单独撤销；且未设置 secret 时 API 完全放行。
多 token/多用户（身份、`reader`/`admin` 角色、单独撤销、`last_used_at` 审计）
已登记为未来扩展，落点与「为什么不引入 Cloudflare Access」见提案
`docs/notes/proposed/product/2026-11-15-watcher-data-collection-and-analysis.md` §9.1 E1。

管理 API（管理页内部使用，也可直接调）：`GET /admin/sources`（含禁用源+最近状态）、
`PATCH /admin/sources/{id}`（enabled / interval_minutes，用户粘性不被 deploy 覆盖）、
`POST /admin/trigger {source}`（手动强制跑一轮，无视间隔与开关）、
`GET/PUT /admin/settings/cws_ids`（可选额外跟踪，支持商店链接或 ID，最多 20 个；留空正常自动发现）。

## 源清单（当前 6 个）

| id | 内容 | 间隔 | 提取方式 |
|---|---|---|---|
| github-newcomers | GitHub 近 90 天创建、超过 50 星的全语言项目，按 stars 取前 30 | 12h | JSON API；滚动日期不重置 baseline |
| producthunt-daily | PH 每日精选 50 条 | 6h | Atom feed |
| appstore-us-top-free | App Store 美区免费榜 | 12h | Apple RSS JSON + 一次批量 ID lookup 补简介 |
| gumroad-notion / gumroad-digital-planner | Gumroad 搜索结果 | 12h | **Inertia data-page JSON**（`props.search_results.products`：name/price/ratings/permalink）|
| cws-discover | Chrome 商店扩展发现页；名称、简介、评分和原始链接 | 12h | 公开页面可访问性关联字段；可选额外跟踪补详情 |

加新源 = 在 `SOURCES` 里加一个条目（jsonSource / feedSource / inertiaSource /
cwsSource 四种工厂 + 自定义 fetch 函数），deploy 即生效。

迁移 0007 停用旧 `github-dart` / `github-rust` / `cws-watchlist`，保留历史观察和研究证据；当前来源 API 与后台只列仍注册的源。新范围独立建基线，首次采集不产生 findings。CWS 发现页是商店推荐样本，不代表全商店、新发布榜或增长排名；页面结构变化、空列表和缺失卡片字段会使本轮失败，保留上次成功基线。

### 采什么字段（以及为什么）

原则：**留下能回答「这个变化值不值得看」的字段，砍掉会让 hash 抖动的噪音。**

- GitHub：`stargazers_count` 之外保留 `forks_count` / `open_issues_count` /
  `language` / `topics` / `license_spdx_id` / `pushed_at` / `created_at` /
  `archived`。星数看不出「还活着吗」「能不能商用」「是不是官方的」。
- Gumroad：`price_cents` + `currency_code` 是竞品定价的核心观测值；
  `ratings_average` / `ratings_count` **拆成两个标量**（整块对象变化时算不出
  `diff`）；`seller_name` 是「谁在做这个生意」。
  **不收** `buyer_currency_*`（按买家 IP 变，会让每条都误报 changed）。
- App Store：`rank` 由条目在 feed 里的**下标**给出（`withRank: true`）。榜单本身不含用途简介，采集最多 25 条后一次请求 Apple lookup，按 `trackId` 补商店简介、分类、评分/评价数、下载标价与币种。lookup 响应最多 2 MiB，简介最多 16000 字符；缺项/失败让本轮失败，不发布缩水基线；新增字段参与 specHash，首轮重建基线，不制造整榜假变化。免费只表示下载标价，不推断内购、销量或收入。
- Product Hunt / Atom：补 `author` 与 `updated`——`updated` 随票数/排名刷新变化，
  与 `published` 分开才能区分「内容改了」和「热度变了」。
  HTML 与 XML 使用 htmlparser2 统一解析，最多解开四层转义，兼容 CDATA/XHTML，移除标签、隐藏脚本与尾部 Discussion/Link 导航。客户端也清洗旧镜像的展示文本，保留原始证据 payload。
- CWS：详情侧栏的 `Offered by` / `Updated` / `Size` 与评分同页，零额外请求。

**CWS 必须先判定「这一页是不是它的」再取数据。** 详情页靠 `canonical` 与
`<title>` 认领：真正的详情页 canonical 指向自身、title 是「<名字> - Chrome Web
Store」。下架/未知 ID 的页面有两种形态，实测都出现过：跳首页 `/`（含 87 张
别人的卡片），或停在 `/detail/empty-title/<id>`（**没有 canonical**，title 只是
商店名，但页面仍有 ~710KB 推荐卡片，含邻居的 name/rating）。

旧实现在匹配不到时用泛正则兜底，于是 uBlock Origin（`cjpalh…`）在 empty-title
页上抓到了邻居 uBlock Origin Lite 的 `4.5 / 3831 / 22000000`，且因数据恒定不变
`itemHash` 永不变，从不产生 finding —— 一个静默读了几个月别人数据的假条目。
**「匹配到了数据」不等于「这是它的数据」**：现在拿不到校验过的 `name` 就直接报
`not available`，不再猜。

**统一的排除项**：图片 URL（`thumbnail_url` / `artworkUrl100`）不收——CDN 换
签名参数就等于「内容变了」，会制造大量无意义的 changed。

### changed 带字段级 delta

`diff.ts` 对 `changed` 计算 `delta: {字段: {from, to, diff?}}`（`diff` 仅在两侧
都是数字时给出），单独存 `delta_json` 列、随 `/findings` 返回。
「+34 stars」比裸的「57225 stars」有用得多：光看绝对值无法判断它是刚涨的还是
本来就在那。上限 12 个字段，防止 payload 膨胀。

### 改字段清单必须 bump `shape`

`specHash` = `id + 采集形状指纹`（jsonSource 用 `fields` 清单，feedSource/cwsSource
用 `FEED_SHAPE`/`CWS_SHAPE` 常量）。**改抽取字段或逻辑后要同步 bump 指纹**，
否则麻烦在下面这条：

`itemHash` 基于字段值，字段清单一旦增减，**每条 item 的 hash 都会变**。没有
基线重置时，改完第一轮会把整个源刷成 `changed`（实测 GitHub Rust 的一次改字段
产出 30 条假变化）。`index.ts` 因此在 `prevShape !== spec.specHash` 时只更新
快照、产出 0 条 findings，并在 `/admin/trigger` 响应里标 `baseline_reset: true`。
判据很清楚：改采集是「我们这边动手了」，不是「数据世界发生了什么」，不该进
findings 流。

## 运维备忘

- 源的 enabled / interval 在 DB 里是**用户粘性**的：管理页改过之后 deploy 不会覆盖
  （代码 spec 只刷新 name/spec_hash）；删掉代码里的源条目后 DB 行保留但不再被采集。
- ** INTERVAL_OVERRIDES**（wrangler.jsonc）可临时缩短某源间隔做验证，用完改回 `{}`。
- `docker restart` 式的坑不存在这里，deploy 即全量替换。
- Workers 平台差异：`redirect: "error"` 非法（只允许 follow/manual）；CWS 无 slug
  URL 会 301 补 slug（同域，代码里跟随一次）。
- 查线上实际拿到的页面用 `wrangler tail` + 临时 `console.log`；`/probe` 不行
  ——它用浏览器 UA 且 `redirect: follow`，跳转形态与采集（`scout-cf/0.1` +
  manual）不同。同一个 ID 在 `/probe` 里表现为「跳首页」，线上却是
  「停在 empty-title」——排查 CWS 时必须看真实采集路径。
- 本机访问 `workers.dev` 需走代理（clash 7897）；绑自有域名可绕开（未做）。
- Tower 的 Python 版（8799）仍在跑 GitHub 源，作为过渡；稳定后 `docker rm -f elsewhen-scout` 下线。
