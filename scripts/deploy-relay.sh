#!/usr/bin/env bash
# 部署 Elsewhen 盲中继（Cloudflare Worker + Durable Objects）。
#
# 认证：wrangler 4 自动加载 relay/.env（CLOUDFLARE_API_TOKEN /
# CLOUDFLARE_ACCOUNT_ID，见 relay/.env.example）；或已 `wrangler login` 的 OAuth。
# .env 已 gitignore，token 不会入库。
#
# 用法：
#   ./scripts/deploy-relay.sh            # 协议测试通过后 wrangler deploy
#   ./scripts/deploy-relay.sh --dry-run  # 只测试 + 试部署（不实际上线）
#   ./scripts/deploy-relay.sh --skip-test
set -euo pipefail

cd "$(dirname "$0")/.."
RELAY_DIR=relay

DRY_RUN=0
SKIP_TEST=0
for arg in "$@"; do
  case "$arg" in
    --dry-run)   DRY_RUN=1 ;;
    --skip-test) SKIP_TEST=1 ;;
    *) echo "未知参数: $arg" >&2; exit 2 ;;
  esac
done

if [[ ! -f "$RELAY_DIR/wrangler.toml" ]]; then
  echo "错误：$RELAY_DIR/wrangler.toml 不存在（在仓库根目录外运行？）" >&2
  exit 1
fi

cd "$RELAY_DIR"

# 依赖（node_modules 缺失时安装）
if [[ ! -d node_modules ]]; then
  echo "==> [0/3] npm install"
  npm install
fi

# 协议不变量测试（worker.ts 签名规范与 test/crypto.test.mjs 对齐）
if [[ $SKIP_TEST -eq 0 ]]; then
  echo "==> [1/3] 协议测试 (node --test)"
  npm test
else
  echo "==> [1/3] 跳过协议测试"
fi

# 认证检查：.env 存在或环境变量已设置，二者有其一即可
if [[ ! -f .env && -z "${CLOUDFLARE_API_TOKEN:-}" ]]; then
  echo "错误：未找到认证信息。" >&2
  echo "  cp .env.example .env 并填入 CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID" >&2
  echo "  或执行 npx wrangler login（OAuth）" >&2
  exit 1
fi

echo "==> [2/3] wrangler deploy$([[ $DRY_RUN -eq 1 ]] && echo ' (dry-run)')"
if [[ $DRY_RUN -eq 1 ]]; then
  npx wrangler deploy --dry-run
else
  npx wrangler deploy
fi

echo "==> [3/3] 健康检查"
# 从 wrangler.toml 读 name，推导默认 workers.dev URL
WORKER_NAME=$(grep -E '^name\s*=' wrangler.toml | sed -E 's/^name\s*=\s*"([^"]+)".*/\1/')
if [[ -n "${CLOUDFLARE_ACCOUNT_SUBDOMAIN:-}" ]]; then
  URL="https://${WORKER_NAME}.${CLOUDFLARE_ACCOUNT_SUBDOMAIN}.workers.dev/health"
  echo "GET $URL"
  curl -fsS "$URL" && echo || echo "（健康检查失败；若账号未启用 workers.dev 子域可忽略，以上面 deploy 输出的 URL 为准）"
else
  echo "（未设置 CLOUDFLARE_ACCOUNT_SUBDOMAIN，跳过自动健康检查；以上面 deploy 输出的 URL 为准，可手动 curl <URL>/health）"
fi
