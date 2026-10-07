import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from app import __version__
from app.core import config
from app.engine.core import CoreError, core
from app.routers import core_api, system

logging.basicConfig(level=logging.INFO, format="[formdes] %(levelname)s %(name)s: %(message)s")
log = logging.getLogger("formdes")


@asynccontextmanager
async def lifespan(_: FastAPI):
    config.ensure_dirs()
    try:
        await core.start()
    except CoreError as exc:
        log.error("%s", exc)
    yield
    await core.stop()


app = FastAPI(title="formdes engine", version=__version__, lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=config.CORS_ORIGINS, allow_methods=["*"], allow_headers=["*"])
app.include_router(system.router, prefix="/api")
app.include_router(core_api.router, prefix="/api")


if (config.DIST_DIR / "index.html").is_file():
    app.mount("/assets", StaticFiles(directory=config.DIST_DIR / "assets"), name="assets")

    @app.get("/{path:path}", include_in_schema=False)
    async def spa(path: str):
        target = (config.DIST_DIR / path).resolve()
        if path and target.is_file() and config.DIST_DIR.resolve() in target.parents:
            return FileResponse(target)
        return FileResponse(config.DIST_DIR / "index.html")
