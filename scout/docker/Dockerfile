# 注：docker.io 直连被 DNS 污染（registry-1.docker.io 解析错误），
# 用 daocloud 镜像前缀；ghcr.io 直连可达。换网络环境可改回 library/python:3.13-slim。
FROM docker.m.daocloud.io/library/python:3.13-slim

COPY --from=ghcr.io/astral-sh/uv:latest /uv /uvx /bin/

WORKDIR /app

# dependency layer first (cached until lockfile changes)
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

# app layer
COPY scout/ scout/
COPY sources.toml .
RUN uv sync --frozen --no-dev

ENV SCOUT_DB=/data/scout.db \
    SCOUT_SOURCES=/app/sources.toml \
    SCOUT_PORT=8787

VOLUME /data
EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["python", "-c", "import urllib.request,os; urllib.request.urlopen(f\"http://127.0.0.1:{os.environ.get('SCOUT_PORT','8787')}/health\", timeout=4)"]

CMD ["uv", "run", "--no-dev", "python", "-m", "scout.server"]
