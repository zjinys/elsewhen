"""Executor registry: maps spec['executor'] to an executor module.

v1 ships httpx (json/html) + rss; curl_cffi / playwright / scrapy slots are
reserved and raise a clear error until implemented (each is an optional
dependency).
"""
from __future__ import annotations

from typing import Any

from . import httpx_exec, rss_exec

_REGISTRY: dict[str, Any] = {
    "httpx": httpx_exec,
    "rss": rss_exec,
}

_RESERVED = ("curl_cffi", "playwright", "scrapy", "crawl4ai")


def get_executor(name: str):
    if name in _REGISTRY:
        return _REGISTRY[name]
    if name in _RESERVED:
        raise ValueError(
            f"executor {name!r} is reserved but not yet implemented; "
            f"add its module to scout/executors and the optional dependency"
        )
    raise ValueError(f"unknown executor {name!r}")


def available() -> list[str]:
    return sorted(_REGISTRY)
