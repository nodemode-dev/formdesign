import pytest

from app.engine.core import core
from app.templates import registry, spec

SHIPPED = sorted(tid for tid, t in registry.templates(refresh=True).items() if not t.get("_user"))


def test_the_library_ships_templates_in_every_category():
    cats = {registry.get(t).get("category") for t in SHIPPED}
    assert len(SHIPPED) >= 60
    assert {"magazine", "newsletter", "report", "print", "ad", "presentation", "stationery", "web", "book"} <= cats


@pytest.mark.parametrize("tid", SHIPPED)
def test_every_shipped_template_validates(client, tid):
    t = registry.get(tid)
    assert spec.problems(t) == []
    rep = client.portal.call(lambda: registry.check_spec(t, render=False))
    assert rep["ok"], rep.get("error")
    assert rep["pages"] >= 1
    assert rep["overset"] == [], f"{tid}: sample text overflows a slot outside the flows: stories {rep['overset']}"
    assert rep["slots"] or rep["flows"], "a template has named slots or flows"


@pytest.mark.parametrize("broken", [
    {"id": "x", "pages": "many"},
    {"id": "x", "name": "X", "category": "print", "page": {"size": "A4"}, "pages": ["one"]},
    {"id": "x", "name": "X", "category": "print", "page": "A4", "pages": []},
    {"id": "x", "name": "X", "category": "print", "page": {"size": "A4"}, "pages": [], "parents": ["A"]},
    {"id": "x", "name": "X", "category": "print", "page": {"size": "A4"}, "pages": [], "flow": "body"},
])
def test_a_broken_spec_is_refused_with_its_problems(client, broken):
    r = client.post("/api/templates/validate", json={"spec": broken})
    body = r.json()
    assert r.status_code == 200 and body["ok"] is False and body["problems"]


def test_a_shipped_template_cannot_be_overwritten(client):
    t = {k: v for k, v in registry.get(SHIPPED[0]).items() if not k.startswith("_")}
    assert client.post("/api/templates", json={"spec": t}).status_code == 409


def test_a_template_opens_as_a_new_document(client):
    r = client.post(f"/api/templates/{SHIPPED[0]}/new", json={})
    assert r.status_code == 200, r.text
    ins = client.portal.call(core.inspect)
    assert ins["pageCount"] >= 1
