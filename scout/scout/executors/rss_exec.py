"""RSS/Atom feed executor.

Many sites block scraping but expose feeds; Product Hunt, blogs, changelogs.
Standard-library-only parsing (xml.etree), no extra dependency.
"""
from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from typing import Any

import httpx

from .httpx_exec import MAX_RESPONSE_BYTES, REQUEST_TIMEOUT_S, USER_AGENT

_ATOM = "{http://www.w3.org/2005/Atom}"
_RSS = "rss/channel"
_NS_MAP = {"atom": "http://www.w3.org/2005/Atom"}


def _clean(text: str | None) -> str | None:
    if not text:
        return None
    v = re.sub(r"<[^>]+>", " ", text)  # strip inline html
    v = re.sub(r"\s+", " ", v).strip()
    return v or None


def _entry_to_item(entry: ET.Element, id_field: str) -> dict[str, Any] | None:
    item: dict[str, Any] = {}

    def sub(tag: str) -> str | None:
        el = entry.find(tag)
        if el is None:
            el = entry.find(f"atom:{tag}", _NS_MAP)
        return el.text if el is not None else None

    title = _clean(sub("title"))
    link_el = entry.find("atom:link", _NS_MAP)
    link = link_el.get("href") if link_el is not None else sub("link")
    summary = _clean(sub("summary") or sub("description") or sub("content"))

    if title:
        item["title"] = title
    if link:
        item["link"] = link
    if summary:
        item["summary"] = summary
    if sub("updated"):
        item["updated"] = sub("updated")
    if sub("published"):
        item["published"] = sub("published")

    entry_id = sub("id") or sub("guid")
    if id_field == "link":
        key = link
    elif id_field == "title":
        key = title
    else:
        key = entry_id
    if not key:
        return None
    item["id"] = str(key)
    return item


async def fetch(spec: dict[str, Any]) -> list[dict[str, Any]]:
    headers = {"User-Agent": USER_AGENT}
    headers.update(spec.get("headers", {}) or {})
    import os

    token_env = spec.get("token_env")
    if token_env and os.environ.get(token_env, "").strip():
        headers["Authorization"] = f"Bearer {os.environ[token_env].strip()}"

    async with httpx.AsyncClient(timeout=REQUEST_TIMEOUT_S, follow_redirects=False) as client:
        resp = await client.get(spec["url"], params=spec.get("params") or {}, headers=headers)
        resp.raise_for_status()
        body = resp.content
        if len(body) > MAX_RESPONSE_BYTES:
            raise ValueError(f"source {spec.get('id')}: response {len(body)} bytes exceeds limit")

    root = ET.fromstring(body)
    if root.tag == "rss":
        channel = root.find("channel")
        if channel is None:
            raise ValueError(f"source {spec.get('id')}: rss feed missing channel")
        entries = channel.findall("item")
    elif root.tag == f"{_ATOM}feed":
        entries = root.findall(f"{_ATOM}entry")
    else:
        raise ValueError(f"source {spec.get('id')}: unrecognized feed root <{root.tag}>")

    id_field = (spec.get("parse") or {}).get("item_id", "id")
    items: list[dict[str, Any]] = []
    for e in entries:
        item = _entry_to_item(e, id_field)
        if item:
            items.append(item)
    return items
