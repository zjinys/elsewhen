# relay 部署 runbook

`config.env` 之外的一切细节。想快速做一次部署只看 `SKILL.md`；这里放「为什么这么写」和出事时查什么。

## 1. 拓扑事实（2026-10-25 实测，不是推测）

| 事实 | 值 | 影响 |
|---|---|---|
| 服务器 | `192.168.101.116`（主机名 `Tower`） | 同一台机器还跑着 Gitea（:30000） |
| ssh 用户 | **只能 `root`** | `pp`/`tower`/`admin`/`zjinys` 全部 `Permission denied (publickey)` |
| 服务器上的 node | **没有** | 不能在服务器上构建，只能传镜像 |
| 服务器上的仓库副本 | **没有** | `relay/standalone/*.mjs` 只存在于开发机 |
| docker | 27.5.1 | 支持 `docker save`/`load` |
| 数据目录 | `/mnt/cache/appdata/elsewhen-relay/data` | 信箱 SQLite（`mailboxes/`） |
| 端口 | `8787` | 客户端中继 URL 用它 |

## 2. 部署链路与每一步的理由

```
本机构建（必须 --network=host）
   ↓ 本机试启动 + 契约冒烟        ← 坏镜像不推到线上
   ↓ docker save | gzip -1 | ssh 'gunzip | docker load'
   ↓ docker rm -f + docker run（同一 data 卷）
   ↓ 模块 md5 对比 + /health + 契约冒烟
```

- **必须 `--network=host` 构建**：本机 docker 的 default bridge 建 veth 对会失败，
  报 `failed to create endpoint … failed to add the host (veth…) <=> sandbox (veth…) pair interfaces: operation not supported`。
  这不是镜像问题、不是网络问题，换 `--network=host` 立刻好。构建期不需要网络隔离。
- **先 `docker load` 再 `docker rm -f`**：镜像没到位时线上仍在服务，停机窗口只有删容器到新容器起来的那一两秒。
- **`chown -R 1500:1500` 数据目录**：镜像内固定 `USER relay`（UID/GID 1500），卷的属主不对就会
  `SQLITE_CANTOPEN`。幂等，每次跑一遍无害。
- **绝不带 `-v …/releases:/releases`、绝不传 `RELEASES_DIR`**：R0 已删除应用内分发功能。
  老容器还留着这两个是历史遗留，重建时若照抄老 `docker run` 会把死配置带回来。

## 3. 本次（2026-10-25）部署前的线上状态

部署前线上是 **R2 之前的镜像**，典型症状恰好说明了「为什么必须比模块 md5」：

```
容器内：只存在 /app/server.mjs，md5=6283b476…（≠ 仓库 18a4004e…）
       /app/schema.mjs /app/sweeper.mjs /app/blob.mjs  →  No such file or directory
但 /health 返回 {"ok":true,"service":"elsewhen-relay","version":2}  ← 绿的
```

也就是说：**`/health` 绿完全不能证明部署成功**。旧镜像没有 blob 数据面、没有 sweeper，
`/status.capabilities` 也不会宣告 `blob`/`drop`，而 healthcheck 一无所知。

## 4. 验证三层，缺一层都会漏

1. **`/health` + `version==2`** — 进程活着、协议号没抬（能力靠 `/status.capabilities` 协商）。
2. **模块 md5 与仓库逐一相等** — 上面那个事故就是被这一层抓住的。
3. **契约冒烟 `smoke.mjs`** — 真发一封、真取回、真 drop：
   - `/fetch` 是 **GET + query 签名**（`GET_ROUTES = fetch/status/sender`），
     不是裸 GET —— `version`/`ts`/`nonce`/`sig` 全在 query 里。
   - 返回值字段是 **`envelopes`**（不是 `mails`）。
   - `/drop` 用 ack 域签名必须 **403**：这是 R5「已消费」与「已丢弃」分家的结构性防线，
     有人把两个动词合并成一个删除端点时，这条会立刻红。
   - `/status` 的 `capabilities` 必须同时含 `blob` 与 `drop`。

冒烟会在服务器上留下两个几 KB 的空信箱文件（`data/mailboxes/<64位hex>`）。
它们不含任何业务数据，行会在 TTL 后由 sweeper 回收；要清掉就删这两个文件名——
**但只删冒烟自己刚打印出来的那两个名字**。

## 5. 回滚

```bash
./skills/relay-deploy/scripts/deploy.sh --rollback
```

取远端同仓库镜像里、当前容器之外最新的那个重建。数据卷不动 —— 回滚代码版本不需要回滚数据
（migration 只在客户端 SQLite 里，relay 侧 schema 由 `schema.mjs` 幂等建表）。

远端保留 `RELAY_KEEP_IMAGES`（默认 5）个镜像；调成 1 就失去回滚能力。

手工回滚（脚本坏了时）：

```bash
ssh root@192.168.101.116 '
  docker images --format "{{.Repository}}:{{.Tag}} {{.CreatedAt}}" elsewhen-relay | sort -k2 -r
  docker rm -f elsewhen-relay
  docker run -d --name elsewhen-relay --restart unless-stopped -p 8787:8787 \
    -e PORT=8787 -e DATA_DIR=/data -e MAILBOX_QUOTA_BYTES=536870912 \
    -v /mnt/cache/appdata/elsewhen-relay/data:/data elsewhen-relay:<旧tag>
'
```

## 6. 配额

| 变量 | 代码默认 | 线上 | 说明 |
|---|---|---|---|
| `MAILBOX_QUOTA_BYTES` | 50 MiB（硬上限 512 MiB） | **512 MiB** | 手机侧一次 enrollment 快照可上百 MB，按默认值会反复撞 507 |
| `BLOB_QUOTA_BYTES` | 64 MiB | 未设（= 64 MiB） | 与信件账互不占用；一窗 10 片 ≈ 10 MiB，64 MiB 够 |

配额是 **env，不是落库配置**，改了必须重建容器。超出配额时客户端拿到 507 并退避 15 s，
不会丢信。

## 7. 绝不删的东西

| 路径 | 内容 | 说明 |
|---|---|---|
| `/mnt/cache/appdata/elsewhen-relay/data` | 信箱 SQLite | 删了等于把这个数据集的设备群全部解绑 |
| `/mnt/cache/appdata/elsewhen-relay/mailboxes.wiped-20261024-210715` | 1.7 G 事故归档 | **删前必须问用户** |
| `~/.local/share/elsewhen/wipe-20261024-210619/` | 263 MB 本地归档 | 同上 |

## 8. 故障速查

| 症状 | 先查 |
|---|---|
| `/health` 绿但客户端报协议不支持 | 模块 md5 —— 很可能部署了旧镜像；再看 `/status.capabilities` |
| `docker run` 后立刻退出 | `docker logs --tail 50 elsewhen-relay`；常见是 `ERR_MODULE_NOT_FOUND`（Dockerfile 漏 COPY 模块）或数据卷不可写 |
| `SQLITE_CANTOPEN` / `attempt to write a readonly database` | 数据卷属主不是 1500:1500 |
| `failed to create endpoint … veth pair` | 构建时漏了 `--network=host` |
| 客户端 507 | 配额账（信件 vs blob 是两本）；`/status` 看 `bytes`/`quota` |
| 镜像传一半断了 | 旧容器未动，直接重跑 `deploy.sh` |

## 9. 与仓库测试的分工

`skills/relay-deploy` 只回答「线上的东西是不是这份代码、能不能跑通主链路」。

协议与数据面的**正确性**仍由仓库测试负责，部署脚本不重复实现：

```bash
cd relay && npm test    # 41 个用例：协议契约、blob 六端点、容量不变式、加入流程
```

线上**不跑** blob 数据面冒烟：那会往真实信箱写分片，收益不抵风险。
