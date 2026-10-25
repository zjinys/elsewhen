#!/usr/bin/env bash
# relay 部署/更新：本地构建镜像 → 传到服务器 → 用**同一份数据卷**重建容器 → 验证。
#
# 为什么是「本地构建、传镜像」而不是「服务器上构建」：
#   服务器（Tower）上没有 node，也没有仓库副本 —— 它只跑 docker。所以镜像必须在
#   有仓库的机器上构建，再 docker save | ssh | docker load 过去。
#
# 用法（在仓库任意位置执行，脚本自己找根目录）：
#   ./skills/relay-deploy/scripts/deploy.sh              # 构建并部署最新代码
#   ./skills/relay-deploy/scripts/deploy.sh --smoke      # 部署后加跑契约冒烟
#   ./skills/relay-deploy/scripts/deploy.sh --tag mytag  # 指定镜像 tag
#   ./skills/relay-deploy/scripts/deploy.sh --rollback   # 回滚到上一个镜像
#   ./skills/relay-deploy/scripts/deploy.sh --verify-only  # 只验证，不动线上
#
# 配置全部在 ../config.env（服务器地址/端口/数据目录/配额），本脚本不含任何常量。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(cd "$HERE/.." && pwd)"
REPO_ROOT="$(cd "$SKILL_DIR/../.." && pwd)"

die() { printf '\n✗ %s\n' "$*" >&2; exit 1; }
say() { printf '\n── %s\n' "$*"; }

# ── 配置 ─────────────────────────────────────────────────────────────────
CONFIG="$SKILL_DIR/config.env"
[ -f "$CONFIG" ] || die "缺配置文件：$CONFIG"
# shellcheck source=/dev/null
. "$CONFIG"
# 本地私密覆盖（不进版本库）：密码、非标准私钥等放这里。
if [ -f "$SKILL_DIR/config.local.env" ]; then
  # shellcheck source=/dev/null
  . "$SKILL_DIR/config.local.env"
fi

: "${RELAY_HOST:?config.env 缺 RELAY_HOST}"
: "${RELAY_SSH_USER:?config.env 缺 RELAY_SSH_USER}"
: "${RELAY_CONTAINER:?config.env 缺 RELAY_CONTAINER}"
: "${RELAY_IMAGE:?config.env 缺 RELAY_IMAGE}"
: "${RELAY_PORT:?config.env 缺 RELAY_PORT}"
: "${RELAY_HOST_DATA_DIR:?config.env 缺 RELAY_HOST_DATA_DIR}"
RELAY_SSH_PORT="${RELAY_SSH_PORT:-22}"
RELAY_UID="${RELAY_UID:-1500}"
RELAY_GID="${RELAY_GID:-1500}"
RELAY_HEALTH_TIMEOUT="${RELAY_HEALTH_TIMEOUT:-60}"
RELAY_KEEP_IMAGES="${RELAY_KEEP_IMAGES:-5}"
RELAY_SMOKE="${RELAY_SMOKE:-0}"

SSH=(ssh -o BatchMode=yes -o ConnectTimeout=10 -p "$RELAY_SSH_PORT" "$RELAY_SSH_USER@$RELAY_HOST")
remote() { "${SSH[@]}" "$@"; }

MODE=deploy
TAG=""
while [ $# -gt 0 ]; do
  case "$1" in
    --smoke) RELAY_SMOKE=1 ;;
    --verify-only) MODE=verify ;;
    --rollback) MODE=rollback ;;
    --tag) shift; TAG="${1:?--tag 需要值}" ;;
    --tag=*) TAG="${1#--tag=}" ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "未知参数：$1" ;;
  esac
  shift
done

# ── 预检 ─────────────────────────────────────────────────────────────────
say "预检"
for tool in docker ssh curl gzip md5sum node; do
  command -v "$tool" >/dev/null 2>&1 || die "本机缺依赖：$tool"
done
for m in server schema sweeper blob; do
  [ -f "$REPO_ROOT/relay/standalone/$m.mjs" ] || die "缺源文件：relay/standalone/$m.mjs"
done

# 服务器只能 root 登（pp/tower/admin 都是 publickey 拒绝），且必须能跑 docker。
remote 'docker version --format "{{.Server.Version}}"' >/dev/null 2>&1 \
  || die "无法以 $RELAY_SSH_USER@$RELAY_HOST 执行 docker（检查 ssh 公钥与 docker 权限）"
echo "  服务器 $RELAY_HOST docker 可达"

CURRENT_TAG="$(remote "docker inspect -f '{{.Config.Image}}' '$RELAY_CONTAINER'" 2>/dev/null || true)"
CURRENT_IMAGE="$(remote "docker inspect -f '{{.Image}}' '$RELAY_CONTAINER'" 2>/dev/null || true)"
echo "  线上容器 ${RELAY_CONTAINER}：${CURRENT_TAG:-<不存在>}"

# 模块 md5 比对与 /health 等待都在 verify.sh 里 —— 同一份逻辑只写一遍，
# 部署只管「换上去」，验证只管「是不是这份代码」。
verify_online() {
  local flag="${1:-}"
  "$HERE/verify.sh" $flag
}

run_container() { # $1 = 镜像引用
  local image="$1"
  # 起容器的参数就是「部署契约」本身，写死在这里而不是散落在 runbook 里：
  #   -v $RELAY_HOST_DATA_DIR:/data  必须保留（信箱/配对关系/enrollment 全在这）
  #   不再挂 /releases、不再传 RELEASES_DIR —— R0 已删除应用内分发功能，
  #   老容器还留着这两个是为历史遗留，重建时绝不能带回来。
  remote bash -s <<EOF
set -euo pipefail
docker rm -f '$RELAY_CONTAINER' >/dev/null 2>&1 || true
mkdir -p '$RELAY_HOST_DATA_DIR'
# 容器以内置 UID $RELAY_UID 运行，数据卷必须让它可写（幂等 chown）。
chown -R '$RELAY_UID:$RELAY_GID' '$RELAY_HOST_DATA_DIR'
docker run -d --name '$RELAY_CONTAINER' \\
  --restart unless-stopped \\
  -p '$RELAY_PORT:$RELAY_PORT' \\
  -e PORT='$RELAY_PORT' \\
  -e DATA_DIR=/data \\
  -e MAILBOX_QUOTA_BYTES='${RELAY_MAILBOX_QUOTA_BYTES:-536870912}' \\
  ${RELAY_BLOB_QUOTA_BYTES:+-e BLOB_QUOTA_BYTES=$RELAY_BLOB_QUOTA_BYTES} \\
  -v '$RELAY_HOST_DATA_DIR:/data' \\
  '$image'
EOF
}

# ── 只验证 ───────────────────────────────────────────────────────────────
if [ "$MODE" = verify ]; then
  say "验证线上容器"
  [ -n "$CURRENT_IMAGE" ] || die "线上没有容器 $RELAY_CONTAINER"
  if [ "$RELAY_SMOKE" = "1" ]; then verify_online; else verify_online --quick; fi
  say "验证完成"; exit 0
fi

# ── 回滚 ─────────────────────────────────────────────────────────────────
if [ "$MODE" = rollback ]; then
  say "回滚"
  # 取远端同仓库镜像里、当前容器之外最新的那个。
  TARGET="$(remote "docker images --format '{{.Repository}}:{{.Tag}} {{.CreatedAt}}' '$RELAY_IMAGE'" \
    | grep -v '<none>' | sort -k2 -r | awk '{print $1}' | grep -v "^${CURRENT_TAG}$" | head -1)" || true
  [ -n "${TARGET:-}" ] || die "远端没有可回滚的旧镜像（RELAY_KEEP_IMAGES=${RELAY_KEEP_IMAGES} 可能太小）"
  echo "  回滚到 $TARGET"
  run_container "$TARGET"
  verify_online --quick
  say "回滚完成：$TARGET（数据卷 $RELAY_HOST_DATA_DIR 未动）"
  exit 0
fi

# ── 部署 ─────────────────────────────────────────────────────────────────
TAG="${TAG:-$(date +%Y%m%d-%H%M%S)}"
IMAGE="$RELAY_IMAGE:$TAG"

say "构建镜像 $IMAGE"
# --network=host 不是可选优化：本机 default bridge 建 veth 对会失败
#   failed to add the host <=> sandbox pair interfaces: operation not supported
# 构建阶段不需要网络隔离，直接用宿主网络。
docker build --network=host -f "$REPO_ROOT/relay/docker/Dockerfile" -t "$IMAGE" "$REPO_ROOT/relay" \
  || die "构建失败"

say "本机冒烟启动（不碰线上）"
SMOKE_PORT="$( (shuf -i 20000-29999 -n 1) 2>/dev/null || echo 28999)"
docker rm -f relay-local-smoke >/dev/null 2>&1 || true
# DATA_DIR 指到 /app 下的可写目录：镜像把 /app 归 relay 所有，省掉一次数据卷挂载。
docker run -d --rm --name relay-local-smoke --network=host \
  -e PORT="$SMOKE_PORT" -e DATA_DIR=/app/data "$IMAGE" >/dev/null || die "本机试启动失败"
trap 'docker rm -f relay-local-smoke >/dev/null 2>&1 || true' EXIT
for i in $(seq 1 20); do
  curl -fsS "http://127.0.0.1:$SMOKE_PORT/health" >/dev/null 2>&1 && break
  sleep 1
done
RELAY_BASE="http://127.0.0.1:$SMOKE_PORT" node "$HERE/smoke.mjs" \
  || die "本机镜像冒烟失败 —— 不要把坏镜像推到线上"
docker rm -f relay-local-smoke >/dev/null 2>&1 || true
trap - EXIT

say "传输镜像到 $RELAY_HOST"
# 先 load 再删旧容器：镜像不到位的窗口内线上仍在服务，没有额外停机。
# gzip -1 足够（180MB 镜像，主要是 node 运行时，压得动、不值得为大小多花 CPU）。
docker save "$IMAGE" | gzip -1 | remote 'gunzip | docker load' >/dev/null \
  || die "镜像传输/加载失败（旧容器未动，线上仍在跑）"

say "重建容器"
run_container "$IMAGE"

say "验证（health + 模块 md5）"
verify_online --quick
if [ "$RELAY_SMOKE" = "1" ]; then
  say "契约冒烟"
  RELAY_BASE="http://$RELAY_HOST:$RELAY_PORT" node "$HERE/smoke.mjs"
fi

say "清理远端旧镜像（保留最近 $RELAY_KEEP_IMAGES 个）"
remote "docker images --format '{{.Repository}}:{{.Tag}} {{.CreatedAt}}' '$RELAY_IMAGE'" \
  | grep -v '<none>' | sort -k2 -r | awk 'NR>'"$RELAY_KEEP_IMAGES"' {print $1}' \
  | while read -r old; do
      [ -n "$old" ] && [ "$old" != "$IMAGE" ] && remote "docker rmi '$old'" >/dev/null 2>&1 || true
    done

say "完成"
cat <<EOF
  镜像    $IMAGE
  容器    $RELAY_CONTAINER（$(remote "docker inspect -f '{{.State.Status}}' '$RELAY_CONTAINER'")）
  地址    http://$RELAY_HOST:$RELAY_PORT
  数据卷  $RELAY_HOST_DATA_DIR（未动）
  回滚    ./skills/relay-deploy/scripts/deploy.sh --rollback
EOF
