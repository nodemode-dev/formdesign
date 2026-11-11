import platform

from fastapi import APIRouter

from app import __version__
from app.core import config
from app.engine.core import core

router = APIRouter()


@router.get("/version")
async def version():
    return {"app": "formdesign", "version": __version__, "core": core.version, "coreRunning": core.alive()}


@router.get("/health")
async def health():
    return {"ok": core.alive(), "core": core.version, "restarts": core.restarts}


@router.get("/capabilities")
async def capabilities():
    return {
        "version": __version__,
        "platform": platform.system().lower(),
        "core": {"running": core.alive(), "version": core.version, "binary": str(core.binary) if core.binary else None},
        "dataDir": str(config.DATA_DIR),
        "paths": {"projects": str(config.PROJECTS_DIR), "exports": str(config.EXPORTS_DIR), "templates": str(config.TEMPLATES_USER_DIR),
                  "brands": str(config.BRANDS_DIR), "intake": str(config.INTAKE_DIR)},
        "fontDirs": [str(d) for d in config.font_dirs()],
        "mcp": f"http://127.0.0.1:{config.PORT}/mcp",
    }
