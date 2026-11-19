"""RSS/Atom executor tests with fixtures (no network)."""
import httpx
import pytest

from scout.executors import rss_exec

ATOM = b"""<?xml version="1.0" encoding="UTF-8"?>
<feed xml:lang="en-US" xmlns="http://www.w3.org/2005/Atom">
  <id>tag:example.com,2005:/feed</id>
  <title>Example Feed</title>
  <entry>
    <id>tag:example.com,2005:Post/1</id>
    <title>Product One &lt;b&gt;bold&lt;/b&gt;</title>
    <link rel="alternate" type="text/html" href="https://example.com/p1"/>
    <summary>&lt;p&gt;A short  blurb&lt;/p&gt;</summary>
    <published>2026-11-15T08:00:00Z</published>
  </entry>
  <entry>
    <id>tag:example.com,2005:Post/2</id>
    <title>Product Two</title>
    <link rel="alternate" type="text/html" href="https://example.com/p2"/>
    <published>2026-11-15T07:00:00Z</published>
  </entry>
</feed>
"""

RSS = b"""<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>Old Feed</title>
  <item>
    <guid>https://old.example/1</guid>
    <title>Old Post</title>
    <link>https://old.example/1</link>
    <description><![CDATA[<p>desc with <b>html</b></p>]]></description>
  </item>
</channel></rss>
"""


def _patch(monkeypatch, body: bytes):
    def handler(request):
        return httpx.Response(200, content=body)

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))

    class Fake:
        def __init__(self, *a, **kw):
            pass

        async def __aenter__(self):
            return client

        async def __aexit__(self, *a):
            return False

    monkeypatch.setattr(httpx, "AsyncClient", Fake)


async def test_atom_entries(monkeypatch):
    _patch(monkeypatch, ATOM)
    items = await rss_exec.fetch({"id": "t", "url": "https://x/feed"})
    assert len(items) == 2
    assert items[0]["id"] == "tag:example.com,2005:Post/1"
    assert items[0]["title"] == "Product One bold"  # html stripped
    assert items[0]["link"] == "https://example.com/p1"
    assert items[0]["summary"] == "A short blurb"


async def test_rss_entries(monkeypatch):
    _patch(monkeypatch, RSS)
    items = await rss_exec.fetch({"id": "t", "url": "https://x/rss"})
    assert len(items) == 1
    assert items[0]["id"] == "https://old.example/1"
    assert items[0]["title"] == "Old Post"
    assert "html" in items[0]["summary"] and "<" not in items[0]["summary"]


async def test_item_id_link(monkeypatch):
    _patch(monkeypatch, ATOM)
    items = await rss_exec.fetch(
        {"id": "t", "url": "https://x/feed", "parse": {"item_id": "link"}}
    )
    assert items[0]["id"] == "https://example.com/p1"


async def test_bad_root_raises(monkeypatch):
    _patch(monkeypatch, b"<html><body>not a feed</body></html>")
    with pytest.raises(ValueError, match="unrecognized feed root"):
        await rss_exec.fetch({"id": "t", "url": "https://x/feed"})


async def test_registry_has_rss():
    from scout.executors.registry import available, get_executor

    assert "rss" in available()
    assert get_executor("rss") is rss_exec
