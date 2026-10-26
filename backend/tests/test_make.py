import pytest

from app.engine.core import core
from app.templates import registry

LONGREAD = "magazine-longread-swiss"
ONE_PAGE = "report-one-pager-summary"


def article(paragraphs, words=30):
    body = "\n\n".join(f"Paragraph {i} " + "words flow along the column " * (words // 5) for i in range(paragraphs))
    return f"# The long read\n\nA standfirst for the piece.\n\n{body}\n"


def make(client, tmp_path, template, md, brief="A magazine long read", fills=None):
    src = tmp_path / "article.md"
    src.write_text(md, encoding="utf-8")
    data = {"brief": brief, "template": template}
    with src.open("rb") as f:
        r = client.post("/api/make/quick", files=[("files", ("article.md", f, "text/markdown"))], data=data)
    assert r.status_code == 200, r.text
    return r.json()["report"]


def named(report, key):
    return {v.split("=", 1)[0] for v in report.get(key, [])}


def slot_names(client, template):
    return set(client.portal.call(lambda: registry.check_spec(registry.get(template), render=False))["slots"])


@pytest.mark.parametrize("template", [LONGREAD, ONE_PAGE, "newsletter-studio-update", "print-flyer-local-business"])
def test_every_named_slot_is_filled_or_reported(client, tmp_path, template):
    rep = make(client, tmp_path, template, article(4))
    accounted = named(rep, "filled") | named(rep, "kept") | named(rep, "placeholders") | named(rep, "emptied")
    missing = slot_names(client, template) - accounted
    assert not missing, f"slots neither filled nor reported: {sorted(missing)}"
    for key in ("kept", "emptied", "placeholders"):
        if rep.get(key):
            assert any(rep[key][0] in line for line in rep.get("next", [])), f"the next steps name what is left in {key}"


def test_the_title_and_body_come_from_the_article(client, tmp_path):
    rep = make(client, tmp_path, LONGREAD, article(3))
    assert "title" in named(rep, "filled")
    assert any(f.startswith("flow:body(") for f in rep["filled"])
    slots = client.get("/api/slots", params={"full": True}).json()["slots"]
    assert "The long read" in str(slots)


def test_a_long_article_gets_continuation_pages_and_the_report_counts_them(client, tmp_path):
    template_pages = len(registry.get(LONGREAD)["pages"])
    rep = make(client, tmp_path, LONGREAD, article(60))
    ins = client.portal.call(core.inspect)
    assert ins["pageCount"] > template_pages
    assert rep["pages"] == ins["pageCount"], "the report states the document's real page count"
    assert rep.get("addedPages") == ins["pageCount"] - template_pages
    body = [s for s in ins["stories"] if len(s.get("frames") or []) > 1]
    assert body and not any(s.get("overset") for s in body), "the body story holds all of its text"


def test_a_short_article_adds_no_pages(client, tmp_path):
    rep = make(client, tmp_path, ONE_PAGE, article(1, words=10))
    ins = client.portal.call(core.inspect)
    assert rep["pages"] == ins["pageCount"] == len(registry.get(ONE_PAGE)["pages"])
    assert "addedPages" not in rep


def test_set_slots_replaces_a_kept_slot(client, tmp_path):
    rep = make(client, tmp_path, LONGREAD, article(2))
    slot = rep["kept"][0]
    r = client.post("/api/slots", json={"values": {slot: "Written by the test"}})
    assert r.status_code == 200, r.text
    assert "Written by the test" in str(client.get("/api/slots", params={"full": True}).json()["slots"])


def test_an_unknown_template_is_refused(client, tmp_path):
    src = tmp_path / "a.md"
    src.write_text("# Title\n\nText.\n", encoding="utf-8")
    with src.open("rb") as f:
        r = client.post("/api/make/quick", files=[("files", ("a.md", f, "text/markdown"))], data={"template": "no-such-template"})
    assert r.status_code == 422
    assert "no template" in r.text
