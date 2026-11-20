import json
import logging
import os
from logging.handlers import RotatingFileHandler
from pathlib import Path

from app.core import startup

LOG_NAME = "formdesign-engine.log"
LOG_FOLDER = "Logs"
LOG_BYTES = 1_048_576
LOG_KEEP = 3
HANDLER = "formdesign-file"
LOGGERS = ("formdesign", "uvicorn", "uvicorn.access")
FORMAT = "%(asctime)s %(levelname)s %(name)s %(message)s"


AlreadyRunning = startup.Busy


def start_logging(data: Path) -> Path:
    folder = data / LOG_FOLDER
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / LOG_NAME
    for held in logging.getLogger("formdesign").handlers:
        if held.get_name() == HANDLER and getattr(held, "baseFilename", "") == os.path.abspath(path):
            for name in LOGGERS:
                if held not in logging.getLogger(name).handlers:
                    logging.getLogger(name).addHandler(held)
            return path
    stop_logging()
    handler = RotatingFileHandler(path, maxBytes=LOG_BYTES, backupCount=LOG_KEEP, encoding="utf-8")
    handler.setFormatter(logging.Formatter(FORMAT))
    handler.set_name(HANDLER)
    for name in LOGGERS:
        logger = logging.getLogger(name)
        logger.addHandler(handler)
        if name == "formdesign":
            logger.setLevel(logging.INFO)
    logging.getLogger("formdesign").info("engine started, pid %s, data folder %s", os.getpid(), data)
    return path


def stop_logging() -> None:
    for name in LOGGERS:
        logger = logging.getLogger(name)
        for handler in list(logger.handlers):
            if handler.get_name() == HANDLER:
                logger.removeHandler(handler)
                handler.close()


def claim(data: Path) -> Path:
    lock, replaced = startup.claim(data, startup.shell_pid())
    if replaced:
        logging.getLogger("formdesign").warning("an engine left running without its app was stopped (process %s)", replaced)
    return lock


def release(data: Path) -> None:
    path = data / startup.LOCK_NAME
    try:
        if int(json.loads(path.read_text(encoding="utf-8")).get("pid", 0)) == os.getpid():
            path.unlink()
    except (OSError, ValueError, TypeError, AttributeError):
        pass
