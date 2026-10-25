# 自建中继（NAS Docker 部署）

盲中继的独立部署版（**唯一实现**）：为一个数据集的设备群保存短期加密信箱。
服务端只见密文，不做业务授权、不合并数据库。

> relay 不承担应用分发：不提供 APK 下载端点，升级靠手动装包。

## 部署到自己的服务器

本文件描述的是**手工步骤**（也适用于群晖/NAS 这类没有仓库副本的机器）。
如果目标是一台能从开发机 ssh 到的 Linux 服务器，直接用仓库里的部署 skill，
它把下面这些步骤连同验证一起固化了，并且能自动发现「部署的是旧镜像」这类故障：

```bash
./skills/relay-deploy/scripts/deploy.sh              # 构建 + 部署 + 验证
./skills/relay-deploy/scripts/verify.sh              # 只读验证线上跑的是不是当前代码
./skills/relay-deploy/scripts/deploy.sh --rollback   # 回滚到上一个镜像
```

服务器地址/端口/数据目录/配额只改 `skills/relay-deploy/config.env`。
**注意**：`/health` 返回 200 **不能**证明部署成功 —— 缺 `schema.mjs`/`sweeper.mjs`/
`blob.mjs` 的旧镜像照样是绿的，必须比对容器内四个模块的 md5。
详见 `skills/relay-deploy/references/runbook.md`。

## 构建镜像

```bash
cd relay
docker build -f docker/Dockerfile -t elsewhen-relay:latest .
```

导出给 NAS（NAS 无外网构建环境时）：

```bash
docker save elsewhen-relay:latest | gzip > elsewhen-relay.tar.gz
# 传到 NAS 后：
docker load < elsewhen-relay.tar.gz
```

## 运行

```bash
mkdir -p /volume1/docker/elsewhen-relay/data
# 容器以 UID 1500 运行，数据卷归属要让它可写：
chown -R 1500:1500 /volume1/docker/elsewhen-relay

docker run -d --name elsewhen-relay \
  --restart unless-stopped \
  -p 8787:8787 \
  -v /volume1/docker/elsewhen-relay/data:/data \
  elsewhen-relay:latest
```

- `-v …/data`：信箱 SQLite 文件（**必须挂卷**，删容器不丢配对）
- NAS 防火墙只需放行 8787/TCP；跨网段访问时路由器不必暴露此端口

## 客户端接线

1. **桌面**：同步设置 → 中继 URL 填 `http://<NAS IP>:8787`
2. **手机**：扫码加入即可（邀请里携带的即是桌面所填地址）
3. 局域网 IP 建议在路由器上给 NAS 做静态 DHCP 绑定，IP 变了要重扫

## 验证

```bash
curl http://<NAS IP>:8787/health
# {"ok":true,"service":"elsewhen-relay","version":2}
```

## 数据面（v3 blob，可选能力）

大对象（同步快照字节流）不进信箱，走独立的 `/blob/*` 端点：

| 端点 | 作用 | 签名方 |
|---|---|---|
| `POST /blob/open` | 开一次传输（声明片数与总字节） | sender |
| `PUT /blob/chunk?blob_id&index` | 上传第 index 片（body 是**裸密文**，不是 base64） | sender |
| `GET /blob/state?blob_id` | 问「已经有哪几片」，续传只补缺口 | owner |
| `GET /blob/chunk?blob_id&from&limit` | 拉连续分片（裸二进制帧，不套 JSON） | owner |
| `POST /blob/ack {blob_id, upto}` | 确认 `index ≤ upto` 已落盘，**立刻释放** | owner |
| `POST /blob/close {blob_id, status}` | 结束并精确回收整个 blob | owner 或 sender |

- 身份是 `(blob_id, idx)`：同一片重复 PUT 在物理上不占第二份空间（这是 4.4× 重复
  副本的结构性解法）。
- 无进展 30 分钟自动回收（`BLOB_TTL_MS`）；在途总量受 `BLOB_QUOTA_BYTES`（默认
  64 MB）限制，**与信件的 `MAILBOX_QUOTA_BYTES` 互不占用** ⇒ 快照挤不掉控制消息。
- `/status` 的 `capabilities` 含 `"blob"` 时才可启用；`blob_bytes` 是已落盘字节，
  `signal_bytes` 是信件字节。不认识的旧客户端继续走 `mail` 路径，无需升级。

## 安全边界

- 传输匿名：信箱 key 即凭据，服务端不知道自然人身份
- 认证 HMAC-SHA256：每一端点签名绑定方法+参数+时间戳+nonce（5 分钟窗口，防重放）
- 内容端到端加密：服务端只见密文，`seq_hint` 是唯一可见的业务字段（用作投递幂等键）；
  blob 侧只见 `blob_id`/长度/索引，`digest` 也按不透明字符串存放
- 仅允许 HTTP 明文用于私网地址（Rust 客户端强制；公网域名一律 HTTPS）

## 本地测试

```bash
cd relay && npm test        # 全部协议 + 客户端互通测试（无需先装依赖，无第三方运行时）
node --test standalone/join-e2e.test.mjs   # 加入流程端到端（需 cargo，约 30s）：
                                           #   1. 双 runtime 完整配对加入（邀请→审批→安装→enrollment→membership→增量收发）
                                           #   2. 丢响应恢复（丢包代理注入 503，授权/回执重投收敛）
                                           #   3. 稳态 tick 增量同步
```
