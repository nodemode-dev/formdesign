"""Share for Review: the open document as a page-image site a reviewer can comment on in any browser.

The site is a folder under Published/ so the engine's /sites route serves it (Publish Online's server). Comments are
posted back to /api/review/public/{token}/comments, kept in Reviews/{token}.json, and written into the document as Notes at
the page position (services.notes), so they show up in the Notes panel as they arrive. The engine listens on loopback only
and the public routes accept only same-origin requests whose Host is a loopback name, so a page on another site cannot post.
"""
import json
import re
import secrets
import shutil
import time
import urllib.parse
from html import escape
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services import notes
from app.services.website import PUBLISHED_DIR, SESSION, slugify

REVIEWS_DIR = config.DATA_DIR / "Reviews"
MAX_TEXT, MAX_NAME, MAX_COMMENTS, MAX_REPLIES = 2000, 60, 500, 50
WINDOW, WINDOW_MAX = 60.0, 30
_hits: dict[str, list[float]] = {}


def loopback_host(host: str | None) -> bool:
    h = (host or "").rsplit(":", 1)[0].strip("[]") if host and not host.endswith("]") else (host or "").strip("[]")
    return h in ("127.0.0.1", "localhost", "::1")


def same_origin(origin: str | None, host: str | None) -> bool:
    """A browser page may post only from the site's own origin; scripts without an Origin header (curl, agents) are local anyway."""
    if not loopback_host(host):
        return False
    if not origin:
        return True
    u = urllib.parse.urlparse(origin)
    return u.netloc == host and u.scheme in ("http", "https")


def throttle(token: str) -> bool:
    now = time.time()
    hits = [t for t in _hits.get(token, []) if now - t < WINDOW]
    if len(hits) >= WINDOW_MAX:
        _hits[token] = hits
        return False
    hits.append(now)
    _hits[token] = hits
    return True


def clean(v, limit: int) -> str:
    s = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", str(v or "")).replace("\r\n", "\n").replace("\r", "\n").strip()
    return s[:limit]


class Reviews:
    def _file(self, token: str) -> Path:
        return REVIEWS_DIR / f"{re.sub(r'[^a-z0-9]', '', token.lower())}.json"

    def entries(self) -> list[dict]:
        out = []
        for f in sorted(REVIEWS_DIR.glob("*.json")) if REVIEWS_DIR.is_dir() else []:
            try:
                out.append(json.loads(f.read_text()))
            except (OSError, ValueError):
                continue
        return [r for r in out if isinstance(r, dict) and r.get("token")]

    def get(self, token: str) -> dict | None:
        try:
            r = json.loads(self._file(token).read_text())
        except (OSError, ValueError):
            return None
        return r if isinstance(r, dict) and r.get("token") == token else None

    def save(self, rec: dict) -> None:
        REVIEWS_DIR.mkdir(parents=True, exist_ok=True)
        f = self._file(rec["token"])
        tmp = f.with_suffix(".tmp")
        tmp.write_text(json.dumps(rec, indent=1))
        tmp.replace(f)

    @staticmethod
    def belongs(rec: dict, meta) -> bool:
        if meta is None:
            return False
        if meta.path:
            return rec.get("doc") == meta.path
        return rec.get("session") == SESSION and rec.get("uid") == meta.uid

    async def for_active(self) -> dict | None:
        meta = await documents.meta_for_active()
        return next((r for r in self.entries() if self.belongs(r, meta)), None)

    def site_dir(self, rec: dict) -> Path:
        return PUBLISHED_DIR / slugify(rec["slug"], fallback="review")

    def url(self, base: str, rec: dict) -> str:
        return f"{base.rstrip('/')}/sites/{rec['slug']}/"

    # ----------------------------------------------------------------- sharing

    async def share(self) -> dict:
        meta = await documents.meta_for_active()
        if meta is None:
            raise CoreError("no document open")
        st = await core.state()
        title = (st["docs"][st["active"]].get("title") or "Document").rsplit(".", 1)[0]
        rec = await self.for_active()
        if rec is None:
            token = secrets.token_hex(6)
            rec = {"token": token, "slug": f"{slugify(title, fallback='review')[:40]}-review-{token}", "created": time.time(), "comments": [], "next": 1}
        lay = (await core.call("layout", source="review")).result
        pages = sorted(lay.get("pages") or [], key=lambda p: p["abs"])
        if not pages:
            raise CoreError("the document has no pages to share")
        target = PUBLISHED_DIR / rec["slug"]
        tmp = PUBLISHED_DIR / f"{rec['slug']}.partial"
        shutil.rmtree(tmp, ignore_errors=True)
        (tmp / "pages").mkdir(parents=True)
        sizes = []
        for p in pages:
            r = await core.call("page.png", {"page": p["abs"], "maxSide": 1500, "format": "jpeg", "quality": 84}, source="review")
            (tmp / "pages" / f"p{p['abs'] + 1}.jpg").write_bytes(r.bin or b"")
            sizes.append({"w": r.result.get("width"), "h": r.result.get("height"), "name": p.get("name"), "pt": [p["rect"][2] - p["rect"][0], p["rect"][3] - p["rect"][1]]})
        cfg = {"token": rec["token"], "title": title, "pages": sizes, "built": time.time()}
        (tmp / "review.json").write_text(json.dumps(cfg), encoding="utf-8")
        (tmp / "index.html").write_text(SITE_HTML.replace("{{title}}", escape(title)), encoding="utf-8")
        (tmp / "review.css").write_text(SITE_CSS, encoding="utf-8")
        (tmp / "review.js").write_text(SITE_JS, encoding="utf-8")
        (tmp / "404.html").write_text("<!doctype html><meta charset=utf-8><title>Not found</title><p>This review link is not available.</p>", encoding="utf-8")
        old = PUBLISHED_DIR / f"{rec['slug']}.old"
        shutil.rmtree(old, ignore_errors=True)
        if target.exists():
            target.rename(old)
        tmp.rename(target)
        shutil.rmtree(old, ignore_errors=True)
        rec.update({"title": title, "doc": meta.path or rec.get("doc"), "uid": meta.uid, "session": SESSION, "updated": time.time(), "pages": len(pages), "open": True})
        self.save(rec)
        return rec

    async def stop(self) -> bool:
        rec = await self.for_active()
        if rec is None:
            return False
        shutil.rmtree(PUBLISHED_DIR / rec["slug"], ignore_errors=True)
        rec["open"] = False
        self.save(rec)
        return True

    # ---------------------------------------------------------------- comments

    @staticmethod
    def public(c: dict) -> dict:
        return {"id": c["id"], "page": c["page"], "x": c["x"], "y": c["y"], "name": c["name"], "text": c["text"], "at": c["at"],
                "resolved": bool(c.get("resolved")), "replies": [{"name": r["name"], "text": r["text"], "at": r["at"], "owner": bool(r.get("owner"))} for r in c.get("replies", [])]}

    @staticmethod
    def note_text(c: dict) -> str:
        lines = [f"Review, page {c['page'] + 1}: {c['text']}"]
        for r in c.get("replies", []):
            lines.append(f"Reply, {r['name']}{' (owner)' if r.get('owner') else ''}: {r['text']}")
        lines.append(f"({notes.stamp(c['at'])}{', resolved' if c.get('resolved') else ''})")
        return "\n".join(lines)

    async def _in_document(self, rec: dict, work):
        """Run work() with the shared document active, then put the person's active document back."""
        st = await core.state()
        docs, prev = st.get("docs") or [], st.get("active")
        idx = next((i for i, d in enumerate(docs) if d.get("uid") == rec.get("uid")), None)
        if idx is None:
            return None
        if idx != prev:
            await core.call("docs.activate", {"index": idx}, source="review")
        try:
            return await work()
        finally:
            if idx != prev and prev is not None and prev < len(docs):
                await core.call("docs.activate", {"index": prev}, source="review")

    async def post(self, rec: dict, name: str, text: str, page: int, x: float, y: float) -> dict:
        if len(rec["comments"]) >= MAX_COMMENTS:
            raise CoreError("this review has reached its comment limit")
        if page < 0 or page >= int(rec.get("pages") or 0):
            raise CoreError("no such page")
        c = {"id": rec["next"], "page": page, "x": min(max(x, 0.0), 1.0), "y": min(max(y, 0.0), 1.0), "name": name or "Reviewer", "text": text,
             "at": time.time(), "resolved": False, "replies": [], "note": None, "placed": False}
        rec["next"] += 1

        async def place():
            lay = (await core.call("layout", source="review")).result
            pg = next((p for p in lay.get("pages") or [] if p["abs"] == page), None)
            if pg is None:
                return None
            w, h = pg["rect"][2] - pg["rect"][0], pg["rect"][3] - pg["rect"][1]
            where = await notes.locate(page, c["x"] * w, c["y"] * h)
            if where is None:
                return None
            made = await notes.add_note(where["story"], where["at"], self.note_text(c), c["name"])
            return {"story": made["story"], "id": made["id"]}

        try:
            c["note"] = await self._in_document(rec, place)
            c["placed"] = c["note"] is not None
        except CoreError:
            c["note"] = None
        rec["comments"].append(c)
        self.save(rec)
        return c

    async def refresh_note(self, rec: dict, c: dict) -> None:
        if not c.get("note"):
            return

        async def edit():
            try:
                await core.exec("note.edit", {"story": c["note"]["story"], "id": c["note"]["id"], "text": self.note_text(c)}, source="review")
            except CoreError:
                c["note"] = None
                c["placed"] = False

        await self._in_document(rec, edit)

    async def reply(self, rec: dict, cid: int, name: str, text: str, owner: bool) -> dict:
        c = next((x for x in rec["comments"] if x["id"] == cid), None)
        if c is None:
            raise CoreError(f"no comment {cid}")
        if len(c.setdefault("replies", [])) >= MAX_REPLIES:
            raise CoreError("this thread has reached its reply limit")
        c["replies"].append({"name": name or ("Owner" if owner else "Reviewer"), "text": text, "at": time.time(), "owner": owner})
        await self.refresh_note(rec, c)
        self.save(rec)
        return c

    async def resolve(self, rec: dict, cid: int, resolved: bool) -> dict:
        c = next((x for x in rec["comments"] if x["id"] == cid), None)
        if c is None:
            raise CoreError(f"no comment {cid}")
        c["resolved"] = resolved
        await self.refresh_note(rec, c)
        self.save(rec)
        return c

    async def remove(self, rec: dict, cid: int) -> bool:
        c = next((x for x in rec["comments"] if x["id"] == cid), None)
        if c is None:
            return False
        rec["comments"].remove(c)
        if c.get("note"):
            async def drop():
                try:
                    await core.exec("note.delete", {"story": c["note"]["story"], "id": c["note"]["id"]}, source="review")
                except CoreError:
                    pass
            await self._in_document(rec, drop)
        self.save(rec)
        return True


reviews = Reviews()

SITE_HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self'; style-src 'self'; script-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'">
<meta name="robots" content="noindex"><meta name="color-scheme" content="dark light">
<title>{{title}} - Review</title><link rel="stylesheet" href="review.css"></head>
<body>
<header class="bar"><div class="ttl"><span class="kicker">Review</span><h1 id="title">{{title}}</h1></div>
<div class="who"><label>Your name <input id="who" type="text" maxlength="60" autocomplete="name" spellcheck="false"></label>
<label class="chk"><input id="showres" type="checkbox"> Show resolved</label></div></header>
<p class="hint" id="hint">Click anywhere on a page to leave a comment there.</p>
<div class="layout"><main id="pages" aria-label="Pages"></main><aside id="side" aria-label="Comments"><h2>Comments <span id="count"></span></h2><ol id="list"></ol><p id="none" class="muted">No comments yet.</p></aside></div>
<div id="pop" class="pop" hidden role="dialog" aria-label="Comment"></div>
<script src="review.js"></script></body></html>
"""

SITE_CSS = """
:root{--bg:#161618;--panel:#1f1f23;--line:#34343b;--ink:#ececf0;--muted:#9a9aa6;--accent:#6aa5ff;--pin:#ff7a45;--done:#5b5b66;color-scheme:dark light}
@media (prefers-color-scheme:light){:root{--bg:#eeeef2;--panel:#fff;--line:#d5d5dd;--ink:#1b1b20;--muted:#666672;--accent:#2b6be0;--pin:#e0561f;--done:#a8a8b4}}
*{box-sizing:border-box}html,body{margin:0}body{background:var(--bg);color:var(--ink);font:14px/1.45 system-ui,-apple-system,Segoe UI,Roboto,sans-serif;scrollbar-width:none}
body::-webkit-scrollbar{display:none}
.bar{position:sticky;top:0;z-index:5;display:flex;gap:16px;align-items:center;justify-content:space-between;flex-wrap:wrap;padding:10px 16px;background:var(--panel);border-bottom:1px solid var(--line)}
.kicker{display:block;font-size:11px;letter-spacing:.08em;text-transform:uppercase;color:var(--accent);font-weight:700}h1{margin:0;font-size:17px}
.who{display:flex;gap:14px;align-items:center;flex-wrap:wrap}.who label{display:flex;gap:6px;align-items:center;color:var(--muted)}
input[type=text],textarea{background:var(--bg);color:var(--ink);border:1px solid var(--line);border-radius:6px;padding:6px 8px;font:inherit;width:100%}
.who input[type=text]{width:170px}textarea{resize:none;scrollbar-width:none}textarea::-webkit-scrollbar{display:none}
input:focus-visible,textarea:focus-visible,button:focus-visible{outline:2px solid var(--accent);outline-offset:1px}
.hint{margin:10px 16px 0;color:var(--muted)}
.layout{display:grid;grid-template-columns:minmax(0,1fr) 320px;gap:16px;padding:16px;align-items:start}
@media (max-width:900px){.layout{grid-template-columns:1fr}#side{order:-1}}
#pages{display:flex;flex-direction:column;align-items:center;gap:20px}
.pg{position:relative;width:min(100%,900px);background:#fff;box-shadow:0 2px 18px rgba(0,0,0,.35);cursor:crosshair;line-height:0}
.pg img{width:100%;height:auto;display:block;user-select:none;-webkit-user-drag:none}
.pg .no{position:absolute;left:0;top:-18px;font-size:11px;color:var(--muted);line-height:1}
.pin{position:absolute;width:24px;height:24px;margin:-24px 0 0 -12px;border:0;border-radius:12px 12px 12px 2px;background:var(--pin);color:#fff;font:700 12px/24px system-ui;text-align:center;cursor:pointer;padding:0;transform:rotate(-45deg) scale(.9)}
.pin span{display:block;transform:rotate(45deg)}.pin.done{background:var(--done)}.pin.new{background:var(--accent)}.pin.on{outline:2px solid #fff}
.pop{position:absolute;z-index:8;line-height:1.45;cursor:auto;text-align:left;width:min(300px,calc(100vw - 24px));background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:10px;box-shadow:0 10px 30px rgba(0,0,0,.4)}
.pop .row{display:flex;gap:8px;justify-content:flex-end;margin-top:8px;flex-wrap:wrap}
.pop .meta{font-size:12px;color:var(--muted);margin-bottom:4px}.pop .body{white-space:pre-wrap;overflow-wrap:anywhere}
.pop .rep{margin-top:8px;padding-top:8px;border-top:1px solid var(--line)}
button{font:inherit;cursor:pointer;border:1px solid var(--line);background:var(--panel);color:var(--ink);border-radius:6px;padding:5px 10px}
button.primary{background:var(--accent);border-color:var(--accent);color:#fff}button:disabled{opacity:.5;cursor:default}
#side{position:sticky;top:76px;max-height:calc(100vh - 92px);overflow:auto;scrollbar-width:none;background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:12px}
#side::-webkit-scrollbar{display:none}#side h2{margin:0 0 8px;font-size:14px}#list{list-style:none;margin:0;padding:0;display:grid;gap:8px}
#list li{border:1px solid var(--line);border-radius:6px;padding:8px;cursor:pointer}#list li.done{opacity:.6}#list li.on{border-color:var(--accent)}
.muted{color:var(--muted)}.tag{font-size:11px;color:var(--muted)}
@media (prefers-reduced-motion:reduce){*{transition:none!important}}
"""

SITE_JS = r"""(function(){
'use strict';
var cfg=null,token='',comments=[],open=null,draft=null,pages={},timer=0;
var $=function(id){return document.getElementById(id)};
var who=$('who'),pop=$('pop'),list=$('list');
try{who.value=localStorage.getItem('fd-review-name')||''}catch(e){}
who.addEventListener('change',function(){try{localStorage.setItem('fd-review-name',who.value.trim())}catch(e){}});
$('showres').addEventListener('change',render);
function el(t,c,x){var e=document.createElement(t);if(c)e.className=c;if(x!=null)e.textContent=x;return e}
function api(path,body){
  var o=body?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}:{};
  return fetch('/api/review/public/'+token+path,o).then(function(r){return r.json().catch(function(){return{}}).then(function(j){if(!r.ok)throw new Error(j.detail||'Request failed');return j})});
}
function when(t){try{return new Date(t*1000).toLocaleString([], {dateStyle:'medium',timeStyle:'short'})}catch(e){return''}}
function build(){
  var host=$('pages');
  cfg.pages.forEach(function(p,i){
    var f=el('div','pg');f.dataset.page=i;
    var im=new Image();im.src='pages/p'+(i+1)+'.jpg';im.alt='Page '+(p.name||i+1);im.width=p.w;im.height=p.h;im.draggable=false;f.appendChild(im);
    f.appendChild(el('span','no','Page '+(p.name||i+1)));
    f.addEventListener('click',function(e){
      if(e.target.closest('.pin')||e.target.closest('.pop'))return;
      var r=f.getBoundingClientRect();
      draft={page:i,x:(e.clientX-r.left)/r.width,y:(e.clientY-r.top)/r.height};open=null;render();showDraft();
    });
    host.appendChild(f);pages[i]=f;
  });
}
function clearPins(){document.querySelectorAll('.pin').forEach(function(p){p.remove()})}
function render(){
  clearPins();
  var showRes=$('showres').checked,vis=comments.filter(function(c){return showRes||!c.resolved});
  vis.forEach(function(c,n){
    var f=pages[c.page];if(!f)return;
    var b=el('button','pin'+(c.resolved?' done':'')+(open===c.id?' on':''));b.type='button';b.appendChild(el('span','',String(c.id)));
    b.style.left=(c.x*100)+'%';b.style.top=(c.y*100)+'%';b.setAttribute('aria-label','Comment '+c.id+' by '+c.name);
    b.addEventListener('click',function(e){e.stopPropagation();open=c.id;draft=null;render();showThread(c,b)});
    f.appendChild(b);
  });
  if(draft&&pages[draft.page]){var d=el('button','pin new');d.type='button';d.appendChild(el('span','','+'));d.style.left=(draft.x*100)+'%';d.style.top=(draft.y*100)+'%';pages[draft.page].appendChild(d)}
  list.textContent='';
  vis.forEach(function(c){
    var li=el('li',(c.resolved?'done ':'')+(open===c.id?'on':''));
    li.appendChild(el('div','tag','#'+c.id+' · page '+(cfg.pages[c.page].name||c.page+1)+' · '+c.name));
    li.appendChild(el('div','',c.text.length>140?c.text.slice(0,139)+'…':c.text));
    if(c.replies.length)li.appendChild(el('div','tag',c.replies.length+(c.replies.length===1?' reply':' replies')));
    li.addEventListener('click',function(){open=c.id;draft=null;render();var pin=pages[c.page].querySelector('.pin.on');if(pin){pin.scrollIntoView({block:'center'});showThread(c,pin)}});
    list.appendChild(li);
  });
  $('none').hidden=vis.length>0;$('count').textContent=vis.length?'('+vis.length+')':'';
}
function place(anchor){
  var host=anchor.closest('.pg'),hr=host.getBoundingClientRect(),ar=anchor.getBoundingClientRect();
  pop.hidden=false;
  var pw=pop.offsetWidth,left=ar.left-hr.left+16,top=ar.top-hr.top;
  if(left+pw>hr.width)left=Math.max(4,ar.left-hr.left-pw-8);
  if(left<4)left=4;
  host.appendChild(pop);pop.style.left=left+'px';pop.style.top=Math.max(0,top)+'px';
}
function closePop(){pop.hidden=true;document.body.appendChild(pop)}
function showDraft(){
  var anchor=pages[draft.page].querySelector('.pin.new');pop.textContent='';
  var ta=el('textarea');ta.rows=3;ta.maxLength=2000;ta.placeholder='Write a comment';ta.setAttribute('aria-label','Comment');
  var row=el('div','row'),cancel=el('button','','Cancel'),post=el('button','primary','Post');cancel.type=post.type='button';
  cancel.addEventListener('click',function(){draft=null;closePop();render()});
  post.addEventListener('click',function(){
    var name=who.value.trim();
    if(!name){who.focus();$('hint').textContent='Enter your name first, so the author knows who commented.';return}
    if(!ta.value.trim())return;
    post.disabled=true;
    api('/comments',{name:name,text:ta.value,page:draft.page,x:draft.x,y:draft.y}).then(function(c){draft=null;closePop();comments.push(c);open=c.id;render();$('hint').textContent='Comment posted.'}).catch(function(e){post.disabled=false;$('hint').textContent=e.message});
  });
  row.append(cancel,post);pop.append(ta,row);place(anchor);ta.focus();
}
function showThread(c,anchor){
  pop.textContent='';
  pop.appendChild(el('div','meta','#'+c.id+' · '+c.name+' · '+when(c.at)+(c.resolved?' · resolved':'')));
  pop.appendChild(el('div','body',c.text));
  c.replies.forEach(function(r){var d=el('div','rep');d.appendChild(el('div','meta',r.name+(r.owner?' (author)':'')+' · '+when(r.at)));d.appendChild(el('div','body',r.text));pop.appendChild(d)});
  var ta=el('textarea');ta.rows=2;ta.maxLength=2000;ta.placeholder='Reply';ta.setAttribute('aria-label','Reply');ta.style.marginTop='8px';pop.appendChild(ta);
  var row=el('div','row'),close=el('button','','Close'),send=el('button','primary','Reply');close.type=send.type='button';
  close.addEventListener('click',function(){open=null;closePop();render()});
  send.addEventListener('click',function(){
    var name=who.value.trim();if(!name){who.focus();return}if(!ta.value.trim())return;send.disabled=true;
    api('/comments/'+c.id+'/replies',{name:name,text:ta.value}).then(function(u){merge(u);render();var a=pages[u.page].querySelector('.pin.on');if(a)showThread(u,a)}).catch(function(e){send.disabled=false;$('hint').textContent=e.message});
  });
  row.append(close,send);pop.appendChild(row);place(anchor);
}
function merge(u){var i=comments.findIndex(function(c){return c.id===u.id});if(i<0)comments.push(u);else comments[i]=u}
function same(a,b){return JSON.stringify(a)===JSON.stringify(b)}
function poll(){
  api('/comments').then(function(r){
    if(!draft&&!same(r.comments,comments)){comments=r.comments;render()}
  }).catch(function(){});
}
document.addEventListener('keydown',function(e){if(e.key==='Escape'){draft=null;open=null;closePop();render()}});
fetch('review.json',{cache:'no-store'}).then(function(r){return r.json()}).then(function(c){
  cfg=c;token=c.token;document.title=c.title+' - Review';build();return api('/comments')
}).then(function(r){comments=r.comments;render();timer=window.setInterval(poll,4000)}).catch(function(e){$('hint').textContent='This review could not be loaded: '+e.message});
})();
"""
