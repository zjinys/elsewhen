"""httpx executor tests with a mocked transport (no real network)."""
import json

import httpx
import pytest

from scout.executors import httpx_exec


def _mock_client(handler):
    return httpx.AsyncClient(transport=httpx.MockTransport(handler))


@pytest.fixture
def patch_client(monkeypatch):
    def _install(handler):
        client = _mock_client(handler)

        class FakeAsyncClient:
            def __init__(self, *a, **kw):
                pass

            async def __aenter__(self):
                return client

            async def __aexit__(self, *a):
                return False

        monkeypatch.setattr(httpx, "AsyncClient", FakeAsyncClient)

    return _install


def test_dot_path():
    assert httpx_exec._dot_path({"a": {"b": [1, 2]}}, "a.b.1") == 2
    assert httpx_exec._dot_path({"a": 1}, "x.y") is None


async def test_fetch_json_normalizes(patch_client):
    payload = {
        "items": [
            {"full_name": "a/b", "html_url": "u", "stargazers_count": 5, "extra": "x"},
            {"no_id_here": True},
        ]
    }

    def handler(request):
        assert "search/repositories" in str(request.url)
        return httpx.Response(200, content=json.dumps(payload).encode())

    patch_client(handler)
    spec = {
        "id": "t",
        "url": "https://api.test/search/repositories",
        "params": {"q": "x"},
        "parse": {
            "format": "json",
            "items_path": "items",
            "item_id": "full_name",
            "fields": ["full_name", "stargazers_count"],
        },
    }
    items = await httpx_exec.fetch(spec)
    assert len(items) == 1  # item without id dropped
    assert items[0] == {"id": "a/b", "full_name": "a/b", "stargazers_count": 5}


async def test_fetch_json_requires_token_env_when_set(patch_client, monkeypatch):
    seen_headers = {}

    def handler(request):
        seen_headers.update(dict(request.headers))
        return httpx.Response(200, content=b'{"items": [{"id": "x"}]}')

    patch_client(handler)
    monkeypatch.setenv("SCOUT_TEST_TOKEN", "sekrit")
    spec = {
        "id": "t",
        "url": "https://api.test/",
        "token_env": "SCOUT_TEST_TOKEN",
        "parse": {"format": "json", "items_path": "items", "item_id": "id"},
    }
    items = await httpx_exec.fetch(spec)
    assert seen_headers.get("authorization") == "Bearer sekrit"
    assert items == [{"id": "x"}]


async def test_fetch_json_rejects_oversize(patch_client):
    def handler(request):
        return httpx.Response(200, content=b"x" * (httpx_exec.MAX_RESPONSE_BYTES + 1))

    patch_client(handler)
    spec = {"id": "t", "url": "https://api.test/",
            "parse": {"format": "json", "items_path": "items", "item_id": "id"}}
    with pytest.raises(ValueError, match="exceeds limit"):
        await httpx_exec.fetch(spec)


async def test_fetch_json_no_redirect(patch_client):
    def handler(request):
        return httpx.Response(302, headers={"Location": "https://evil.test/"})

    patch_client(handler)
    spec = {"id": "t", "url": "https://api.test/",
            "parse": {"format": "json", "items_path": "items", "item_id": "id"}}
    with pytest.raises(httpx.HTTPStatusError):
        await httpx_exec.fetch(spec)


async def test_fetch_html_rows(patch_client):
    html = b"""
    <html><body>
      <div class="repo"><a class="name" href="/a/b">a/b</a><span class="stars">1,024 stars</span></div>
      <div class="repo"><a class="name" href="/c/d">c/d</a><span class="stars">  77   stars </span></div>
      <div class="repo"><span class="stars">orphan</span></div>
    </body></html>
    """

    def handler(request):
        return httpx.Response(200, content=html)

    patch_client(handler)
    spec = {
        "id": "t",
        "url": "https://api.test/trending",
        "parse": {
            "format": "html",
            "row_selector": "div.repo",
            "item_id": "name",
            "fields": {
                "name": {"selector": "a.name", "attr": "href"},
                "stars": {"selector": "span.stars"},
            },
        },
    }
    items = await httpx_exec.fetch(spec)
    assert len(items) == 2
    assert items[0] == {"name": "/a/b", "stars": "1,024 stars", "id": "/a/b"}
    assert items[1]["stars"] == "77 stars"


async def test_unknown_format_raises():
    with pytest.raises(ValueError, match="unsupported parse.format"):
        await httpx_exec.fetch({"id": "t", "parse": {"format": "pdf"}})
