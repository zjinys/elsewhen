#!/usr/bin/env bash
# 只读验证：不动容器、不传镜像，只回答「线上那份到底是不是这份代码」。
#
#   ./skills/relay-deploy/scripts/verify.sh          # health + 模块 md5 + 契约冒烟
#   ./skills/relay-deploy/scripts/verify.sh --quick  # 只 health + md5
#
# 单独存在（而不是只做 deploy.sh --verify-only）是因为部署后隔几天想复核时，
# 读一个只做验证的脚本比读整条部署链路快。
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(cd "$HERE/.." && pwd)"
REPO_ROOT="$(cd "$SKILL_DIR/../.." && pwd)"

die() { printf '\n✗ %s\n' "$*" >&2; exit 1; }

. "$SKILL_DIR/config.env"
[ -f "$SKILL_DIR/config.local.env" ] && . "$SKILL_DIR/config.local.env"
RELAY_SSH_PORT="${RELAY_SSH_PORT:-22}"
RELAY_HEALTH_TIMEOUT="${RELAY_HEALTH_TIMEOUT:-15}"

SSH=(ssh -o BatchMode=yes -o ConnectTimeout=10 -p "$RELAY_SSH_PORT" "$RELAY_SSH_USER@$RELAY_HOST")
remote() { "${SSH[@]}" "$@"; }

QUICK=0
[ "${1:-}" = "--quick" ] && QUICK=1

echo "── /health"
# 带重试：刚 docker run 完的那一两秒进程还没 listen，部署后立刻调用也能等到。
# 已经在跑的 relay 会第一轮就通过，所以独立验证时感觉不到这个循环。
HEALTH=""
for i in $(seq 1 "$RELAY_HEALTH_TIMEOUT"); do
  if HEALTH="$(curl -fsS --max-time 2 "http://$RELAY_HOST:$RELAY_PORT/health" 2>&1)"; then
    echo "  $HEALTH（${i}s）"
    break
  fi
  [ "$i" = "$RELAY_HEALTH_TIMEOUT" ] && die "等 ${RELAY_HEALTH_TIMEOUT}s 仍未通过 /health：$HEALTH
看日志：ssh -p $RELAY_SSH_PORT $RELAY_SSH_USER@$RELAY_HOST docker logs --tail 50 $RELAY_CONTAINER"
  sleep 1
done
echo "$HEALTH" | grep -q '"version":2' || die "协议版本不是 2 —— 客户端可能不认这个 relay"

echo "── 容器"
remote "docker inspect -f '状态={{.State.Status}} 健康={{if .State.Health}}{{.State.Health.Status}}{{else}}无{{end}} 镜像={{.Config.Image}} 启动={{.State.StartedAt}}' '$RELAY_CONTAINER'" \
  || die "查不到容器 $RELAY_CONTAINER"

echo "── 模块版本（关键：老镜像只有 server.mjs）"
WANT="$( cd "$REPO_ROOT/relay" && md5sum standalone/server.mjs standalone/schema.mjs standalone/sweeper.mjs standalone/blob.mjs | sed 's#standalone/#/app/#' )"
GOT="$(remote "docker exec '$RELAY_CONTAINER' md5sum /app/server.mjs /app/schema.mjs /app/sweeper.mjs /app/blob.mjs" 2>/dev/null || true)"
printf '%s\n' "$GOT" | sed 's/^/  /'
if printf '%s\n' "$WANT" | diff -u <(printf '%s\n' "$WANT" | sort -k2) <(printf '%s\n' "$GOT" | sort -k2) >/dev/null 2>&1; then
  echo "  ✓ 与仓库一致"
else
  echo "  ✗ 与仓库不一致 —— 线上跑的不是当前代码"
  printf '%s\n' "$WANT" | diff -u <(printf '%s\n' "$WANT" | sort -k2) <(printf '%s\n' "$GOT" | sort -k2) || true
  exit 1
fi

if [ "$QUICK" = 0 ]; then
  echo "── 契约冒烟"
  RELAY_BASE="http://$RELAY_HOST:$RELAY_PORT" node "$HERE/smoke.mjs" || die "契约冒烟失败"
fi

echo
echo "验证通过：$RELAY_HOST:$RELAY_PORT 跑的是当前仓库代码"
