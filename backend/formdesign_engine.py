import os
import sys

if sys.stdin is None:
    sys.stdin = open(os.devnull, encoding="utf-8")
if sys.stdout is None:
    sys.stdout = open(os.devnull, "w", encoding="utf-8")
if sys.stderr is None:
    sys.stderr = open(os.devnull, "w", encoding="utf-8")

import json
import logging
import socket
from pathlib import Path


def resource_dir() -> str:
    return getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))


def free(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        try:
            s.bind(("127.0.0.1", port))
            return True
        except OSError:
            return False


def spare() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


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

    info["formdesign"] = __version__
    info["dist_bundled"] = os.path.isdir(os.path.join(resource_dir(), "frontend_dist"))
    info["templates"] = config.TEMPLATES_DIR.is_dir()
    info["fonts"] = config.FONTS_DIR.is_dir()
    print(json.dumps(info))
    return 0


def watch_shell(data) -> None:
    from app.core import runtime, startup

    shell = startup.shell_pid()
    if shell <= 1:
        return

    def leave() -> None:
        logging.getLogger("formdesign").info("the app that started this engine is gone (process %s), stopping", shell)
        runtime.release(data)
        os._exit(0)

    startup.watch(shell, leave)


def serve() -> int:
    bundled = os.path.join(resource_dir(), "frontend_dist")
    if os.path.isdir(bundled):
        os.environ.setdefault("FORMDESIGN_DIST", bundled)
    os.environ.setdefault("FORMDESIGN_DESKTOP", "1")
    wanted = int(os.environ.get("FORMDESIGN_PORT", "8196") or 0)
    port = wanted if wanted and free(wanted) else spare()
    os.environ["FORMDESIGN_PORT"] = str(port)
    from app.core import config, runtime, startup

    data = config.DATA_DIR
    try:
        logs = runtime.start_logging(data).parent
    except OSError as err:
        print(f"formdesign cannot use its data folder: {err}", file=sys.stderr)
        return 5
    startup.keep_errors(logs)
    log = logging.getLogger("formdesign")
    if startup.alone():
        beside = startup.app_beside(Path(sys.executable))
        if beside is not None and startup.open_app(beside):
            log.info("started without its app, so it opened %s and stopped", beside)
            return 0
        log.error("started without its app, and no app program was found near %s", sys.executable)
        print("formdesign-engine is a part of FormDesign and serves nothing on its own. Open FormDesign instead.", file=sys.stderr)
        return 4
    try:
        runtime.claim(data)
    except runtime.AlreadyRunning as err:
        log.error("%s", err)
        print(err, file=sys.stderr)
        return 2
    watch_shell(data)
    import uvicorn

    from app.main import app as fastapi_app
    from app.core import window_key

    if window_key.stdout_mode():
        print(window_key.LINE + window_key.KEY, flush=True)
    import signal

    def leave(_signum, _frame) -> None:
        runtime.release(data)
        os._exit(0)

    signal.signal(signal.SIGTERM, leave)
    server = uvicorn.Server(uvicorn.Config(fastapi_app, host="127.0.0.1", port=port, log_level="info"))
    runtime.start_logging(data)
    try:
        server.run()
    except KeyboardInterrupt:
        pass
    finally:
        runtime.release(data)
    return 0 if server.started else 3


def main() -> int:
    if "--selfcheck" in sys.argv:
        return selfcheck()
    try:
        return serve()
    except Exception:
        logging.getLogger("formdesign").exception("the engine stopped on an error")
        raise


if __name__ == "__main__":
    import multiprocessing
    multiprocessing.freeze_support()
    sys.exit(main())
