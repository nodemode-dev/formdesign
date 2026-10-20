import base64
import difflib
import hashlib
import io
import json
import time
import uuid
import zipfile
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import DocMeta, documents
from app.services import motion as M

FORMAT = "formdesign-assignment"
FORMATS = (FORMAT, "formdes-assignment")
EXT = ".fdes-assignment"
USER_FILE = config.DATA_DIR / "workflow-user.json"
COLORS = ["#e5654b", "#4b8fe5", "#46aa5a", "#c8961e", "#965ac8", "#1ea0aa"]
CONTENT_PREFIXES = ("text.insert", "text.delete", "story.", "note.", "footnote.", "changes.", "anchor.", "xref.", "hyperlink.")
TEXT_SEL_PREFIXES = ("style.", "para.", "char.", "type.", "format.", "table.", "glyph.", "text.")
CLIP = ("edit.paste", "edit.pasteWithoutFormatting", "edit.cut", "edit.clear")
EDIT_KEYS = {"Backspace", "Delete", "Enter", "Return", "Tab"}
EDITOR_TOOLS = {"type", "hand", "zoom"}

user: dict = {"name": "Designer", "role": "designer"}
_undoable: dict[str, bool] = {}


def load_user() -> None:
    try:
        d = json.loads(USER_FILE.read_text())
        if isinstance(d, dict):
            user.update(name=str(d.get("name") or "Designer")[:60], role="editor" if d.get("role") == "editor" else "designer")
    except (OSError, ValueError):
        pass


def set_user(name: str, role: str) -> dict:
    user.update(name=(name or "").strip()[:60] or "Designer", role="editor" if role == "editor" else "designer")
    try:
        USER_FILE.parent.mkdir(parents=True, exist_ok=True)
        USER_FILE.write_text(json.dumps(user))
    except OSError:
        pass
    core.announce({"type": "workflow", "what": "user"})
    return dict(user)


def store(meta: DocMeta) -> dict:
    w = meta.workflow
    w.setdefault("assignments", [])
    w.setdefault("locks", {})
    w.setdefault("docId", uuid.uuid4().hex[:16])
    return w


def _when(ts: float) -> str:
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(ts))


async def _commands() -> dict[str, bool]:
    if not _undoable:
        r = await core.call("commands", {"enabledOnly": False}, source="guard")
        for c in r.result:
            _undoable[c["id"]] = bool(c.get("undoable"))
    return _undoable


async def _inspect() -> dict:
    return (await core.call("exec", {"command": "document.inspect", "params": {}}, source="guard")).result


def _story_of_items(insp: dict, ids: list[int]) -> list[int]:
    out: list[int] = []
    want = set(ids)
    for sp in insp.get("spreads", []):
        for it in sp.get("items", []):
            if it["id"] in want and it.get("story") is not None and it["story"] not in out:
                out.append(int(it["story"]))
    return out


def _refuse_lock(story: int, lock: dict, who: str, label: str) -> CoreError:
    return CoreError(f"The story {label!r} is checked out to {lock['by']} since {_when(lock['since'])}, so {who} cannot change its text. Ask {lock['by']} to check it in (Window > Assignments), or break the lock there.")


def _verify(story: int, locks: dict, who: str, editor: bool) -> None:
    lock = locks.get(str(story))
    label = (lock or {}).get("label") or f"story {story}"
    if editor:
        if not lock or lock["by"] != who:
            raise CoreError(f"The story {label!r} is not checked out to {who}. In the Editor role text can only be changed in stories checked out to you: check it out in Window > Assignments.")
    elif lock and lock["by"] != who:
        raise _refuse_lock(story, lock, who, label)


async def guard(method: str, params: dict, source: str) -> None:
    if source in ("workflow", "guard"):
        return
    editor = user["role"] == "editor"
    if not editor and not any(m.workflow.get("locks") for m in documents.meta.values()):
        return
    st = (await core.call("state", source="guard")).result
    if st.get("active") is None:
        return
    meta = documents.meta.get(st["docs"][st["active"]]["uid"])
    locks = ((meta.workflow.get("locks") if meta else None) or {})
    if not editor and not locks:
        return
    who = user["name"]
    sel = st.get("selection") or {}
    text_sel = sel.get("text") or None

    if method == "pointer":
        if editor and (params.get("tool") or st.get("tool")) not in EDITOR_TOOLS:
            raise CoreError("The Editor role works with the Type tool: layout tools are off. Switch to the Designer role to change the layout.")
        return
    if method == "text":
        if text_sel:
            _verify(int(text_sel["story"]), locks, who, editor)
        elif editor:
            raise CoreError("Nothing is taking text. Click into a story checked out to you.")
        return
    if method == "key":
        k = str(params.get("key") or "")
        mods = params.get("mods") or {}
        cmd_key = bool(mods.get("cmd") or mods.get("ctrl"))
        if text_sel:
            if k in EDIT_KEYS or (len(k) == 1 and (not cmd_key or k.lower() in "xv")):
                _verify(int(text_sel["story"]), locks, who, editor)
        elif editor and k in ("Backspace", "Delete"):
            raise CoreError("The Editor role cannot delete layout objects; click into a story checked out to you first.")
        return
    if method == "findChange":
        op = params.get("op")
        if op in ("change", "changeFind", "changeAll"):
            if editor and op == "changeAll":
                raise CoreError("Change All is off in the Editor role; use Change in a story checked out to you.")
            if op == "changeAll":
                for s, lk in locks.items():
                    if lk["by"] != who:
                        raise _refuse_lock(int(s), lk, who, lk.get("label") or f"story {s}")
            elif text_sel:
                _verify(int(text_sel["story"]), locks, who, editor)
        return
    cmds: list[tuple[str, dict]] = []
    if method == "exec":
        cmds = [(str(params.get("command")), params.get("params") or {})]
    elif method == "batch":
        if params.get("script") is not None and not params.get("commands"):
            if editor:
                raise CoreError("Scripts are off in the Editor role.")
            return
        cmds = [(str(c.get("command")), c.get("params") or {}) for c in params.get("commands") or [] if isinstance(c, dict)]
    undoable = await _commands()
    insp: dict | None = None
    for cmd, p in cmds:
        if cmd == "script.run":
            if editor:
                raise CoreError("Scripts are off in the Editor role.")
            continue
        if not undoable.get(cmd, True):
            continue
        content = cmd.startswith(CONTENT_PREFIXES) or (cmd in CLIP and bool(text_sel)) or (cmd.startswith(TEXT_SEL_PREFIXES) and bool(text_sel or p.get("story")))
        if content:
            stories: list[int] = []
            if p.get("story") is not None:
                stories = [int(p["story"])]
            elif p.get("frame") is not None or p.get("frames"):
                insp = insp or await _inspect()
                stories = _story_of_items(insp, [int(i) for i in ([p["frame"]] if p.get("frame") is not None else p["frames"])])
            elif text_sel:
                stories = [int(text_sel["story"])]
            elif sel.get("items"):
                insp = insp or await _inspect()
                stories = _story_of_items(insp, [int(i) for i in sel["items"]])
            if not stories and editor:
                raise CoreError("Nothing is selected in a story checked out to you.")
            for s in stories:
                _verify(s, locks, who, editor)
        elif editor and undoable.get(cmd, True):
            raise CoreError(f"The Editor role cannot change the layout ({cmd}). Switch to the Designer role to do that.")


async def active() -> DocMeta:
    meta = await documents.meta_for_active()
    if meta is None:
        raise CoreError("no document open")
    store(meta)
    return meta


async def overview() -> dict:
    meta = await active()
    w = meta.workflow
    insp = await _inspect()
    geo = await M.geometry()
    stories = []
    live = set()
    frames_of: dict[int, list[int]] = {}
    for sp in insp.get("spreads", []):
        for it in sp.get("items", []):
            if it.get("story") is not None:
                frames_of.setdefault(int(it["story"]), []).append(int(it["id"]))
    for s in insp.get("stories", []):
        sid = int(s["id"])
        live.add(str(sid))
        lock = w["locks"].get(str(sid))
        frames = [{"id": f, "page": geo[f]["page"], "pageName": geo[f]["pageName"], "spread": geo[f]["spread"], "bounds": geo[f]["bounds"]} for f in (s.get("frames") or frames_of.get(sid, [])) if f in geo]
        stories.append({"id": sid, "preview": s.get("preview", ""), "length": s.get("length", 0), "frames": frames, "lock": lock,
                        "assignments": [a["id"] for a in w["assignments"] if sid in a["stories"]]})
    for k in list(w["locks"]):
        if k not in live:
            w["locks"].pop(k)
    for a in w["assignments"]:
        a["stories"] = [s for s in a["stories"] if str(s) in live]
    return {"user": dict(user), "docId": w["docId"], "assignments": w["assignments"], "stories": stories, "package": w.get("package")}


def _resolve(insp: dict, stories: list, frames: list) -> list[int]:
    out = [int(s) for s in stories or []]
    for s in _story_of_items(insp, [int(f) for f in frames or []]):
        if s not in out:
            out.append(s)
    known = {int(s["id"]) for s in insp.get("stories", [])}
    bad = [s for s in out if s not in known]
    if bad:
        raise CoreError(f"no story {bad[0]}")
    return out


async def save_assignment(body: dict) -> dict:
    meta = await active()
    w = meta.workflow
    insp = await _inspect()
    stories = _resolve(insp, body.get("stories"), body.get("frames"))
    name = str(body.get("name") or "").strip() or f"Assignment {len(w['assignments']) + 1}"
    cur = next((a for a in w["assignments"] if a["id"] == body.get("id")), None)
    rec = cur or {"id": uuid.uuid4().hex[:8], "created": time.time(), "exported": None}
    color = str(body.get("color") or rec.get("color") or COLORS[len(w["assignments"]) % len(COLORS)])
    rec.update(name=name, assignee=str(body.get("assignee") or rec.get("assignee") or "").strip()[:60], color=color, stories=stories)
    if cur is None:
        w["assignments"].append(rec)
    core.announce({"type": "workflow", "what": "assignments"})
    return rec


def delete_assignment(meta: DocMeta, aid: str) -> None:
    w = store(meta)
    a = next((x for x in w["assignments"] if x["id"] == aid), None)
    if a is None:
        raise CoreError("no such assignment")
    for s in a["stories"]:
        lk = w["locks"].get(str(s))
        if lk and lk.get("assignment") == aid:
            w["locks"].pop(str(s))
    w["assignments"].remove(a)
    core.announce({"type": "workflow", "what": "assignments"})


async def checkout(stories: list[int], who: str | None = None, assignment: str | None = None) -> list[int]:
    meta = await active()
    w = meta.workflow
    insp = await _inspect()
    labels = {int(s["id"]): s.get("preview", "") for s in insp.get("stories", [])}
    who = who or user["name"]
    for s in stories:
        if s not in labels:
            raise CoreError(f"no story {s}")
        lk = w["locks"].get(str(s))
        if lk and lk["by"] != who:
            raise CoreError(f"The story {labels[s][:40]!r} is already checked out to {lk['by']} since {_when(lk['since'])}.")
    for s in stories:
        if str(s) not in w["locks"]:
            w["locks"][str(s)] = {"by": who, "since": time.time(), "assignment": assignment, "label": labels[s][:60]}
    core.announce({"type": "workflow", "what": "locks"})
    return stories


async def checkin(stories: list[int], force: bool = False) -> list[int]:
    meta = await active()
    w = meta.workflow
    done = []
    for s in stories:
        lk = w["locks"].get(str(s))
        if not lk:
            continue
        if lk["by"] != user["name"] and not force:
            raise CoreError(f"The story {lk.get('label')!r} is checked out to {lk['by']}; only they can check it in (Break Lock overrides).")
        if force and user["role"] == "editor":
            raise CoreError("Breaking a lock is a designer action.")
        w["locks"].pop(str(s))
        done.append(s)
    core.announce({"type": "workflow", "what": "locks"})
    return done


async def _story_text(story: int) -> str:
    r = await core.exec("story.get", {"story": story}, source="guard")
    return str(r.get("text") or "")


async def build_package(aid: str, *, base: dict | None = None, checkout_to_assignee: bool = True) -> tuple[bytes, dict]:
    meta = await active()
    w = meta.workflow
    a = next((x for x in w["assignments"] if x["id"] == aid), None)
    if a is None:
        raise CoreError("no such assignment")
    if not a["stories"]:
        raise CoreError("the assignment has no stories")
    who = a.get("assignee") or "Editor"
    if checkout_to_assignee:
        await checkout(a["stories"], who, aid)
    insp = await _inspect()
    geo = await M.geometry()
    info = {int(s["id"]): s for s in insp.get("stories", [])}
    pkg_base = (base or {})
    files: dict[str, bytes] = {}
    stories, pages = [], {}
    for sid in a["stories"]:
        text = await _story_text(sid)
        b = pkg_base.get(str(sid), text)
        frames = [f for f in (info[sid].get("frames") or []) if f in geo]
        stories.append({"id": sid, "preview": info[sid].get("preview", ""), "frames": frames, "file": f"stories/{sid}.json", "baseHash": hashlib.sha1(b.encode()).hexdigest()})
        files[f"stories/{sid}.json"] = json.dumps({"id": sid, "text": text, "base": b}, ensure_ascii=False, indent=1).encode()
        for f in frames:
            g = geo[f]
            pages.setdefault(g["page"], {"abs": g["page"], "name": g["pageName"], "file": f"previews/page-{g['page'] + 1}.png", "frames": []})["frames"].append({"id": f, "story": sid, "bounds": [g["x"], g["y"], g["x"] + g["w"], g["y"] + g["h"]], "pageSize": [g["pageW"], g["pageH"]]})
    for p in pages.values():
        r = await core.call("page.png", {"page": p["abs"], "maxSide": 900}, source="guard")
        files[p["file"]] = r.bin or b""
    ser = await core.call("serialize", source="guard")
    files["document.fdoc"] = ser.bin or b""
    locks = {str(s): w["locks"][str(s)] for s in a["stories"] if str(s) in w["locks"]}
    st = await core.state()
    manifest = {"format": FORMAT, "version": 1, "created": time.time(), "docId": w["docId"], "title": st["docs"][st["active"]]["title"],
                "assignment": {k: a[k] for k in ("id", "name", "assignee", "color")}, "stories": stories, "pages": sorted(pages.values(), key=lambda p: p["abs"]),
                "workflow": {"assignments": [a], "locks": locks, "docId": w["docId"], "package": {"base": {s["id"]: json.loads(files[s["file"]])["base"] for s in stories}}}}
    files["manifest.json"] = json.dumps(manifest, ensure_ascii=False, indent=1).encode()
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr("manifest.json", files.pop("manifest.json"), compress_type=zipfile.ZIP_DEFLATED)
        for n, d in files.items():
            z.writestr(n, d, compress_type=zipfile.ZIP_STORED if n.endswith((".png", ".fdoc")) else zipfile.ZIP_DEFLATED)
    a["exported"] = time.time()
    core.announce({"type": "workflow", "what": "assignments"})
    return buf.getvalue(), manifest


def read_package(data: bytes) -> tuple[dict, zipfile.ZipFile]:
    try:
        z = zipfile.ZipFile(io.BytesIO(data))
        manifest = json.loads(z.read("manifest.json"))
    except (zipfile.BadZipFile, KeyError, ValueError) as exc:
        raise CoreError("this is not a FormDesign assignment package") from exc
    if manifest.get("format") not in FORMATS:
        raise CoreError("this is not a FormDesign assignment package")
    return manifest, z


async def open_package(data: bytes, name: str = "assignment") -> dict:
    """The editor's side: open the package's document with its stories checked out to the assignee, in the Editor role."""
    manifest, z = read_package(data)
    await core.call("exec", {"command": "file.openBytes", "params": {"name": Path(manifest.get("title") or name).stem + ".fdoc", "base64": base64.b64encode(z.read("document.fdoc")).decode()}}, source="workflow")
    uid = await documents.active_uid()
    m = DocMeta(uid)
    m.workflow = json.loads(json.dumps(manifest["workflow"]))
    documents.meta[uid] = m
    set_user(manifest["assignment"].get("assignee") or "Editor", "editor")
    return await documents.overview()


async def return_package() -> tuple[bytes, dict]:
    meta = await active()
    w = meta.workflow
    if not w.get("package") or not w["assignments"]:
        raise CoreError("this document was not opened from an assignment package")
    return await build_package(w["assignments"][0]["id"], base=w["package"].get("base"), checkout_to_assignee=False)


def _byte_offsets(text: str) -> list[int]:
    out, n = [0], 0
    for ch in text:
        n += len(ch.encode())
        out.append(n)
    return out


async def update_content(data: bytes, force: bool = False, keep_locked: bool = False) -> dict:
    meta = await active()
    w = meta.workflow
    manifest, z = read_package(data)
    if manifest.get("docId") != w["docId"]:
        raise CoreError("this package belongs to another document")
    aid = manifest["assignment"]["id"]
    result = {"updated": [], "unchanged": [], "conflicts": [], "checkedIn": []}
    for s in manifest["stories"]:
        sid = int(s["id"])
        try:
            rec = json.loads(z.read(s["file"]))
        except KeyError:
            continue
        lk = w["locks"].get(str(sid))
        label = (lk or {}).get("label") or s.get("preview", "")[:40]
        edited, base = str(rec.get("text", "")), str(rec.get("base", ""))
        try:
            current = await _story_text(sid)
        except CoreError:
            result["conflicts"].append({"story": sid, "label": label, "why": "the story no longer exists"})
            continue
        if edited == base:
            result["unchanged"].append(sid)
        elif current != base and not force:
            result["conflicts"].append({"story": sid, "label": label, "why": "the layout's text changed after the assignment was exported; use Update Anyway to overwrite it"})
            continue
        else:
            ops = [o for o in difflib.SequenceMatcher(None, current, edited, autojunk=False).get_opcodes() if o[0] != "equal"]
            offs = _byte_offsets(current)
            for tag, i1, i2, j1, j2 in reversed(ops):
                await core.call("exec", {"command": "story.replaceRange", "params": {"story": sid, "start": offs[i1], "end": offs[i2], "text": edited[j1:j2]}}, source="workflow")
            result["updated"].append(sid)
        if not keep_locked and lk and lk.get("assignment") == aid:
            w["locks"].pop(str(sid))
            result["checkedIn"].append(sid)
    core.announce({"type": "workflow", "what": "locks"})
    return result
