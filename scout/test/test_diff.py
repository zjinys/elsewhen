"""Diff engine tests: new / changed / gone, id stability, first run."""
from scout.diff import diff_items


def _repo(key: str, stars: int) -> dict:
    return {"id": key, "name": key, "stars": stars}


def test_first_run_all_new():
    findings = diff_items([_repo("a", 1), _repo("b", 2)], None)
    assert sorted((f["kind"], f["item_key"]) for f in findings) == [
        ("new", "a"),
        ("new", "b"),
    ]


def test_empty_previous_all_new():
    findings = diff_items([_repo("a", 1)], [])
    assert [f["kind"] for f in findings] == ["new"]


def test_no_changes_no_findings():
    cur = [_repo("a", 5), _repo("b", 3)]
    prev = [_repo("a", 5), _repo("b", 3)]
    assert diff_items(cur, prev) == []


def test_changed_detected():
    cur = [_repo("a", 9), _repo("b", 3)]
    prev = [_repo("a", 5), _repo("b", 3)]
    findings = diff_items(cur, prev)
    assert len(findings) == 1
    assert findings[0]["kind"] == "changed"
    assert findings[0]["item_key"] == "a"
    assert findings[0]["payload"]["stars"] == 9


def test_gone_detected():
    cur = [_repo("a", 5)]
    prev = [_repo("a", 5), _repo("b", 3)]
    findings = diff_items(cur, prev)
    assert len(findings) == 1
    assert findings[0]["kind"] == "gone"
    assert findings[0]["item_key"] == "b"


def test_mixed_kinds():
    cur = [_repo("a", 5), _repo("c", 1)]
    prev = [_repo("a", 4), _repo("b", 2)]
    findings = diff_items(cur, prev)
    got = {(f["item_key"], f["kind"]) for f in findings}
    assert got == {("a", "changed"), ("b", "gone"), ("c", "new")}


def test_missing_id_raises():
    import pytest

    with pytest.raises(ValueError):
        diff_items([{"name": "no-id"}], None)


def test_field_order_does_not_affect_change_detection():
    # same content, different key order must NOT be a change
    cur = [{"id": "x", "stars": 5, "name": "x"}]
    prev = [{"id": "x", "name": "x", "stars": 5}]
    assert diff_items(cur, prev) == []
