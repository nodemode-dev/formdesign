"""Plug-ins: folders or .fdplugin zips with a manifest.json, a catalogue to browse them, and a broker for what they may do.

A plug-in runs in a sandboxed iframe in the window and can only post messages. The window's broker checks each request against the
permissions the person agreed to at install and sends it here; this module checks the same grant again (the window is not trusted
with it) and runs the request as the MCP client `plugin:<id>`, so Agent Connect's policy and approvals apply to a plug-in exactly as
they do to an agent. Network access is a host allow-list; the plug-in has no network of its own.
"""
import hashlib
import io
import json
import re
import shutil
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

from app import __version__
from app.core import config

PLUGINS_DIR = config.DATA_DIR / "Plug-ins"
STATE = PLUGINS_DIR / "state.json"
STAGE = config.CACHE_DIR / "plugin-stage"
REPO_PLUGINS = Path(__import__("os").environ.get("FORMDES_PLUGINS") or (config.BUNDLE_DIR / "plugins" if config.FROZEN else config.REPO_ROOT / "plugins"))
DEFAULT_CATALOGUE = "/api/plugins/repo/catalogue.json"
LOCAL_PREFIX = "/api/plugins/repo/"
ID = re.compile(r"^[a-z0-9][a-z0-9._-]{1,63}$")
HOST = re.compile(r"^[a-z0-9]([a-z0-9.-]{0,251}[a-z0-9])?$")
MAX_ZIP = 8 * 1024 * 1024
MAX_FILES = 200
PERMISSIONS = {
    "commands": "Add commands the person can run",
    "document.read": "Read the open document (items, text, styles, page images)",
    "document.write": "Change the open document",
    "panel": "Show a panel in the window",
    "menu": "Add items under Window > Plug-ins",
}
READ_TOOLS = {"query", "outline", "inspect", "look", "get_document", "get_slots", "list_documents", "preview", "find_commands", "preflight"}
WRITE_TOOLS = {"set_slots", "run_commands", "undo", "redo", "show"}


class PluginError(ValueError):
    pass


def _read(path: Path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def _write(path: Path, data) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=1))


def _state() -> dict:
    s = _read(STATE, {})
    s.setdefault("plugins", {})
    s.setdefault("catalogue", DEFAULT_CATALOGUE)
    return s


def _ver(v: str) -> tuple:
    return tuple(int(x) for x in re.findall(r"\d+", v)[:3]) + (0,) * (3 - len(re.findall(r"\d+", v)[:3]))


def check_manifest(m) -> dict:
    if not isinstance(m, dict):
        raise PluginError("manifest.json must be an object")
    out = {}
    for k in ("id", "name", "version", "author", "description", "entry"):
        if not isinstance(m.get(k), str) or not m[k].strip():
            raise PluginError(f"manifest.json needs a {k}")
        out[k] = m[k].strip()
    if not ID.match(out["id"]):
        raise PluginError("id is lowercase letters, digits, dots, dashes and underscores (2 to 64 characters)")
    if ".." in out["entry"] or out["entry"].startswith(("/", "\\")) or not out["entry"].endswith(".js"):
        raise PluginError("entry is a .js file inside the plug-in")
    out["minAppVersion"] = str(m.get("minAppVersion") or "0.0.0")
    if _ver(out["minAppVersion"]) > _ver(__version__):
        raise PluginError(f"{out['name']} needs FormDesign {out['minAppVersion']} or newer (this is {__version__})")
    perms = m.get("permissions") or {}
    if not isinstance(perms, dict):
        raise PluginError("permissions is an object")
    clean = {}
    for k in PERMISSIONS:
        if perms.get(k):
            clean[k] = True
    hosts = perms.get("network") or []
    if not isinstance(hosts, list) or not all(isinstance(h, str) and HOST.match(h.lower()) for h in hosts):
        raise PluginError("permissions.network is a list of host names")
    if hosts:
        clean["network"] = [h.lower() for h in hosts]
    unknown = set(perms) - set(PERMISSIONS) - {"network"}
    if unknown:
        raise PluginError("unknown permission: " + ", ".join(sorted(unknown)))
    out["permissions"] = clean
    con = m.get("contributes") or {}
    cmds = [{"id": str(c["id"]), "label": str(c["label"])} for c in con.get("commands") or [] if isinstance(c, dict) and c.get("id") and c.get("label")]
    if cmds and "commands" not in clean:
        raise PluginError("contributes.commands needs the commands permission")
    if con.get("panel") and "panel" not in clean:
        raise PluginError("contributes.panel needs the panel permission")
    menu = [str(x) for x in con.get("menu") or []]
    if menu and "menu" not in clean:
        raise PluginError("contributes.menu needs the menu permission")
    for x in menu:
        if x not in {c["id"] for c in cmds}:
            raise PluginError(f"contributes.menu names {x}, which is not a command")
    out["contributes"] = {"commands": cmds, "panel": {"title": str((con.get("panel") or {}).get("title") or out["name"])} if con.get("panel") else None, "menu": menu}
    return out


def _safe_extract(data: bytes, dest: Path) -> None:
    if len(data) > MAX_ZIP:
        raise PluginError("that plug-in file is too large")
    try:
        z = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise PluginError("not a .fdplugin (zip) file") from exc
    infos = [i for i in z.infolist() if not i.is_dir()]
    if len(infos) > MAX_FILES or sum(i.file_size for i in infos) > 4 * MAX_ZIP:
        raise PluginError("that plug-in has too many or too large files")
    names = [i.filename for i in infos]
    root = ""
    if "manifest.json" not in names:
        tops = {n.split("/")[0] for n in names}
        if len(tops) == 1 and f"{next(iter(tops))}/manifest.json" in names:
            root = next(iter(tops)) + "/"
        else:
            raise PluginError("manifest.json is missing")
    for i in infos:
        rel = i.filename[len(root):]
        if not rel or rel.startswith(("/", "\\")) or ".." in Path(rel).parts or ":" in rel:
            raise PluginError(f"unsafe path in the plug-in: {i.filename}")
        out = dest / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(z.read(i))


def _fetch(url: str, limit: int = MAX_ZIP) -> bytes:
    if url.startswith(LOCAL_PREFIX):
        rel = urllib.parse.unquote(url[len(LOCAL_PREFIX):].split("?")[0])
        path = (REPO_PLUGINS / rel).resolve()
        if REPO_PLUGINS.resolve() not in path.parents or not path.is_file():
            raise PluginError("not found in the bundled plug-ins")
        return path.read_bytes()
    if not url.startswith("https://"):
        raise PluginError("catalogue and download addresses are https or the bundled catalogue")
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "formdes"}), timeout=60) as r:
            data = r.read(limit + 1)
    except OSError as exc:
        raise PluginError(f"could not fetch {url}: {exc}") from exc
    if len(data) > limit:
        raise PluginError("that download is too large")
    return data


def catalogue_url() -> str:
    return _state()["catalogue"]


def set_catalogue(url: str) -> str:
    url = (url or "").strip() or DEFAULT_CATALOGUE
    if not (url.startswith("https://") or url.startswith(LOCAL_PREFIX)):
        raise PluginError("the catalogue address is https")
    s = _state()
    s["catalogue"] = url
    _write(STATE, s)
    return url


def catalogue() -> dict:
    url = catalogue_url()
    try:
        data = json.loads(_fetch(url, 2 * 1024 * 1024))
    except ValueError as exc:
        raise PluginError("the catalogue is not valid JSON") from exc
    items = []
    inst = _state()["plugins"]
    for e in (data.get("plugins") if isinstance(data, dict) else None) or []:
        if not isinstance(e, dict) or not e.get("id") or not e.get("download") or not re.fullmatch(r"[0-9a-f]{64}", str(e.get("sha256") or "")):
            continue
        items.append({k: e.get(k) for k in ("id", "name", "version", "author", "description", "permissions", "download", "sha256", "minAppVersion")} |
                     {"installed": e["id"] in inst, "installedVersion": (inst.get(e["id"]) or {}).get("version")})
    return {"url": url, "name": (data.get("name") if isinstance(data, dict) else None) or "Catalogue", "plugins": items}


def stage_catalogue(pid: str) -> dict:
    cat = catalogue()
    e = next((x for x in cat["plugins"] if x["id"] == pid), None)
    if e is None:
        raise PluginError(f"{pid} is not in the catalogue")
    url = urllib.parse.urljoin(cat["url"], e["download"]) if not e["download"].startswith(LOCAL_PREFIX) else e["download"]
    if not url.startswith("https://") and not url.startswith(LOCAL_PREFIX):
        raise PluginError("a download is https or the bundled catalogue")
    data = _fetch(url)
    got = hashlib.sha256(data).hexdigest()
    if got != e["sha256"]:
        raise PluginError(f"the download does not match the catalogue's sha256 (expected {e['sha256'][:12]}, got {got[:12]}); it was not installed")
    return stage_bytes(data, "catalogue", e["sha256"])


def stage_bytes(data: bytes, source: str, digest: str = "") -> dict:
    sid = hashlib.sha256(data).hexdigest()[:16]
    dest = STAGE / sid
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True, exist_ok=True)
    try:
        _safe_extract(data, dest)
        m = check_manifest(_read(dest / "manifest.json", None))
        if not (dest / m["entry"]).is_file():
            raise PluginError(f"the entry file {m['entry']} is missing")
    except PluginError:
        shutil.rmtree(dest, ignore_errors=True)
        raise
    return {"stage": sid, "manifest": m, "source": source, "sha256": digest or hashlib.sha256(data).hexdigest(), "installed": m["id"] in _state()["plugins"]}


def stage_path(path: str) -> dict:
    p = Path(path).expanduser()
    if p.is_dir():
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            for f in sorted(p.rglob("*")):
                if f.is_file():
                    z.write(f, f.relative_to(p).as_posix())
        return stage_bytes(buf.getvalue(), "folder")
    if p.is_file():
        return stage_bytes(p.read_bytes(), "file")
    raise PluginError(f"{path} was not found")


def install(stage: str, consent: bool) -> dict:
    if not consent:
        raise PluginError("a plug-in is installed only after the person agrees to its permissions")
    if not re.fullmatch(r"[0-9a-f]{16}", stage or ""):
        raise PluginError("unknown stage")
    src = STAGE / stage
    m = check_manifest(_read(src / "manifest.json", None))
    dest = PLUGINS_DIR / m["id"]
    shutil.rmtree(dest, ignore_errors=True)
    shutil.copytree(src, dest)
    shutil.rmtree(src, ignore_errors=True)
    s = _state()
    s["plugins"][m["id"]] = {"enabled": True, "granted": m["permissions"], "version": m["version"]}
    _write(STATE, s)
    return _row(m["id"], s["plugins"][m["id"]])


def _manifest(pid: str) -> dict:
    return check_manifest(_read(PLUGINS_DIR / pid / "manifest.json", None))


def _row(pid: str, st: dict) -> dict:
    try:
        m = _manifest(pid)
    except PluginError as exc:
        return {"id": pid, "name": pid, "error": str(exc), "enabled": False, "granted": {}}
    return {**m, "enabled": bool(st.get("enabled")), "granted": st.get("granted") or {}}


def listing() -> dict:
    s = _state()
    return {"plugins": [_row(pid, st) for pid, st in sorted(s["plugins"].items())], "catalogue": s["catalogue"], "defaultCatalogue": DEFAULT_CATALOGUE,
            "permissions": PERMISSIONS, "folder": str(PLUGINS_DIR)}


def set_enabled(pid: str, on: bool) -> dict:
    s = _state()
    if pid not in s["plugins"]:
        raise PluginError("that plug-in is not installed")
    s["plugins"][pid]["enabled"] = bool(on)
    _write(STATE, s)
    return _row(pid, s["plugins"][pid])


def uninstall(pid: str) -> None:
    s = _state()
    if pid not in s["plugins"]:
        raise PluginError("that plug-in is not installed")
    shutil.rmtree(PLUGINS_DIR / pid, ignore_errors=True)
    s["plugins"].pop(pid)
    _write(STATE, s)


def entry_source(pid: str) -> str:
    st = _state()["plugins"].get(pid)
    if not st or not st.get("enabled"):
        raise PluginError("that plug-in is not enabled")
    m = _manifest(pid)
    return (PLUGINS_DIR / pid / m["entry"]).read_text(encoding="utf-8")


def grant(pid: str) -> dict:
    st = _state()["plugins"].get(pid)
    if not st or not st.get("enabled"):
        raise PluginError("that plug-in is not installed and enabled")
    return st.get("granted") or {}


def allowed_tool(pid: str, tool: str) -> None:
    g = grant(pid)
    if tool in READ_TOOLS:
        need = "document.read"
    elif tool in WRITE_TOOLS:
        need = "document.write"
    else:
        raise PluginError(f"plug-ins cannot use the {tool} tool")
    if not g.get(need):
        raise PluginError(f"{pid} was not given the {need} permission")


def allowed_fetch(pid: str, url: str) -> str:
    g = grant(pid)
    u = urllib.parse.urlparse(url)
    if u.scheme != "https" or not u.hostname or u.hostname.lower() not in (g.get("network") or []):
        raise PluginError(f"{pid} may not reach {u.hostname or url}; it declared {', '.join(g.get('network') or []) or 'no network hosts'}")
    return url


def fetch(pid: str, url: str) -> str:
    allowed_fetch(pid, url)
    return _fetch(url, 2 * 1024 * 1024).decode("utf-8", "replace")
