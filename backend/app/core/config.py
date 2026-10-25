import os
import sys
from pathlib import Path

APP_DIR = Path(__file__).resolve().parent.parent
BACKEND_DIR = APP_DIR.parent
REPO_ROOT = BACKEND_DIR.parent
FROZEN = getattr(sys, "frozen", False)
BUNDLE_DIR = Path(getattr(sys, "_MEIPASS", APP_DIR))

HOST = os.environ.get("FORMDESIGN_HOST", "127.0.0.1")
PORT = int(os.environ.get("FORMDESIGN_PORT", "8196"))
FRONTEND_PORT = 5196


def _data_dir() -> Path:
    raw = os.environ.get("FORMDESIGN_DATA_DIR", "").strip()
    if raw:
        return Path(raw).expanduser()
    home = Path.home() / "FormDesign"
    _move_legacy_data_dir(Path.home() / "Formdes", home)
    return home


def _move_legacy_data_dir(old: Path, new: Path) -> None:
    try:
        if old.is_dir() and not new.exists():
            old.rename(new)
    except OSError:
        pass


DATA_DIR = _data_dir()
PROJECTS_DIR = DATA_DIR / "Projects"
TEMPLATES_USER_DIR = DATA_DIR / "Templates"
BRANDS_DIR = DATA_DIR / "Brands"
QUERIES_DIR = DATA_DIR / "Queries"
INTAKE_DIR = DATA_DIR / "Intake"
EXPORTS_DIR = DATA_DIR / "Exports"
FONTS_USER_DIR = DATA_DIR / "Fonts"
CACHE_DIR = DATA_DIR / ".cache"

FONTS_DIR = (BUNDLE_DIR / "fonts") if FROZEN else (APP_DIR / "fonts")
TEMPLATES_DIR = (BUNDLE_DIR / "templates" / "library") if FROZEN else (APP_DIR / "templates" / "library")

CORS_ORIGINS = [o.strip() for o in os.environ.get(
    "FORMDESIGN_CORS_ORIGINS", f"http://localhost:{FRONTEND_PORT},http://127.0.0.1:{FRONTEND_PORT}").split(",") if o.strip()]


def window_origins() -> list[str]:
    ports = (FRONTEND_PORT, FRONTEND_PORT + 1)
    dev = [f"http://{name}:{port}" for port in ports for name in ("localhost", "127.0.0.1")]
    return [*CORS_ORIGINS, *dev, "tauri://localhost", "http://tauri.localhost"]


DIST_DIR = Path(os.environ["FORMDESIGN_DIST"]) if os.environ.get("FORMDESIGN_DIST") else (
    (BUNDLE_DIR / "frontend_dist") if FROZEN else REPO_ROOT / "frontend" / "dist")

MAX_UPLOAD_BYTES = 512 * 1024 * 1024
FDES_MIME = "application/vnd.formdesign+zip"
FDES_EXT = ".fdes"


def core_binary() -> Path | None:
    exe = ".exe" if os.name == "nt" else ""
    env = os.environ.get("FORMDESIGN_CORE", "").strip()
    candidates = [Path(env)] if env else []
    if FROZEN:
        candidates.append(Path(sys.executable).resolve().parent / f"formdesign-core{exe}")
    candidates += [REPO_ROOT / "native" / "target" / "release" / f"formdesign-core{exe}",
                   REPO_ROOT / "native" / "bin" / f"formdesign-core{exe}",
                   REPO_ROOT / "native" / "target" / "debug" / f"formdesign-core{exe}"]
    for c in candidates:
        if c.is_file():
            return c
    return None


def font_dirs() -> list[Path]:
    extra = [Path(p) for p in os.environ.get("FORMDESIGN_FONT_DIRS", "").split(os.pathsep) if p.strip()]
    return [d for d in [FONTS_DIR, FONTS_USER_DIR, DATA_DIR / "Cloud Fonts", *extra] if d.is_dir()]


DICTIONARIES_DIR = DATA_DIR / "Dictionaries"


def dictionary_dirs() -> list[Path]:
    extra = [Path(p) for p in os.environ.get("FORMDESIGN_DICT_DIRS", "").split(os.pathsep) if p.strip()]
    return [DICTIONARIES_DIR, *extra]


def ensure_dirs() -> None:
    for d in (DATA_DIR, PROJECTS_DIR, TEMPLATES_USER_DIR, BRANDS_DIR, QUERIES_DIR, INTAKE_DIR, EXPORTS_DIR, FONTS_USER_DIR, CACHE_DIR):
        d.mkdir(parents=True, exist_ok=True)
