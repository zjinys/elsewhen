---
name: relay-deploy
description: 部署/更新/回滚/验证 Elsewhen 盲中继（relay）到自建服务器：本地构建镜像 → 传过去 → 用同一份数据卷重建容器 → 三层验证（health / 模块 md5 / 契约冒烟）。服务器地址、端口、数据目录、配额一律改 config.env，脚本里没有常量。Use when deploying, upgrading, rolling back, or health-checking the elsewhen relay server.
---

# relay 部署

一个数据集（一台桌面 + 它的手机）的加密信箱跑在自建服务器上的 docker 容器里。
这个 skill 负责把它换上当前仓库的代码，并且**证明**换上去的确实是这份代码。

## 先读配置

所有服务器信息在 `config.env`（本 skill 目录下），脚本不含任何常量：

| 键 | 当前值 | 说明 |
|---|---|---|
| `RELAY_HOST` | `192.168.101.116` | Tower（同机还跑 Gitea:30000） |
| `RELAY_SSH_USER` | `root` | **只有 root 能登**，其他用户名一律 publickey 拒绝 |
| `RELAY_CONTAINER` | `elsewhen-relay` | |
| `RELAY_PORT` | `8787` | 客户端中继 URL 用它 |
| `RELAY_HOST_DATA_DIR` | `/mnt/cache/appdata/elsewhen-relay/data` | **信箱数据，必须保留** |
| `RELAY_MAILBOX_QUOTA_BYTES` | `536870912` | 512 MiB；代码默认只有 50 MiB |
| `RELAY_KEEP_IMAGES` | `5` | 回滚靠它 |

密钥类东西写在 `config.local.env`（已在 `.gitignore`，脚本会覆盖加载）。

## 三条命令

```bash
./skills/relay-deploy/scripts/deploy.sh              # 构建 + 部署最新代码
./skills/relay-deploy/scripts/deploy.sh --rollback   # 回滚到上一个镜像
./skills/relay-deploy/scripts/verify.sh               # 只读验证，不动线上
```

加 `--smoke` 在部署后再跑一次契约冒烟（`RELAY_SMOKE=1` 也行）。
`--tag <name>` 指定镜像 tag，`--verify-only` 等价于 verify。

### 部署前会先验线上信箱（`scripts/preflight-mailboxes.mjs`）

`openMailboxDb()` 在**开库时**建表并跑幂等索引迁移，这一步抛错 = relay 启动即死；
而 relay 是「代码一改就带来新迁移」的形态，线上每个老信箱都要能在新 `schema.mjs`
下打开。所以 `deploy.sh` 在构建镜像**之前**就把线上信箱 `tar` 一份副本回来，
用仓库当前的 `schema.mjs` 逐个试开，不开就中止（线上一个字节都不动）。

也可以单独跑（参数是信箱目录，脚本自己再复制一份到临时目录，源目录只读）：

```bash
ssh root@$RELAY_HOST "tar -cf - -C /mnt/cache/appdata/elsewhen-relay/data mailboxes" | tar -xf - -C /tmp/mb
node skills/relay-deploy/scripts/preflight-mailboxes.mjs /tmp/mb/mailboxes
# ✓ {"mailbox":"bf44c43f","bytes":4096,"idemIndex":1,"integrity":"ok","mail":0,...}
```

它打印每个信箱的 `mail/senders/owner/blobs/blobChunks/idemIndex/integrity`；
`idemIndex=1` 表示幂等索引建得起来，`integrity=ok` 表示没有行被迁移删坏。
**只拷主文件会漏数**：`-wal` 里可能还有未 checkpoint 的已提交事务，脚本连
`-wal`/`-shm` 一起复制。

## 四条铁律

1. **构建必须 `--network=host`。** 本机 default bridge 建 veth 对会失败
   （`failed to add the host <=> sandbox pair interfaces: operation not supported`），
   这不是镜像问题也不是网络问题。
2. **服务器上没有 node、没有仓库副本。** 别想着 ssh 上去 `npm start` —— 只能在有仓库的
   机器上构建，`docker save | gzip -1 | ssh 'gunzip | docker load'` 过去。
3. **重建容器时不许带回 `/releases` 挂载与 `RELEASES_DIR`。** R0 已删除应用内分发功能，
   老容器的这两个是历史遗留。数据卷 `/data` 必须照原样挂回去。
4. **relay 侧代码改了就必须重新部署，`md5` 才作数。** 改完只跑 `node --test` 等于没上线
   —— 2026-10-25 就是这样漏的：`relay/standalone/{server,schema,blob}.mjs` 改到 18:43，
   镜像还是 15:45 那个，`/health` 一直绿、`version:2` 一直对，线上跑的却是 P4 之前的代码。
   判断「线上是不是这份代码」的**唯一**依据是 `verify.sh` 的模块 md5 逐项相等，
   不是 `/health`、不是容器 `healthy`、更不是「我记得部署过」。

## `/health` 绿不等于部署成功

这一条必须记住，因为踩过：

```
容器内只有 /app/server.mjs，md5 与仓库不同，
/app/schema.mjs /app/sweeper.mjs /app/blob.mjs 根本不存在
而 /health 照样返回 {"ok":true,…,"version":2}
```

旧镜像没有 blob 数据面、没有 sweeper，healthcheck 一无所知。所以验证必须三层：

1. `/health` 返回 `version:2`；
2. **`/app/{server,schema,sweeper,blob}.mjs` 的 md5 与仓库逐一相等**；
3. 契约冒烟 `scripts/smoke.mjs`：真发一封、真 `fetch`、真 `drop`，
   并断言「拿 `ack` 的签名去 `/drop` 必须 403」与「`/status.capabilities` 含 `blob`+`drop`」。

第 3 层里有几个容易写错的协议细节（写脚本时全靠它才跑通）：

- `POST_ROUTES = create/register/revoke/rotate/send/ack/drop`；
  `GET_ROUTES = fetch/status/sender` —— **GET 也要带 `version`/`ts`/`nonce`/`sig`**，
  签名材料走 query string，不是裸 GET。
- `/fetch` 的返回字段是 **`envelopes`**，不是 `mails`。
- `/drop` 是 owner 签名、域 `drop`、parts `[mailbox_key, ids.join(',')]`，与 `/ack`
  完全同形 —— 唯一的差别就是域字符串。
- 签名材料 `parts` 已由 `relay/test/helpers.mjs` 现成提供（`createBody`/`registerBody`/
  `sendBody`/`fetchBody`/`ackBody`/`statusBody`），`smoke.mjs` 直接复用，不重写一遍 HMAC。

## 出事先看

- 命令跑完打印了镜像 tag、容器状态、回滚命令。
- 详细排障、手工回滚、配额解释、绝不删的路径：`references/runbook.md`。
- 协议与数据面的正确性由仓库测试负责，**部署脚本不重复实现**：
  `cd relay && npm test`（41 个用例）。
