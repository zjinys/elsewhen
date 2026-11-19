# Scout：独立数据采集服务 + Elsewhen 情报消费端

- 日期：2026-11-15（同日两次定稿：v1 内嵌 watcher → v2 独立服务，演变记录见 §10）
- 状态：`implementing`（采集与阅读基线已有；真实连续运行待验收；2026-11-17 商机研究扩展已规划，O0–O7 未实施）
- 范围：独立定时采集外部数据（GitHub trend / Chrome Web Store / Gumroad / Product Hunt 等）→ 快照留存 → 差异检出 → findings 存储 + HTTP API；Elsewhen 作为消费端拉取、展示、认知加工。不涉及给第三方的交付报表（后置，见 §7）
- 性质：工程内独立服务（`scout/`，Python）+ Elsewhen 客户端集成（Rust + Flutter）
- 性质补充：本服务同时是用户出海情报生意的内部生产管线原型（个人研究与交付数据在 schema 上分离，见 §7）

2026-11-17 产品方向扩展：用户明确情报用于发现商机与决策。新增[商机研究提案](../feature/2026-11-17-scout-opportunity-research.md)与[实施路线 O0–O7](../../../roadmap/2026-11-17-scout-opportunity-research-roadmap.md)，接管后续产品归并、趋势、需求证据与研究闭环的实施顺序。本文保留采集架构及 E1/E2 后置边界；原分期不再作为商机研究的推进顺序。

---

## 1. 背景与需求来源

### 1.1 对话溯源（2026-11-14，主对话 93196d4f）

用户与 AI 讨论「出海数据监控与竞品情报」方向时，AI 给出四个具体监控切口：

1. **Chrome Web Store 插件异动**：安装量每周环比增长 >30% 的轻量插件
2. **Gumroad / Lemon Squeezy 热销追踪**：数字产品近期突然起量的小品类
3. **Product Hunt / TikTok Shop 榜单**：近 7 天单量异动暴涨、竞争店铺少的小爆品
4. （GitHub trend 是用户额外关心的开发侧信号）

AI 给的交付形态明确是「**用脚本自动化抓取，做成即拿即用的表格或周报**」「每周只筛 20~30 个」。

### 1.2 用户明确的四条需求（2026-11-15 拍板）

1. 通过脚本自动采集这些数据（灵活可扩展）；
2. 采集的数据**保存下来研究**（不是看完即弃的报告）；
3. 数据在一段时间内大部分相同（如 GitHub trend 长期是同一批 repo），**突然出现的新信息最值得关注**；
4. 出现新信息后尽快提醒——**一天之内可接受，不要求实时**。

### 1.3 为什么是独立服务而不是 Elsewhen 内嵌（v2 关键转向）

- 数据采集是用户已选定业务方向（出海情报）的**核心生产环节**，将来要演化成对外产品——它应该是独立的服务，而非埋在个人笔记应用里的功能；
- 独立服务 7×24 常驻，**彻底解决「app 关了就停采集」**的时效问题，比内嵌方案（依赖 Flutter 后台 worker）更契合需求 4；
- 职责纯粹化：**服务侧**管采集/存储/调度（plumbing），**Elsewhen 侧**管消费与认知（把外部信号变成判断和记录）——笔记应用不引入采集调度这种与「记录→分析→沉淀」无关的常驻职责；
- 用户拍板（2026-11-15）：不与 relay 混、独立服务、Elsewhen 通过 HTTP 拉取（方案 b）、语言按生态选型（定为 Python）。

---

## 2. 架构决策（已拍板）

| # | 决策 | 理由 |
|---|---|---|
| D1 | **独立服务 `scout/`，与 relay 平级、零耦合**（独立数据卷/配置/部署，不碰 relay 的 config.env） | 用户明确要求；relay 是同步基础设施，scout 是业务服务，故障域隔离 |
| D2 | **语言选 Python**（FastAPI + httpx + APScheduler + sqlite3 stdlib），**采集执行器可插拔**，成熟爬虫框架按源接入而非当地基 | 采集生态 2026 年横向对比 Python 显著更优：Scrapy（63k⭐）、lxml/parsel、**curl_cffi**（TLS 指纹伪装反爬，Gumroad/PH 类站点刚需）、**Crawl4AI**（LLM 友好抽取，Apache 2.0 可自托管）、**pandas**（「保存下来研究」的直接受益，Node 无等价体验）；本机 Python 3.14 + requests/httpx/aiohttp 已装。否决 Node（relay 同栈好处缩水：容器化部署模式对任意语言通用）。**执行器分层**：Scrapy 的强项（队列/middleware/去重/限速）在千页级规模才值回票价，而 scout 是少数源 × 每日一次 × 几十条，且 scout 独有价值（快照/diff/调度/API）Scrapy 均不提供——故 v1 内建 httpx 轻执行器，复杂需求按源接入专用执行器（见 §3.6）；Crawl4AI Cloud 的 `/monitors` 变更监控与本服务语义重合但按 credit 计费，佐证自建价值 |
| D3 | **差异检测由确定性代码做，不用 AI** | 按稳定 ID 的集合 diff（new/changed/gone），快、免费、零幻觉。AI 只做可选摘要增强，不在主链路 |
| D4 | **快照与 findings 都是持久研究数据**，append-only | 与 events 不可变哲学同构；每条目有完整时间线（首次出现、数值轨迹），是研究素材本体 |
| D5 | **调度每日 1~2 次（可配）+ 常驻进程**；服务端重启不丢数据 | 用户接受一天内时效；7×24 服务使「app 打开时补跑」不再必要，时效上限 = 服务中断时长 |
| D6 | **v1 不做脚本引擎/沙箱**：源定义用 TOML spec（声明式：URL/解析规则/稳定 ID 字段） | 服务由用户自己运维，采集源是配置不是不可信代码；将来 SaaS 化接用户自定义源时再评估脚本引擎（Lua 沙箱方案保留为远期可选，见 §10） |
| D7 | **Elsewhen 侧只读消费**：监控源管理 UI 首期不做，spec 在服务端手写 | 自己的服务无需管理 UI；Elsewhen 的「监控源」页只读展示源状态 |
| D8 | **个人研究数据与可交付数据 schema 分离** | 阶段 3 若做客户交付报表，不返工；见 §7 |
| D9 | **服务→Elsewhen 传输用共享 token 的 HTTP API**（方案 b），不写 relay、不走同步通道 | 简单直接；复用 RelayClient 的网络纪律（严格 URL/响应上限、禁重定向） |
| D10 | **提醒走应用内通道**（今天栏卡片 + 侧栏未读徽标），桌面系统通知后置 | 一天内可接受 → 不需要打断式通知；服务端有新 findings 时 Elsewhen 定期拉取即可覆盖 |

---

## 3. Scout 服务设计（`scout/`，Python）

### 3.1 目录结构

```
scout/
├── pyproject.toml           # fastapi uvicorn httpx apscheduler pydantic（5 个直接依赖）
├── scout/
│   ├── server.py            # FastAPI：GET /sources /findings /runs /health，token 校验
│   ├── scheduler.py         # APScheduler：按 source interval 触发，错过补跑
│   ├── sources.py           # TOML spec → httpx 抓取 → 解析归一化（输出稳定 ID 的 item 列表）
│   ├── diff.py              # 与上次快照比对：new / changed / gone
│   ├── store.py             # sqlite3 stdlib，四表，append-only 快照
│   └── schema.sql
├── sources.toml             # 源定义（手写，重启生效；v1 无管理 UI）
├── test/                    # pytest：diff 全覆盖、解析器 fixture、API 鉴权
└── docker/Dockerfile        # python:slim，对齐 relay 部署纪律（独立卷、md5 验证）
```

### 3.2 数据模型（服务端 SQLite，四表）

```
scout_sources                -- 源定义（sources.toml 启动时同步进库，含 spec hash）
  id, name, enabled, spec_hash, interval_minutes, last_run_at

scout_snapshots              -- 每次采集原始数据，append-only
  id, source_id, taken_at, data_json, item_count
  -- 保留策略：全量保留 90 天，更早每日抽样 1 份

scout_findings               -- 检出的新信息
  id, source_id, snapshot_id,
  kind,                      -- new | changed | gone
  item_key,                  -- 稳定 ID（如 repo full_name）
  payload_json,              -- 条目完整数据（研究素材）
  detected_at

scout_runs                   -- 运行记录
  id, source_id, status, started_at, finished_at,
  items_count, findings_count, error
```

### 3.3 源 spec（sources.toml 示例）

```toml
[[source]]
name = "github-dart"
interval_minutes = 720            # 每日 2 次
url = "https://api.github.com/search/repositories"
params = { q = "language:dart stars:>100 sort:stars", per_page = 30 }
token_env = "SCOUT_GITHUB_TOKEN"  # 环境变量引用，不落配置文件
[source.parse]
format = "json"
items_path = "items"
item_id = "full_name"             # 稳定 ID 字段
fields = ["full_name", "html_url", "stargazers_count", "description"]
```

v1 的解析器支持 `json`（dot-path 取值）与 `html`（CSS 选择器 + 属性提取）两种 format；复杂站点（CWS）用 Playwright 渲染作为 v2 增量。

### 3.4 运行时纪律

1. 每次 run：抓取 → 归一化 → 与上次快照 diff → 快照 + findings + run 确认一个事务；
2. 失败指数退避，连续失败只记 run error，不阻断其他源；
3. HTTP 上限：响应体 ≤5MB、超时 30s、UA 固定、遵守 robots 语义（采集源均为公开榜单/搜索 API）；
4. token 类凭据只放环境变量（`token_env` 引用），配置文件与库内不落密钥；
5. API 鉴权：`SCOUT_TOKEN` 共享密钥（Bearer），内网/公网均可部署，全端点 401 保护。

### 3.5 采集执行器（可插拔，成熟框架按源接入）

sources.toml 中每个源声明执行器，统一输出「稳定 ID 的 item 列表」接口；快照/diff/调度/API 地基不随执行器变化：

| executor | 引入阶段 | 适用场景 |
|---|---|---|
| `httpx`（内建） | 阶段 0 | JSON API 与静态 HTML 榜单（多数源） |
| `curl_cffi` | 阶段 2 按需 | TLS 指纹伪装的反爬站点 |
| `playwright` | 阶段 2 按需 | JS 渲染/需登录的源（如 CWS） |
| `scrapy` | 阶段 2+ 按需 | 单源规模到千页级（如全品类店铺扫描），该源直接写成 spider，scout 调用 |
| `crawl4ai` | 阶段 3 按需 | LLM 友好的页面抽取 |

原则：不为假想规模预付框架成本；每个执行器是独立模块（`executors/httpx.py` 等），注册到 spec 解析，无执行器间耦合。

### 3.6 API（v1 全只读）

```
GET /health                     # 无需鉴权，liveness
GET /sources                    # 源列表 + 上次/下次运行 + 状态
GET /findings?since=&source=&kind=&unread=true
GET /findings/{id}              # 单条详情（含历史轨迹）
GET /snapshots/{id}             # 快照原文回溯
```

---

## 4. Elsewhen 消费端设计

### 4.1 Rust 侧：`ScoutClient`（`src/intel/scout_client.rs` 或 `src/scout/`）

- 配置：服务 base URL + token（存命名密钥，`{{secret:...}}` 复用现有机制）；
- 网络纪律照 RelayClient：严格 URL/响应上限、禁用重定向、超时上限；
- 拉取策略：Flutter 后台 worker tick 里降频轮询（分钟级，照 compass 模式），把新 findings 落进本地 `intel_findings` 镜像表（保留服务端 detected_at，本地补 `read_at` 记录已读态）；
- **离线容忍**：服务不可达只是拉不到新数据，本地镜像和 UI 照常工作。

### 4.2 本地表（迁移顺延，两张）

```
intel_findings_mirror     # 服务 findings 的本地镜像 + read_at（已读态归 Elsewhen 管）
intel_settings            # scout base url / token 引用 / 拉取间隔 / 启用
```

2026-11-17 阅读界面以[情报阅读工作区](../../implemented/feature/2026-11-17-scout-reading-workspace.md)为准：直接进入跨源动态、左栏筛来源、站内详情；已接入记为事件和存为素材。下述三页为初始方案记录。

### 4.3 UI：左侧栏一级入口「情报」（图标 `Icons.travel_explore_outlined`）

放在「复盘」与「同步」之间；带未读 findings 数徽标（复用现有红点通道）。内部结构：**固定三个子 tab 承载动态内容**（`IndexedStack` 二级页 + `NavigationBar`），源数量增长只影响列表长度，不动导航骨架：

```
情报
├── Dashboard（默认落点，只读）
├── 发现（findings 流）
└── 数据源（只读：源状态/频率/下次运行/运行历史）
```

1. **Dashboard**：顶部未读摘要卡（「今天：GitHub Dart 监控 3 个新仓库，Gumroad 2 个异动」）→ 按 source 分组的卡片（最近 24h findings，new/changed 用主题色/次级色区分）→ 每源 30 天条目数迷你 sparkline（CustomPaint）；卡片点击深链「发现」页带源筛选；管理动作不在此页。
2. **发现页**：全部 findings 时间倒序流，筛选器（源 / kind / 已读未读）；**单条详情 = 研究价值最重的页面**：条目完整数据、首次发现时间、历史轨迹（该 item_key 每次快照的数值小表）、快照原文回溯入口。
3. **数据源页**：只读列表（D7）——名称、频率、最近状态、未读数；源的新增/编辑/停用 = 改服务端 sources.toml，Elsewhen 不做编辑 UI。
4. **今天栏提醒**：新 findings 落镜像后下一 tick 出未读卡片；一次拉取多条合并成一张卡（「GitHub Dart 监控：3 个新仓库，2 个异动」）；`daily_digest` 节奏每日至多一张卡；夜间静默（照习惯问候频控纪律）。

### 4.4 认知加工（这是 Elsewhen 侧存在的理由）

finding 详情页提供三个动作，接现有链路：

- **记为事件**：把 finding 摘要写进事件流（走 `Store::insert_event()` 单一提交边界，此后正常进 analysis/digest 队列）；
- **存为素材**：作为导入材料进素材库（挂来源 URL 与快照引用）；
- **转知识页草稿**：生成知识页草稿（照 wiki 草稿确认制，经审阅才入库）。

原始 payload 不进知识链路——进链路的永远是用户确认的判断和记录，与「raw events immutable / derived data 经审阅」的架构原则一致。

---

## 5. 执行流总览

```
[sources.toml] → scout 调度（7×24）→ httpx 抓取 → 归一化 → diff
    → 快照 + findings 落库（事务）→ HTTP API
                                      ↓
Elsewhen worker（分钟级轮询）→ 拉取新 findings → 本地镜像 → 今天栏卡片 + 侧栏徽标
    → 用户点开 → 历史轨迹/快照回溯 → 记为事件 / 存为素材 / 转知识页草稿
```

服务端重启不丢数据（SQLite 卷）；Elsewhen 离线容忍（D9/D10）；时效上限 = min(源间隔, Elsewhen 轮询间隔)。

---

## 6. 部署（对齐 relay 纪律）

- 独立 Docker 镜像（python:slim），独立数据卷，独立端口，独立 `SCOUT_TOKEN`；
- 不与 relay 共享 config.env / 容器 / 数据卷（D1）；
- 部署脚本与验证（health / md5 / 冒烟）参照 `skills/relay-deploy` 模式另立 scout 版，不改动 relay 脚本。

---

## 7. 研究/交付数据分离（D8）

- `scout_snapshots` / `scout_findings` = **个人研究数据**：原始、完整、只属于自己；
- 将来交付给客户的周报/表格 = **可交付数据**：是 findings 的挑选、清洗与再组织产物（pandas 管线），单独建模，不与 findings 混表（阶段 3 另行设计）；
- 阶段 1/2 只落研究数据，交付层留空。

---

## 8. 分阶段实施

### 阶段 0：scout 服务 MVP（先行，无 Elsewhen 改动）

- [ ] `scout/` 骨架 + 五依赖 + Dockerfile
- [ ] 四表 schema + append-only 快照 + 保留策略
- [ ] TOML spec 加载 + `httpx` 执行器（json/html 两 format 解析）+ fixture 测试
- [ ] 执行器注册接口定型（executor 字段，预留 curl_cffi/playwright/scrapy 扩展位）
- [ ] diff 全覆盖测试（new/changed/gone、空结果不产 findings）
- [ ] GitHub source 真实跑通（token 走环境变量）
- [ ] FastAPI 只读 API + token 鉴权 + /health
- [ ] 验收：部署到服务器跑一周，`curl /findings` 能看到 trend 变化

### 阶段 1：Elsewhen 消费端最小闭环

- [x] `ScoutClient`（网络纪律照 RelayClient）+ 两张本地表迁移（迁移 85 `scout_findings_mirror`/`scout_settings`；payload 定形 `payload_json` 不暴露 `serde_json::Value`）
- [x] worker 轮询接线（降频 + 离线容忍）：`worker_scout.dart` 挂 compass 同款 12 拍节奏，tick 只读设置行、到期才 `pollScoutNow`，失败仅 debugPrint
- [x] 侧栏「情报」入口 + Dashboard/发现/数据源三页（`scout_view.dart` + parts）+ 设置页 Scout 连接区块（地址/密钥/间隔/启停/测试/清空镜像）
- [x] 今天栏未读卡片 + 频控 + 夜间静默（`storage/scout_card.rs`：每日至多一张合并卡，22:00–07:00 静默，poll 成功后同步 tick；`scout_brief` 气泡点击跳情报页）
- [ ] 验收：真实运行一周，服务端检出新 repo → 打开 app 见红点 → findings 时间线可回溯

### 阶段 2：采集扩展（业务管线，执行器按源接入）

- [ ] CWS / Gumroad / Product Hunt 源 spec
- [ ] `playwright` 执行器（CWS 渲染）与 `curl_cffi` 执行器（反爬站点）按需实现
- [ ] 若出现千页级源：该源写成 `scrapy` spider 接入执行器位
- [ ] pandas 报表管道原型（给自己用的研究视图，非交付）
- [ ] 验收：≥3 个真实源稳定运行两周

### 阶段 3（后置，业务成立才做）

- [ ] 可交付报表层（客户周报，研究/交付分离落地）
- [ ] findings 认知加工深化（与 insight/知识页自动关联建议）
- [ ] 源管理 UI（Elsewhen 侧编辑 spec 上传）或轻量 web 管理台
- [ ] 远期可选：脚本引擎（Lua 沙箱）接用户自定义源（SaaS 化前提）

### 演进触发条件

- 阶段 0 → 1：服务连续一周稳定出 findings；
- 阶段 1 → 2：阶段 1 连续两周有真实查看行为；
- 阶段 2 → 3：确有对外交付动作；
- 任一阶段验收失败：登记具体缺陷，不跳阶段。

---

## 9. 明确不做

- 不做通用插件系统/脚本市场；
- 不做实时推送、常驻移动端后台、系统通知（后置增量另议）；
- 不做 Elsewhen 侧源编辑 UI（D7，v1 spec 在服务端手写）；
- 不与 relay 共享任何运行时资源（D1）；
- 不做 SaaS 多租户（那是产品化阶段的事）；
- 不碰 knowledge digest 现有机制（findings 进事件/素材链路走既有提交边界）。

---

## 9.1 已登记的未来扩展（未排期）

以下需求已确认存在、当前**明确不做**，登记在此以免重复讨论。
每条都写明为什么现在不做、以及动手时的落点。

### E1. 多 token / 多用户访问（服务端鉴权）

**现状**：`scout-cf/src/index.ts` 的 `checkAuth` 只认单个 `env.SCOUT_TOKEN`，
精确相等比较。因此一个部署只有一个有效 token，所有设备/人共用；
撤销某人只能换 token，全体重配。

**缺口（为什么不等于「一个用户」）**：现在的 token 是共享口令，不是身份。

| 用户应有的 | 现状 |
|---|---|
| 身份（谁在用） | 无，请求链上只有同一个 `Bearer xxx` |
| 权限（能干什么） | 无区分：读 `/findings` 与 `PATCH /admin/sources`、`POST /admin/trigger` 是同一个 token |
| 单独撤销 | 做不到 |
| 审计（`last_used_at`） | 无 |

**计划落点**（动手时按此办，避免再讨论一遍）：

1. D1 新表存 token **哈希**（不存明文）：
   ```sql
   CREATE TABLE scout_tokens (
     id TEXT PRIMARY KEY,        -- 'pixel-7' / 'desktop'
     label TEXT,
     token_hash TEXT NOT NULL,   -- SHA-256
     role TEXT NOT NULL DEFAULT 'reader',  -- reader | admin
     created_at TEXT NOT NULL,
     last_used_at TEXT,
     revoked_at TEXT
   );
   ```
2. `checkAuth` 从同步改异步查表，返回 `{ id, role }`；`/admin/*` 只放行 `admin`，
   否则只读 token 也能关源。
3. `SCOUT_TOKEN` 保留为兜底 admin（现有部署零改动升级）。
4. 管理页加「token 管理」栏：生成（明文只显示一次）/ 列出 / 撤销 / 看 `last_used_at`。
5. **Elsewhen 客户端无需改动**——仍是「填一个 token」，只是每台设备填自己那条。

顺带修一个风险：`checkAuth` 现在在 `SCOUT_TOKEN` 未设置时 `return null`
（= 完全放行）。多 token 改造时一并改为 fail closed（503），只有显式开发
开关才放行。

**明确不走的路**：不引入 Cloudflare Access / Zero Trust / OAuth。
SCOUT_TOKEN 是 Worker 自己发的 API 凭证，与 CF 账号体系无关；
把它和 CF 身份混在一起只会增加部署前提（需自定义域名等），而这个 API
的消费者只有 Elsewhen 客户端和一两个人。

### E2. 一个部署接多个 scout 服务器（客户端侧）

**现状**：`scout_settings` 是单行表（`CHECK(id = 1)`），密钥代号固定
`scout_token`，镜像表 `scout_findings_mirror` 的主键直接是**服务端
findings.id**。

**为什么不能只加个字段**：两个服务的 findings.id 都从 1 递增，接第二个会
**主键互相覆盖**；`scout_source_summaries()` 只按 `source_id` 聚合，两台都有
`github-dart` 时也会混成一组。

**真要做的落点**：镜像表加 `server_id`、主键改 `(server_id, id)`；设置表拆多行、
密钥代号改 `scout_token:<server_id>`；Rust 查询链（list/count/summaries/
mark-read/clear）全部加 server 作用域；UI 改服务器列表 + 切换。

**优先级判断**：多监控源应该在**服务端加源**（`sources.ts` 里一个
`jsonSource(...)` 就是十几行，管理页立刻可管），而不是客户端接多台服务。
E2 只有在「跨团队/跨地域各部署一套」时才值得做。

---

## 10. 演变记录

- **v1（2026-11-15 上午）**：watcher 内嵌 Elsewhen（Lua 5.4/mlua 沙箱、四表、移动端 spike、保存时授权）。Lua 移动端可行性已查证（纯 ANSI C 静态嵌入可行，禁 LuaJIT）。
- **v2（2026-11-15，本版）**：转向独立服务。触发：用户明确「数据采集应该单独抽出来作为第三方服务」（业务方向使然：它就是出海情报生意的生产管线原型）；随后拍板四点（不混 relay / HTTP 拉取 / 生态选语言 / 工程内目录）+ 命名 scout + Python 定栈。
- **v2.1（2026-11-15 晚）**：采集出口迁 Cloudflare Workers（`scout-cf/`，TS 重写）。触发：Tower 无法直连外网，被墙的商业源（Gumroad/CWS/PH 网页）需要一个海外出口；Workers 出口实测全通（PH feed 50 条、Gumroad discover 200、CWS/TikTok 200 无盾）。实现换 TS（Workers + Cron Triggers + D1），核心机制四表/确定性 diff/append-only 快照/单事务提交/API 形状全部 1:1 保留；Python 版（Tower 8799）留作过渡。已上线 7 源：GitHub×2、Product Hunt、App Store 榜、Gumroad×2（Inertia data-page 提取）、CWS 竞品清单（安装量/评分轨迹）；阶段 2 的「采集扩展」实际上提前完成。执行器插拔表在 CF 版对应为四种源工厂（jsonSource/feedSource/inertiaSource/cwsSource），按源按需增加。
- v1 的核心机制（四表、确定性 diff、append-only 快照、三页 UI、时效约定、研究/交付分离）全部保留，归属变更为 scout 服务端；Lua 沙箱降级为阶段 3 远期可选（SaaS 化接用户脚本时再评估）。

## 11. 关联

- 需求对话：2026-11-14 主对话 93196d4f（出海数据监控四切口）、2026-11-15 本会话（两轮方案定稿 + 命名/选型拍板）
- 部署先例：`relay/`（docker + 数据卷 + 验证纪律）、`skills/relay-deploy`
- 复用纪律：`RelayClient` 网络上限纪律、命名密钥 `{{secret:...}}`、`Store::insert_event()` 单一提交边界、今天栏/nudge 频控先例（习惯问候、digest insight 每日至多一条）
- Backlog：`docs/proposal-backlog.md`（本提案入「工程待办」）
