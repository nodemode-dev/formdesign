import io
import zipfile

import pytest
from pypdf import PdfReader

from app.engine.core import core

TEMPLATES = ["report-one-pager-summary", "magazine-longread-swiss", "newsletter-company-quarterly"]


def open_template(client, tid):
    assert client.post(f"/api/templates/{tid}/new", json={}).status_code == 200
    return client.portal.call(core.inspect)


def export(client, fmt, options=None):
    r = client.post("/api/documents/export", json={"format": fmt, "options": options or {}})
    assert r.status_code == 200, r.text[:500]
    return r.content


def fonts_of(reader):
    found = {}
    for page in reader.pages:
        res = page.get("/Resources") or {}
        for font in (res.get("/Font") or {}).values():
            f = font.get_object()
            for d in [f, *[x.get_object() for x in f.get("/DescendantFonts", [])]]:
                desc = d.get("/FontDescriptor")
                if desc is not None:
                    desc = desc.get_object()
                    found[str(desc.get("/FontName"))] = any(k in desc for k in ("/FontFile", "/FontFile2", "/FontFile3"))
    return found


@pytest.mark.parametrize("tid", TEMPLATES)
def test_pdf_opens_with_the_documents_page_count(client, tid):
    ins = open_template(client, tid)
    data = export(client, "pdf")
    assert data.startswith(b"%PDF-")
    reader = PdfReader(io.BytesIO(data))
    assert len(reader.pages) == ins["pageCount"]


@pytest.mark.parametrize("tid", TEMPLATES)
def test_pdf_embeds_every_font_it_uses(client, tid):
    open_template(client, tid)
    fonts = fonts_of(PdfReader(io.BytesIO(export(client, "pdf"))))
    assert fonts, "the PDF uses fonts"
    assert all(fonts.values()), f"not embedded: {[n for n, e in fonts.items() if not e]}"


def test_pdf_page_size_matches_the_document(client):
    open_template(client, "report-one-pager-summary")
    box = PdfReader(io.BytesIO(export(client, "pdf"))).pages[0].mediabox
    assert abs(float(box.width) - 612) < 0.5 and abs(float(box.height) - 792) < 0.5, "a Letter template exports Letter pages"
    with_bleed = PdfReader(io.BytesIO(export(client, "pdf", {"bleed": True}))).pages[0].mediabox
    assert float(with_bleed.width) >= 612 + 18 - 0.5, "bleed widens the media box by the template's 9 pt each side"


def test_pdf_text_is_selectable(client):
    open_template(client, "report-one-pager-summary")
    text = "".join(p.extract_text() or "" for p in PdfReader(io.BytesIO(export(client, "pdf"))).pages)
    assert len(text.split()) > 20


def test_pdf_of_chosen_pages(client):
    ins = open_template(client, "magazine-longread-swiss")
    assert ins["pageCount"] >= 2
    assert len(PdfReader(io.BytesIO(export(client, "pdf", {"pages": "1"}))).pages) == 1


def test_idml_export_from_the_service_is_a_package(client):
    open_template(client, "newsletter-company-quarterly")
    z = zipfile.ZipFile(io.BytesIO(export(client, "idml")))
    names = z.namelist()
    assert names[0] == "mimetype"
    assert "designmap.xml" in names
    assert any(n.startswith("Spreads/") for n in names) and any(n.startswith("Stories/") for n in names)
    assert z.testzip() is None
