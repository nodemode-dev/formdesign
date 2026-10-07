import json
import os
import socket
import sys


def resource_dir() -> str:
    return getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))


def free(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.bind(("127.0.0.1", port))
            return True
        except OSError:
            return False


def selfcheck() -> int:
    info = {"python": sys.version.split()[0]}
    for name in ("PIL", "fastapi", "uvicorn", "pypdfium2", "docx", "openpyxl", "markdown_it"):
        try:
            module = __import__(name)
            info[name.lower()] = getattr(module, "__version__", "ok")
        except Exception as exc:
            info[name.lower()] = f"missing: {exc}"
    from app import __version__
    from app.core import config

    info["formdes"] = __version__
    info["dist_bundled"] = os.path.isdir(os.path.join(resource_dir(), "frontend_dist"))
    info["templates"] = config.TEMPLATES_DIR.is_dir()
    info["fonts"] = config.FONTS_DIR.is_dir()
    print(json.dumps(info))
    return 0


def main() -> int:
    if "--selfcheck" in sys.argv:
        return selfcheck()
    bundled = os.path.join(resource_dir(), "frontend_dist")
    if os.path.isdir(bundled):
        os.environ.setdefault("FORMDES_DIST", bundled)
    os.environ.setdefault("FORMDES_DESKTOP", "1")
    import uvicorn

    from app.main import app as fastapi_app

    wanted = int(os.environ.get("FORMDES_PORT", "8196") or 0)
    port = wanted if wanted and free(wanted) else 0
    uvicorn.run(fastapi_app, host="127.0.0.1", port=port, log_level="info")
    return 0


if __name__ == "__main__":
    import multiprocessing
    multiprocessing.freeze_support()
    sys.exit(main())
