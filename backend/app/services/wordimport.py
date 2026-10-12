"""Microsoft Word Import Options for placing a .docx.

The core reads a .docx's paragraph styles by name, bold and italic, and nothing else, and file.place takes no style map. So the
Word file is rewritten before it is placed (style names mapped to the document's, direct formatting dropped, quotes curled), and what
the core cannot read at all (character styles, footnotes) is applied to the placed story afterwards with the ordinary style and
footnote commands, at offsets worked out from the same rewritten file and checked against the story's text first.
"""
import hashlib
import io
import json
import re
import zipfile
from pathlib import Path

from lxml import etree

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services.website import SESSION

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
XML = "http://www.w3.org/XML/1998/namespace"
NS = {"w": W}
STORE = config.DATA_DIR / "word-import.json"
CACHE = config.CACHE_DIR / "word-import"
DEFAULTS = {"removeStyles": False, "preserveOverrides": True, "footnotes": True, "quotes": False}
PENDING: list[dict] = []


def q(tag: str) -> str:
    return f"{{{W}}}{tag}"


def _attr(e, name: str) -> str | None:
    return e.get(q(name))


def _read(path: str) -> tuple[zipfile.ZipFile, dict]:
    p = Path(path).expanduser()
    if p.suffix.lower() != ".docx" or not p.is_file():
        raise CoreError(f"{p.name} is not a .docx file")
    try:
        z = zipfile.ZipFile(p)
        names = set(z.namelist())
    except zipfile.BadZipFile as exc:
        raise CoreError(f"{p.name} is not a Word file") from exc
    if "word/document.xml" not in names:
        raise CoreError(f"{p.name} is not a Word file: no word/document.xml")
    return z, {n: None for n in names}


def _xml(z: zipfile.ZipFile, name: str):
    try:
        return etree.fromstring(z.read(name))
    except KeyError:
        return None
    except etree.XMLSyntaxError as exc:
        raise CoreError(f"{name} in the Word file is damaged: {exc}") from exc


def style_table(styles) -> dict[str, dict]:
    out = {}
    if styles is None:
        return out
    for s in styles.findall("w:style", NS):
        sid = _attr(s, "styleId")
        name = s.find("w:name", NS)
        kind = _attr(s, "type")
        if sid and kind in ("paragraph", "character"):
            out[sid] = {"id": sid, "name": _attr(name, "val") if name is not None else sid, "type": kind, "default": _attr(s, "default") in ("1", "true"), "el": s}
    return out


def scan(path: str) -> dict:
    z, _ = _read(path)
    with z:
        doc, styles = _xml(z, "word/document.xml"), _xml(z, "word/styles.xml")
        table = style_table(styles)
        counts_p: dict[str, int] = {}
        counts_c: dict[str, int] = {}
        normal = next((s for s in table.values() if s["type"] == "paragraph" and s["default"]), None)
        for p in doc.iter(q("p")):
            ps = p.find("w:pPr/w:pStyle", NS)
            sid = _attr(ps, "val") if ps is not None else (normal["id"] if normal else None)
            if sid:
                counts_p[sid] = counts_p.get(sid, 0) + 1
        for r in doc.iter(q("r")):
            rs = r.find("w:rPr/w:rStyle", NS)
            if rs is not None and _attr(rs, "val"):
                counts_c[_attr(rs, "val")] = counts_c.get(_attr(rs, "val"), 0) + 1
        text = "".join(t.text or "" for t in doc.iter(q("t")))
        foot = len(doc.findall(".//w:footnoteReference", NS))

        def rows(kind: str, counts: dict[str, int]) -> list[dict]:
            out = []
            for sid, n in counts.items():
                s = table.get(sid)
                out.append({"id": sid, "name": s["name"] if s else sid, "count": n})
            return sorted(out, key=lambda r: (-r["count"], r["name"].lower()))

        return {"file": Path(path).name, "paragraph": rows("paragraph", counts_p), "character": rows("character", counts_c), "footnotes": foot,
                "straightQuotes": text.count('"') + text.count("'")}


# ------------------------------------------------------------------ memory

def _doc_key(meta) -> str:
    return meta.path if meta.path else f"session:{SESSION}:{meta.uid}"


def _load() -> dict:
    try:
        d = json.loads(STORE.read_text())
        return d if isinstance(d, dict) else {}
    except (OSError, ValueError):
        return {}


async def recall() -> dict:
    meta = await documents.meta_for_active()
    if meta is None:
        return {}
    return _load().get(_doc_key(meta)) or {}


async def remember(styles: dict, options: dict) -> None:
    meta = await documents.meta_for_active()
    if meta is None:
        return
    data = _load()
    data[_doc_key(meta)] = {"styleMap": styles, "options": options}
    if len(data) > 200:
        for k in list(data)[:-200]:
            data.pop(k)
    try:
        STORE.parent.mkdir(parents=True, exist_ok=True)
        STORE.write_text(json.dumps(data, indent=1))
    except OSError:
        pass


# --------------------------------------------------------------- rewriting

def curl(text: str, prev: str) -> str:
    out = []
    for ch in text:
        if ch == '"':
            out.append("“" if (not prev or prev.isspace() or prev in "([{—–‘“") else "”")
        elif ch == "'":
            out.append("‘" if (not prev or prev.isspace() or prev in "([{—–“‘") else "’")
        else:
            out.append(ch)
        prev = out[-1]
    return "".join(out)


def para_text_items(p):
    """What the core makes of a paragraph, in order: ('t', text, run) for w:t, ('\\t'), ('\\u2028'), and footnote references."""
    for e in p.iter():
        tag = etree.QName(e).localname if isinstance(e.tag, str) else ""
        if e.tag == q("t"):
            yield "t", e.text or "", e
        elif tag == "tab" and e.getparent() is not None and e.getparent().tag != q("tabs"):
            yield "t", "\t", e
        elif tag in ("br", "cr"):
            yield "t", " ", e
        elif tag == "footnoteReference":
            yield "f", _attr(e, "id"), e


def _run_of(e):
    while e is not None and e.tag != q("r"):
        e = e.getparent()
    return e


def build(path: str, style_map: dict, options: dict, known: dict[str, set[str]]) -> tuple[bytes, dict]:
    """The rewritten .docx, plus the jobs the core cannot do: character-style runs and footnotes, with story offsets."""
    opt = {**DEFAULTS, **{k: bool(v) for k, v in (options or {}).items() if k in DEFAULTS}}
    z, _ = _read(path)
    with z:
        doc, styles = _xml(z, "word/document.xml"), _xml(z, "word/styles.xml")
        foot = _xml(z, "word/footnotes.xml")
        table = style_table(styles)
        pmap, cmap = (style_map or {}).get("paragraph") or {}, (style_map or {}).get("character") or {}
        report = {"renamed": 0, "newStyles": [], "warnings": []}

        def target(kind: str, name: str, mapping: dict) -> str | None:
            """The document style name a Word style becomes, or None to keep Word's own name."""
            m = mapping.get(name)
            have = known[kind]
            if m is None:
                return None
            mode = m.get("mode")
            if mode == "existing" and m.get("name"):
                return str(m["name"])
            if mode == "new" and str(m.get("name") or "").strip():
                nm = str(m["name"]).strip()
                if nm not in have and nm.lower() not in {h.lower() for h in have}:
                    report["newStyles"].append({"kind": kind, "name": nm})
                return nm
            return None

        normal = next((s for s in table.values() if s["type"] == "paragraph" and s["default"]), None)
        for e in list(doc.iter(q("tabs"))):
            e.getparent().remove(e)  # the core reads a tab stop's w:tab as a typed tab
        if opt["removeStyles"]:
            for e in list(doc.iter(q("pStyle"))) + list(doc.iter(q("rStyle"))):
                e.getparent().remove(e)
            for e in list(doc.iter(q("rPr"))):
                for c in list(e):
                    e.remove(c)
            pmap, cmap = {}, {}
        else:
            nm = target("paragraph", normal["name"], pmap) if normal else None
            if normal and nm and styles is not None:
                # core reads a paragraph without a style as "Normal", which is its own default style; make that explicit
                synth = "FdNormal"
                s = etree.SubElement(styles, q("style"), {q("type"): "paragraph", q("styleId"): synth})
                etree.SubElement(s, q("name")).set(q("val"), nm)
                for p in doc.iter(q("p")):
                    if p.find("w:pPr/w:pStyle", NS) is None:
                        ppr = p.find("w:pPr", NS)
                        if ppr is None:
                            ppr = etree.Element(q("pPr"))
                            p.insert(0, ppr)
                        ps = etree.Element(q("pStyle"))
                        ps.set(q("val"), synth)
                        ppr.insert(0, ps)
                report["renamed"] += 1
            for sid, s in table.items():
                if s["type"] != "paragraph":
                    continue
                nm = target("paragraph", s["name"], pmap)
                if nm is not None:
                    el = s["el"].find("w:name", NS)
                    if el is not None:
                        el.set(q("val"), nm)
                        report["renamed"] += 1
            if not opt["preserveOverrides"]:
                for e in list(doc.iter(q("b"))) + list(doc.iter(q("i"))):
                    if e.getparent() is not None and e.getparent().tag == q("rPr") and e.getparent().getparent().tag == q("r"):
                        e.getparent().remove(e)

        # quotes, then offsets (so offsets describe the text that is placed)
        text_parts: list[str] = []
        runs: list[dict] = []
        notes: list[dict] = []
        pos = 0
        paras = list(doc.iter(q("p")))
        foot_text = {}
        if foot is not None and opt["footnotes"]:
            for fn in foot.findall("w:footnote", NS):
                fid = _attr(fn, "id")
                body = "\n".join("".join(t.text or "" for t in p.iter(q("t"))) for p in fn.iter(q("p")))
                if fid is not None and body.strip():
                    foot_text[fid] = body.strip()
        for i, p in enumerate(paras):
            prev = ""
            cur = ""
            here = pos
            for kind, val, e in list(para_text_items(p)):
                if kind == "t":
                    if e.tag == q("t") and opt["quotes"]:
                        e.text = curl(val, prev)
                        val = e.text
                    cur += val
                    if val:
                        prev = val[-1]
                        run = _run_of(e)
                        rs = run.find("w:rPr/w:rStyle", NS) if run is not None else None
                        if rs is not None and not opt["removeStyles"]:
                            sid = _attr(rs, "val")
                            s = table.get(sid)
                            runs.append({"start": here + len(cur) - len(val), "end": here + len(cur), "word": s["name"] if s else sid})
                elif kind == "f" and val in foot_text:
                    notes.append({"at": here + len(cur), "text": foot_text[val]})
            text_parts.append(cur)
            pos = here + len(cur) + 1
        # merge adjacent runs of one style
        merged: list[dict] = []
        for r in runs:
            if merged and merged[-1]["word"] == r["word"] and merged[-1]["end"] == r["start"]:
                merged[-1]["end"] = r["end"]
            else:
                merged.append(dict(r))
        jobs_runs = []
        for r in merged:
            m = cmap.get(r["word"])
            nm = target("character", r["word"], cmap)
            mode = (m or {}).get("mode")
            if mode == "word" or m is None:
                name = r["word"]
                if not any(name.lower() == h.lower() for h in known["character"]):
                    report["newStyles"].append({"kind": "character", "name": name})
            else:
                name = nm
            if name:
                jobs_runs.append({**r, "style": name})
        for s in {j["style"] for j in jobs_runs}:
            if not any(s.lower() == h.lower() for h in known["character"]) and not any(n["name"] == s and n["kind"] == "character" for n in report["newStyles"]):
                report["newStyles"].append({"kind": "character", "name": s})

        out = io.BytesIO()
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zo:
            for info in z.infolist():
                data = z.read(info.filename)
                if info.filename == "word/document.xml":
                    data = etree.tostring(doc, xml_declaration=True, encoding="UTF-8", standalone=True)
                elif info.filename == "word/styles.xml" and styles is not None:
                    data = etree.tostring(styles, xml_declaration=True, encoding="UTF-8", standalone=True)
                zo.writestr(info, data)
        jobs = {"text": "\n".join(text_parts), "runs": jobs_runs, "notes": notes}
        report["options"] = opt
        return out.getvalue(), {"jobs": jobs, "report": report}


# ----------------------------------------------------------------- placing

async def known_styles() -> dict[str, set[str]]:
    s = await core.exec("style.list", {})
    return {"paragraph": set(s.get("paragraph") or []), "character": set(s.get("character") or [])}


def _bytes_at(text: str, n: int) -> int:
    return len(text[:n].encode())


async def apply_jobs(story: int, jobs: dict) -> dict:
    """Character styles and footnotes onto a placed story, after checking it is the text these offsets describe."""
    got = (await core.exec("story.get", {"story": story})).get("text", "")
    want = jobs["text"]
    if got.rstrip("\n") != want.rstrip("\n"):
        return {"applied": False, "reason": "the placed text differs from the Word text, so character styles and footnotes were not applied"}
    runs, notes = jobs["runs"], jobs["notes"]
    made = {s["name"] for s in jobs.get("create", [])}
    cmds: list[dict] = []
    for name in sorted({r["style"] for r in runs}):
        if name in made:
            cmds.append({"command": "style.character.create", "params": {"name": name}})
    for r in runs:
        cmds.append({"command": "text.select", "params": {"story": story, "anchor": _bytes_at(got, r["start"]), "focus": _bytes_at(got, r["end"])}})
        cmds.append({"command": "style.character.apply", "params": {"name": r["style"]}})
    for n in sorted(notes, key=lambda x: -x["at"]):
        b = _bytes_at(got, n["at"])
        cmds.append({"command": "text.select", "params": {"story": story, "anchor": b, "focus": b}})
        cmds.append({"command": "footnote.insert", "params": {"text": n["text"]}})
    if not cmds:
        return {"applied": True, "runs": 0, "footnotes": 0}
    await core.batch(commands=cmds, atomic=True, source="wordimport")
    return {"applied": True, "runs": len(runs), "footnotes": len(notes)}


async def place(path: str, style_map: dict, options: dict, frame: int | None = None) -> dict:
    meta = await documents.meta_for_active()
    if meta is None:
        raise CoreError("no document open")
    known = await known_styles()
    data, built = build(path, style_map, options, known)
    jobs, report = built["jobs"], built["report"]
    jobs["create"] = [n for n in report["newStyles"] if n["kind"] == "character"]
    folder = CACHE / hashlib.sha1(data).hexdigest()[:12]
    folder.mkdir(parents=True, exist_ok=True)
    out = folder / Path(path).name
    out.write_bytes(data)
    if frame is None:
        st = await core.state()
        for i in ((st.get("selection") or {}).get("items") or [])[:1]:
            frame = i
    params: dict = {"path": str(out)}
    if frame is not None:
        params["frame"] = frame
    try:
        r = await core.exec("file.place", params, source="wordimport")
    except CoreError:
        if frame is None:
            raise
        r = await core.exec("file.place", {"path": str(out)}, source="wordimport")
        frame = None
    result = {"file": Path(path).name, "report": report, "placed": "frame" if frame is not None and r.get("story") is not None else "cursor"}
    needs = bool(jobs["runs"] or jobs["notes"])
    if r.get("story") is not None:
        result["story"] = r["story"]
        result["frame"] = r.get("frame")
        if needs:
            result["applied"] = await apply_jobs(r["story"], jobs)
    elif needs:
        PENDING.append({"jobs": jobs, "uid": meta.uid, "at": __import__("time").time()})
        result["pending"] = len(PENDING)
    await remember(style_map or {}, report["options"])
    return result


async def finish() -> dict:
    """Apply the jobs of a file loaded into the Place cursor once it has been dropped into a frame."""
    meta = await documents.meta_for_active()
    now = __import__("time").time()
    PENDING[:] = [p for p in PENDING if now - p["at"] < 1800]
    done = []
    if meta is not None and PENDING:
        ins = await core.inspect()
        for p in list(PENDING):
            if p["uid"] != meta.uid:
                continue
            for s in (ins.get("stories") or [])[:300]:
                got = (await core.exec("story.get", {"story": s["id"]})).get("text", "")
                if got.rstrip("\n") == p["jobs"]["text"].rstrip("\n"):
                    done.append({"story": s["id"], **await apply_jobs(s["id"], p["jobs"])})
                    PENDING.remove(p)
                    break
    return {"applied": done, "pending": len(PENDING)}
