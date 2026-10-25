# Elsewhen 盲中继（独立版）—— Docker 镜像
# 构建并推送：
#   cd relay && docker build -f docker/Dockerfile -t elsewhen-relay:latest .
#   # 导出给 NAS：
#   docker save elsewhen-relay:latest | gzip > elsewhen-relay.tar.gz
#   # 在 NAS 上：
#   docker load < elsewhen-relay.tar.gz
# 自动构建（NAS 直接拉 git 后构建亦可）。
#
# 运行（数据都在卷里，升级容器不丢）：
#   docker run -d --name elsewhen-relay \
#     -p 8787:8787 \
#     -v /path/to/relay-data:/data \
#     elsewhen-relay:latest
#
# 客户端中继 URL：http://<NAS IP>:8787

FROM node:26-alpine

# node:sqlite 内置（>=22.5），无编译步骤、无原生依赖，musl/alpine 直接可用。
WORKDIR /app
# 四个模块缺一不可：server.mjs 静态 import 了 schema.mjs，运行时还派发到 blob.mjs
# 与 sweeper.mjs（worker 线程），漏 COPY 会在容器启动时 ERR_MODULE_NOT_FOUND。
# 测试文件不进镜像，所以逐文件列。
COPY standalone/server.mjs standalone/schema.mjs standalone/sweeper.mjs standalone/blob.mjs /app/

# 固定 UID=1500，宿主机挂载卷时 chown 1500:1500 即可。
RUN mkdir -p /data/mailboxes \
    && addgroup -S -g 1500 relay && adduser -S -u 1500 -G relay relay \
    && chown -R relay:relay /data /app

ENV NODE_ENV=production
ENV PORT=8787
ENV DATA_DIR=/data

USER relay
EXPOSE 8787

# healthcheck：不碰信箱（无 DO 等价物），纯进程探活。
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s \
  CMD wget -qO- http://127.0.0.1:8787/health >/dev/null 2>&1 || exit 1

CMD ["node", "/app/server.mjs"]
