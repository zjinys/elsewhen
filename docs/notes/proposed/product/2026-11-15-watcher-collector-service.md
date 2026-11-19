# 情报采集器（watcher-collector）服务设计

- 日期：2026-11-15
- 状态：`proposed`（设计定稿，未排期实施）
- 范围：**外置采集服务**的架构、源适配层、数据契约与部署。只负责"把外部数据变成快照"；diff、findings、提醒、UI 全部仍由 Elsewhen 承担（见姊妹提案）
- 姊妹提案：[2026-11-15-watcher-data-collection-and-analysis.md](2026-11-15-watcher-data-collection-and-analysis.md)（下称"主提案"）。本设计落地后，主提案的 D1/D7/§1.3/§6 需按文末修订点改写
- 核心变化：**采集整体移出 Elsewhen（含 GitHub 源），Elsewhen 内不再跑任何采集**，只做拉取 → diff → 研究/提醒

---

## 1. 为什么采集要外置（对主提案 §1.3 的修正）

主提案当初"内嵌不外置"的三条理由仍然成立，但指向的是**研究侧**而非**采集侧**：

1. 认知链路闭环靠"快照回流 app"保住——findings 依然流进记录/知识页/选品判断；
2. 业务原型依然是这套管线——而且交付周报本来就需要一台常驻机器，提前立起生产形态；
3. 内嵌的 UI/提醒/知识关联不变。

采集侧外置的实际收益：

- **常驻**：app 关闭期间照常采集（主提案 §6 承认的"app 关闭不采集"代价消除）；
- **反爬强度**：真实 HTTP 客户端（任意 method/header/代理），必要时可挂 headless 侧车；Lua 沙箱 + GET-only 的手机端 `http.fetch` 对 Cloudflare 后面的 Gumroad、需要 POST 的 PH GraphQL 基本裸奔（2026-11-15 查证结论，见 §6 源附录）；
- **拆掉脚本引擎这颗雷**：主提案 D1 的大部分复杂度（mlua 交叉编译、iOS 禁 JIT、QuickJS fallback、沙箱与授权流）服务于"在不可信环境跑动态脚本"——采集外置 + 单维护者 + AI 协作后，这一整块可以不做（§4 用内建适配器替代）；
- **资源隔离**：抓取、解析的内存/带宽/失败重试不占 app 进程。

隐私边界不破坏：采集器只采**公开市场数据**（传感器），不碰任何个人记录；快照回流后仍走本地存储与端到端加密同步。

---

## 2. 架构决策

| # | 决策 | 理由 |
|---|---|---|
| C1 | **独立单二进制小服务**：Rust + tokio 常驻调度，源适配编译内建（§4），单租户，`systemd` 托管 | 与 Elsewhen 同生态；单二进制 + 单配置 + 数据目录，运维成本压到最低 |
| C2 | **采集器是哑的**：只产快照，不做 diff / findings / 去重 / 提醒 | 保持"传感器"定位：好实现、可替换、挂了不丢数据（重拉即可）；diff 语义单一事实源在 app（主提案 D2） |
| C3 | **数据契约 = 快照 JSON**，与主提案 `watcher_snapshots` 同构（watcher 名、taken_at、run_id、status、items[]） | app 侧 schema 不返工；快照是唯一交付物 |
| C4 | **拉模式交付**：采集器暴露只读本地 HTTP API（bearer token），Elsewhen 的 background worker 到点拉新快照 | app 已有 worker tick 与"启动补跑"机制，复用最自然；推送（webhook/ntfy）后置为可选增量 |
| C5 | **不做脚本引擎**：源适配为编译内 Rust trait + 少量内建适配器类型，配置驱动（§4） | 单维护者 + AI 写适配器，重编译成本分钟级；移动端遗留的沙箱/授权复杂度整体删除；HTML/JSON 解析用成熟 crate（`scraper`/`serde`） |
| C6 | **密钥注入沿用命名密钥约定**：适配器配置里写 `{{secret:GITHUB_TOKEN}}`，宿主在构造时解析注入，密钥不进日志 | 与 Elsewhen `{{secret:...}}` 同款心智 |
| C7 | **快照本地保留 30 天**；长期留存（90 天全量 + 每日抽样）是 app 侧职责（主提案 §3） | 采集器存储保持薄；app 拉走即长期事实源 |
| C8 | **每 watcher 独立调度与退避**：一个源挂死不拖累其他源；连续失败退避至 6h 冷却（对齐主提案 §4.3 纪律） | 隔离性 |
| C9 | **GitHub 源走 GitHub REST + PAT**（`http` adapter），不经 MCP | 采集器侧无 MCP 基建，REST + token 最简单；主提案阶段 1 的"MCP 内建模板"随之取消 |
| C10 | **无多机、无集群**：单采集器实例；多副本需求出现再说 | 单人业务，YAGNI |

---

## 3. 数据契约（采集器 → Elsewhen）

### 3.1 快照文件（采集器落盘 + API 返回同一格式）

```json
{
  "watcher": "github-dart-trend",
  "taken_at": "2026-11-15T09:00:00Z",       // 全部 UTC
  "run_id": "0192f0a2-…",                    // 幂等键，app 用它去重
  "status": "ok",                            // ok | error | partial
  "error": null,                             // status != ok 时的诊断串（不含大段原文）
  "items": [
    { "id": "foo/bar", "name": "foo/bar", "url": "https://github.com/foo/bar",
      "stars": 1234, "desc": "…" }
  ]
}
```

- `items[].id` 即主提案的 `item_key`，**稳定性责任在适配器**；适配器实现或其参数变更视为基线重置（配合主提案"首次 run 只建基线不产 findings"规则）；
- `status=partial`：部分 item 失败但主体可用，app 照常 diff，meta 里有失败计数；
- 逐 watcher 一个目录，一次 run 一个文件：`data/<watcher>/<RFC3339-taken_at>-<run_id前8>.json`。

### 3.2 HTTP API（只读 + bearer token）

```
GET /v1/health                          → 200 {"ok":true}
GET /v1/watchers                        → 名称列表 + 每 watcher 最近 run meta
                                           （last_run_at、status、item_count、error、next_eta）
GET /v1/watchers/{name}/snapshots?after={run_id|ISO时间}
                                        → 该 watcher 的快照数组（旧→新），默认最近 1 份
```

- token 来自配置文件（0600）；Elsewhen 侧存为命名密钥（`{{secret:COLLECTOR_TOKEN}}` 同款注入）；
- 传输：同机/内网直接明文；跨公网走反代 HTTPS 或 Tailscale（部署章节定，采集器自身不做 TLS）；
- 分页不需要（单 watcher 单次 run ≤30 item，`after` 拉增量足够）；app 每 watcher 记录"已消费 run_id"，重启补拉（对齐主提案"启动补跑"）。

---

## 4. 源适配层（Rust 内建适配器，无脚本引擎）

服务端仍保留"动态脚本"的最后理由是免重编译加源——但单维护者 + AI 协作的现实里，新增/修改一个源的成本是"改几十行 Rust + 重编译 + 重启"（分钟级），而脚本引擎的代价（沙箱、宿主 API 面、对抗性测试、第二套语言生态）是持续的。因此**不做任何脚本引擎**，适配器是编译进二进制的代码：

```rust
#[async_trait]
trait Source {
    fn name(&self) -> &str;
    fn domains(&self) -> &[&str];                    // 出网声明（须 ⊆ 配置授权）
    async fn collect(&self, ctx: &Ctx) -> Result<Vec<Item>>;
    // Ctx 注入：白名单 http client（reqwest）、kv（种子/游标）、密钥、日志
}
```

### 4.1 内建适配器类型（首批 5 个）

| 类型 | 关键参数（toml） | 覆盖 |
|---|---|---|
| `github_search` | `query`、`token_ref`、`days` | GitHub trend（REST + PAT） |
| `cws_listing` | `seed_ids[]` 或 `seed_ids_kv` | CWS 详情页 JSON-LD（含"下架返回 200"防护） |
| `rss_feed` | `url`、字段映射 | PH Atom feed 等一切 feed |
| `http_json` | `method/url/headers/body 模板`、字段映射 | PH GraphQL（POST + token）、一般 REST |
| `manual_import` | 目录监听 / 文件路径 | 付费 SaaS 导出（TikTok 数据等） |

- 新数据源若属于已知类型 → **只改配置**；真新类型 → 写一个 ~100 行适配器；
- `http_json` 的字段映射保持极简（JSON 路径 → item 字段），复杂解析一律写适配器，**不发明 DSL**。

### 4.2 保留的护栏（宿主侧，针对配置与输出而非脚本）

1. 每 watcher 域名白名单：适配器声明 ⊆ toml 授权，重定向逐跳校验；
2. 请求预算 ≤40 次/run、出网 ≤20MB、墙钟 120s；响应体上限 2MB；
3. 输出校验：items ≤100 条、单字段 ≤4KB、`id` 必填非空（快照契约 §3.1）；
4. kv 限额 4MB/watcher；同 watcher 串行、watcher 间并发 ≤4；
5. 失败退避：1h → 2h → 4h → 6h 冷却（对齐主提案）。

原主提案的"对抗性测试清单"取消——没有不可信输入的执行面，仅保留输出 schema 校验。

### 4.3 密钥与代理

- 配置里的 `{{secret:...}}` 在适配器构造时解析为参数注入，不进日志；
- 每 watcher 可配 HTTP(S) 代理与自定义 UA（Cloudflare 场景预留）。

---

## 5. 部署形态

```toml
# /etc/watcher-collector/config.toml
[server]
listen = "127.0.0.1:7421"
token  = "{{secret:COLLECTOR_TOKEN}}"

[storage]
data_dir = "/var/lib/watcher-collector"   # 快照 + runs.db + kv

[watcher.github-dart-trend]
adapter = "github_search"
query   = "created:>{now-7d} language:dart stars:>50"
interval = "24h"
domains  = ["api.github.com"]
env.GITHUB_TOKEN = "{{secret:GITHUB_TOKEN}}"

[watcher.cws-watchlist]
adapter  = "cws_listing"
seed_ids_kv = "cws_seeds"          # kv 里的种子 ID 列表
interval = "24h"
domains  = ["chromewebstore.google.com"]

[watcher.ph-daily]
adapter  = "rss_feed"
url      = "https://www.producthunt.com/feed"
interval = "12h"
```

- 单二进制 + systemd unit（`Restart=on-failure`）；可部署在现有 VPS 上与既有服务并存；
- CLI 辅助（运维用，不算"手动触发按钮"）：`collector run <watcher>`（调试）、`collector check`（校验配置：适配器参数、域名授权、密钥引用）、`collector doctor`；
- Elsewhen 侧配置：采集器 URL + token，每 watcher 的名称与 `alert_policy` 照主提案 §3（`script`/`interval`/`domains` 字段从 app 的 watchers 表**移除**，采集器是唯一事实源）。

---

## 6. 源适配附录（2026-11-15 查证结论）

### 6.1 GitHub trend（REST + PAT，替代主提案的 MCP 模板）

- `GET https://api.github.com/search/repositories?q=created:>2026-11-08+language:dart&sort=stars`，`Authorization: Bearer {{secret:GITHUB_TOKEN}}`；
- `item_key = full_name`；两次快照间 stars 差值即趋势信号；
- 建议模板盯"近 N 天新建 + stars 阈值"而非全量 stars 榜（后者长期同批，diff 价值低）。

### 6.2 Chrome Web Store（`cws_listing` 适配器：GET + JSON-LD 解析）

- 详情页**服务端渲染**：`GET https://chromewebstore.google.com/detail/{id}?hl=en`，安装数/评分/版本在页面内嵌 JSON-LD（`interactionStatistic` / `aggregateRating`），无需浏览器；
- robots.txt **允许 `/detail/`**、禁止 `/search` 与 `/reviews`——合规抓法是"种子 ID 列表"（`store.kv` 维护，人工 + HN/Reddit 提及捞补），不做全店/搜索扫描；
- 坑：① 下架插件返回 **200 + 通用标题页**，适配器必须校验标题以 "Chrome Web Store" 结尾；② users 是周活、大插件按档位圆整——"周环比 >30%" 只对中小插件有效，阈值按量级分段；③ 建议配置限定"单 watcher ≤10 个 ID"，预算内一天一抓。

### 6.3 Product Hunt（双层）

- **零 token Atom feed**（`GET producthunt.com/feed`）：字段瘦但够检出"新品出现"（`item_key = slug`）；
- **GraphQL API v2**（免费开发者 token，永不过期）：`POST /v2/api/graphql` 拿 `votesCount` 轨迹、按 topic 过滤——`http` 的 POST 能力在此生效；
- **ToS 注记**：PH API 默认禁止商用，给客户交付含 PH 数据需联系 PH 授权（写进主提案阶段 3 风险清单）。

### 6.4 Gumroad / Lemon Squeezy（待实测）

- 无公开销量数据；可采 discover/分类页 HTML 的**评论数/评分/价格/新品上架**，用评论数增速做"起量"代理信号；
- 页面在 Cloudflare 后面，**先跑可达性 spike 再排期**；不行则上 Playwright 侧车（阶段 3）或放弃。

### 6.5 TikTok Shop（明确不采）

- 公开途径不存在，数据在付费 SaaS（Kalodata 等）手里；
- 后备方案：采集器加"**手动导入**"适配器（CSV/JSON 直接进快照目录，`run_id` 由导入时间生成）——付费数据源也进同一条研究链路。写进阶段 3。

---

## 7. 分阶段实施

### 阶段 0：契约 spike（半天~一天）

- [ ] 手写一份快照 JSON 喂给 elsewhen 的 diff 逻辑（或先用测试），验证主提案 §3 契约无返工；
- [ ] CWS 详情页与 Gumroad discover 页 `curl` 实测（可达性 + 字段在不在），决定阶段 1 源范围。

### 阶段 1：采集器核心 + 两个源

- [ ] 单二进制骨架：调度、快照落盘、HTTP API、token、退避；
- [ ] 适配器框架（Source trait + Ctx 注入）+ 输出 schema 校验；
- [ ] 适配器：`github_search`、`cws_listing`（JSON-LD 解析 + 下架 200 防护）；
- [ ] Elsewhen 侧：拉取接线（worker 到点拉 + 启动补跑 + run_id 幂等）；app watchers 表瘦身（§5）；
- [ ] 验收：真实跑一周，app 打开 ≤ 拉取间隔内看到新 findings。

### 阶段 2：更多源与通道

- [ ] `rss_feed`（PH feed）、`http_json`（PH GraphQL POST）、`manual_import`；
- [ ] `gumroad_discover` 适配器（视可达性 spike 结果）；
- [ ] 可选：ntfy/webhook 推送通道（准实时提醒，仍可选装）。

### 阶段 3（后置）

- [ ] Playwright 侧车（Cloudflare 硬源）；
- [ ] 手动导入适配器（付费 SaaS 数据）；
- [ ] 推送通道细化、多采集器（均按需）。

### 演进触发条件

- 阶段 0 → 1：契约 spike 通过 **且** 至少一个非 GitHub 源实测可达；
- 阶段 1 → 2：阶段 1 连续两周有真实查看行为（对齐主提案纪律）；
- 任一验收失败：登记缺陷，不跳阶段。

---

## 8. 明确不做

- 不做 SaaS / 多租户 / 账号体系；
- 不做 diff / findings / 提醒（app 职责）；
- 不做内置 TLS（交给反代 / Tailscale）；
- 不做浏览器渲染（阶段 3 侧车另行评估）；
- 不碰 TikTok 公开抓取（§6.5）。

## 9. 对主提案的修订点（采纳本设计后）

1. **D1 取消**：主提案阶段 2 的脚本引擎（mlua / 沙箱 / 七个宿主 API / 静态扫描授权流 / 对抗性测试）整体不再实施；阶段 2 改为"采集器新增适配器"（本提案 §7）；
2. **D7 改写**：app 内不再有 `http.fetch`；出网约束移至采集器适配器（域名白名单 + 预算，本提案 §4.2）；
3. **D6 简化**：授权流改为"toml 配置即授权"（本提案 §4），静态扫描审批流取消；
4. **D9 语义调整**："无手动触发"指 app 内不提供；采集器 CLI `run` 属运维调试，不算违背；
5. **§3 watchers 表瘦身**：`script`/`interval_minutes`/`declared_tools_json`/`allowed_domains_json` 等字段移除，app 只留名称/启停/alert_policy；新增"采集器连接配置"（URL + token）；
6. **§6 执行流改写**：worker tick 从"跑采集"变为"拉快照"；"app 关闭不采集"代价声明删除；
7. **阶段 1 清单**：删除"GitHub MCP 内建模板"，GitHub 源与 CWS 源同走采集器。

## 10. 关联

- 姊妹提案：`2026-11-15-watcher-data-collection-and-analysis.md`（diff/四表/UI/提醒不变，按 §9 修订）
- 查证来源（2026-11-15）：CWS 详情页 SSR JSON-LD 与 robots.txt（Apify Actor 文档、extenify 指南）、PH GraphQL v2 文档与 Atom feed、GitHub REST search API
- 复用约定：命名密钥 `{{secret:...}}`、退避/冷却纪律（主提案 §4.3）、run_id 幂等
