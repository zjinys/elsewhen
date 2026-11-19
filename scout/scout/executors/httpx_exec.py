"""Built-in httpx executor: JSON APIs (dot-path extraction) and static HTML
(CSS selectors). Response/timeout limits enforced here.
"""
from __future__ import annotations

import json
import re
from typing import Any, Protocol

import httpx

MAX_RESPONSE_BYTES = 5 * 1024 * 1024
REQUEST_TIMEOUT_S = 30.0
USER_AGENT = "scout/0.1 (+local-first data collector; contact via config)"


class Executor(Protocol):
    async def fetch(self, spec: dict[str, Any]) -> list[dict[str, Any]]: ...


def _dot_path(obj: Any, path: str) -> Any:
    cur: Any = obj
    for part in path.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        elif isinstance(cur, list) and part.isdigit() and int(part) < len(cur):
            cur = cur[int(part)]
        else:
            return None
    return cur


async def fetch_json(spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Fetch a JSON endpoint and normalize via spec['parse'] rules.

    spec keys used: url, params, headers, token_env, parse{items_path,
    item_id, fields}.
    """
    import asyncio
    import os

    parse = spec.get("parse", {})
    items_path = parse.get("items_path", "items")
    item_id_field = parse.get("item_id")
    if not item_id_field:
        raise ValueError(f"source {spec.get('id')}: parse.item_id is required")
    fields = parse.get("fields")

    headers = {"User-Agent": USER_AGENT}
    headers.update(spec.get("headers", {}) or {})
    token_env = spec.get("token_env")
    if token_env:
        token = os.environ.get(token_env, "")
        if token:
            headers["Authorization"] = f"Bearer {token}"

    async with httpx.AsyncClient(timeout=REQUEST_TIMEOUT_S, follow_redirects=False) as client:
        resp = await client.get(
            spec["url"], params=spec.get("params") or {}, headers=headers
        )
        resp.raise_for_status()
        body = resp.content
        if len(body) > MAX_RESPONSE_BYTES:
            raise ValueError(
                f"source {spec.get('id')}: response {len(body)} bytes exceeds limit"
            )
        data = json.loads(body)

    raw_items = _dot_path(data, items_path)
    if raw_items is None:
        raise ValueError(f"source {spec.get('id')}: items_path {items_path!r} not found")
    if not isinstance(raw_items, list):
        raise ValueError(f"source {spec.get('id')}: items_path {items_path!r} is not a list")

    out: list[dict[str, Any]] = []
    for raw in raw_items:
        if not isinstance(raw, dict):
            continue
        item_id = _dot_path(raw, item_id_field)
        if item_id is None:
            continue
        item: dict[str, Any] = {"id": str(item_id)}
        if fields:
            for f in fields:
                v = _dot_path(raw, f)
                if v is not None:
                    item[str(f).split(".")[-1]] = v
        else:
            item.update({k: v for k, v in raw.items() if k != "id"})
        out.append(item)
    return out


async def fetch_html(spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Fetch a static HTML page and extract rows via CSS selectors.

    spec keys used: url, params, headers, token_env (Bearer on headers),
    parse{row_selector, fields: {out_field: {selector, attr?, text?}}}.
    attr/text default: element text, whitespace-collapsed.
    """
    import os

    try:
        from bs4 import BeautifulSoup
    except ImportError as e:  # pragma: no cover
        raise RuntimeError(
            "html executor requires beautifulsoup4+lxml; "
            "install with `uv add beautifulsoup4 lxml`"
        ) from e

    parse = spec.get("parse", {})
    row_selector = parse.get("row_selector")
    field_rules = parse.get("fields")
    if not row_selector or not field_rules:
        raise ValueError(f"source {spec.get('id')}: html parse needs row_selector and fields")

    headers = {"User-Agent": USER_AGENT}
    headers.update(spec.get("headers", {}) or {})
    token_env = spec.get("token_env")
    if token_env:
        token = os.environ.get(token_env, "")
        if token:
            headers["Authorization"] = f"Bearer {token}"

    async with httpx.AsyncClient(timeout=REQUEST_TIMEOUT_S, follow_redirects=False) as client:
        resp = await client.get(
            spec["url"], params=spec.get("params") or {}, headers=headers
        )
        resp.raise_for_status()
        body = resp.content
        if len(body) > MAX_RESPONSE_BYTES:
            raise ValueError(
                f"source {spec.get('id')}: response {len(body)} bytes exceeds limit"
            )

    soup = BeautifulSoup(body, "lxml")
    out: list[dict[str, Any]] = []
    for row in soup.select(row_selector):
        item: dict[str, Any] = {}
        for out_field, rule in field_rules.items():
            el = row.select_one(rule.get("selector", ""))
            if el is None:
                continue
            if rule.get("attr"):
                v = el.get(rule["attr"])
            else:
                v = el.get_text(" ", strip=True)
            if v:
                item[out_field] = re.sub(r"\s+", " ", str(v)).strip()
        item_id_field = parse.get("item_id")
        item_id = item.get(item_id_field) if item_id_field else None
        if not item_id:
            # rows missing the stable-id field are dropped (same policy as json)
            continue
        item["id"] = str(item_id)
        out.append(item)
    return out


async def fetch(spec: dict[str, Any]) -> list[dict[str, Any]]:
    fmt = (spec.get("parse") or {}).get("format", "json")
    if fmt == "json":
        return await fetch_json(spec)
    if fmt == "html":
        return await fetch_html(spec)
    raise ValueError(f"source {spec.get('id')}: unsupported parse.format {fmt!r}")
