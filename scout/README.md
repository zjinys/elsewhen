# scout

独立情报采集服务：定时抓取配置的源，与上次快照做确定性 diff（new / changed / gone），快照与发现存进 SQLite，通过只读 HTTP API 供 Elsewhen（或其他消费端）拉取。与 Elsewhen 彻底解耦——app 关了它照样跑。

技术栈：FastAPI + uvicorn + httpx + APScheduler + SQLite。项目用 [uv](https://docs.astral.sh/uv/) 管理。

## 快速开始

```bash
cd scout
uv sync --extra test           # 创建 .venv 并安装依赖
uv run pytest                  # 31 个测试
uv run python -m scout.server  # 启动（默认 0.0.0.0:8787）
```

环境变量：

| 变量 | 默认 | 说明 |
|---|---|---|
| `SCOUT_DB` | `./scout.db` | SQLite 路径（容器内指向 `/data` 卷） |
| `SCOUT_SOURCES` | `./sources.toml` | 源配置 |
| `SCOUT_HOST` / `SCOUT_PORT` | `0.0.0.0` / `8787` | 监听地址 |
| `SCOUT_TOKEN` | 未设置 | 设置后除 `/health` 外全部要求 `Authorization: Bearer <token>` |
| 源各自的 `token_env` | — | 每条源引用自己的凭据环境变量（如 `SCOUT_GITHUB_TOKEN`），不落盘 |

## 源配置（sources.toml）

v1 无管理 UI：手写 TOML，服务每分钟 tick 检测文件 mtime 变更自动重载，无需重启。字段：

| 字段 | 默认 | 说明 |
|---|---|---|
| `name` | 必填 | 人类可读名；`id` 缺省取 name（须匹配 `[A-Za-z0-9_-]+`） |
| `executor` | `httpx` | 采集执行器；`curl_cffi`/`playwright`/`scrapy`/`crawl4ai` 为预留位 |
| `interval_minutes` | `720` | 采集间隔，最小 15（防误配打爆目标） |
| `enabled` | `true` | 停用源：从配置删掉（历史保留、API 隐藏）或 `enabled = false` |
| `url` / `params` / `headers` | — | 请求定义；`headers` 可覆盖默认 UA |
| `token_env` | — | 环境变量名，非空时注入 `Authorization: Bearer <value>` |
| `parse` | `format="json"` | 见下 |

**JSON 源** `parse`：

```toml
[source.parse]
format = "json"
items_path = "items"              # dot-path 定位条目数组（支持 a.b.0.c）
item_id = "full_name"             # 稳定 ID 字段（dot-path），缺失的条目丢弃
fields = ["full_name", "stargazers_count", "html_url", "description"]
                                  # 可选；缺省保留原始条目全部字段
```

**HTML 源** `parse`（需 beautifulsoup4 + lxml，已含在依赖里）：

```toml
[source.parse]
format = "html"
row_selector = "div.repo"                 # 每行条目的 CSS 选择器
item_id = "name"                          # 用哪个输出字段当稳定 ID
[source.parse.fields]                     # 输出字段 <- 选择器规则
name = { selector = "a.name", attr = "href" }
stars = { selector = "span.stars" }       # 无 attr 时取文本（空白折叠）
```

**diff 语义**：对每条归一化后的 item（含全部字段）做内容哈希——任何字段变了都算 `changed`；首次出现 `new`；上次有这次没有 `gone`。空结果（如目标站改版抓空）不产生 `gone` 洪水，但会在 run 里记 item_count=0 供人发现。

## HTTP API（全只读）

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/health` | 免鉴权 liveness |
| GET | `/sources` | 源列表 + 上次运行时间 |
| GET | `/findings?source=&kind=&since=&limit=` | 发现流，时间倒序 |
| GET | `/findings/{id}` | 单条详情 + 该 item_key 的**历史轨迹**（每次快照的数值表） |
| GET | `/snapshots/{id}` | 快照原文回溯 |
| GET | `/runs?source=&limit=` | 运行日志（status / items / findings / error） |

## 数据模型（SQLite 四表）

- `scout_sources`：源注册表（spec hash 同步入库，配置删除后行保留、`enabled=0`）
- `scout_snapshots`：**append-only** 原始快照；保留策略 90 天全量 + 更早每日抽样 1 份（`Store.prune_snapshots`）
- `scout_findings`：diff 产出，外键到检测它的快照
- `scout_runs`：每次运行一条（成功/失败、条数、错误摘要）；失败不落快照

每次运行「快照 + 发现 + run」单事务提交，中途失败整批回滚；单源失败不阻塞其他源。

## 部署（对齐 relay 纪律：独立镜像、独立卷、md5/health 验证）

```bash
cd scout/docker
docker build -f docker/Dockerfile -t scout ..
docker run -d --name scout \
  -v /srv/scout/data:/data \
  -e SCOUT_TOKEN=$(openssl rand -hex 24) \
  -e SCOUT_GITHUB_TOKEN=<gh-token> \
  -p 8787:8787 \
  scout
curl http://127.0.0.1:8787/health
```

## 设计文档

`docs/notes/proposed/product/2026-11-15-watcher-data-collection-and-analysis.md`（架构决策 D1–D10、执行器插拔表、四阶段路线与触发条件）。
