# 自建中继（NAS Docker 部署）

盲中继的独立部署版：与 Cloudflare Worker 版协议完全一致（v2），客户端零改动，
只是存储从 Durable Objects 换成本地 SQLite（每信箱一个文件）。

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
mkdir -p /volume1/docker/elsewhen-relay/data /volume1/docker/elsewhen-relay/releases
# 容器以 UID 1500 运行，数据卷归属要让它可写：
chown -R 1500:1500 /volume1/docker/elsewhen-relay

docker run -d --name elsewhen-relay \
  --restart unless-stopped \
  -p 8787:8787 \
  -v /volume1/docker/elsewhen-relay/data:/data \
  -v /volume1/docker/elsewhen-relay/releases:/releases \
  elsewhen-relay:latest
```

- `-v …/data`：信箱 SQLite 文件（**必须挂卷**，删容器不丢配对）
- `-v …/releases`：应用升级分发（可选；留空则 `/release/latest` 返 404，不影响配对）
- NAS 防火墙只需放行 8787/TCP；跨网段访问时路由器不必暴露此端口

## 发布一个升级包（可选）

```bash
# manifest: <version>/<hash>/notes 字段与 scripts/publish-release.sh 生成的一致
cp manifest.json  /releases/release/manifest.json
cp app-release.apk /releases/apk/<hash>.apk   # hash = sha256(apk) 的 hex
```

## 客户端接线

1. **桌面**：同步设置 → 中继 URL 填 `http://<NAS IP>:8787`
2. **手机**：扫码加入即可（邀请里携带的即是桌面所填地址）
3. 局域网 IP 建议在路由器上给 NAS 做静态 DHCP 绑定，IP 变了要重扫

## 验证

```bash
curl http://<NAS IP>:8787/health
# {"ok":true,"service":"elsewhen-relay","version":2}
```

## 安全边界（与 CF 版一致）

- 传输匿名：信箱 key 即凭据，服务端不知道自然人身份
- 认证 HMAC-SHA256：每一端点签名绑定方法+参数+时间戳+nonce（5 分钟窗口，防重放）
- 内容端到端加密：服务端只见 base64 密文
- 仅允许 HTTP 明文用于私网地址（Rust 客户端强制；公网域名一律 HTTPS）

## 本地测试

```bash
cd relay/standalone && node --test server.test.mjs   # 8 项协议语义测试
node --test join-e2e.test.mjs                        # 加入流程端到端（需 cargo，约 30s）：
                                                     #   1. 双 runtime 完整配对加入（邀请→审批→安装→enrollment→membership→增量收发）
                                                     #   2. 丢响应恢复（丢包代理注入 503，授权/回执重投收敛）
                                                     #   3. 稳态 tick 增量同步
```
