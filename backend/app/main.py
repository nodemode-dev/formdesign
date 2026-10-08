import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from app import __version__
from app.core import config
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services import userprefs as userprefs_service
from app.routers import books, core_api, documents as documents_api, find, intake, libraries, make, mcp, preflight, reader, system, templates, userprefs

logging.basicConfig(level=logging.INFO, format="[formdes] %(levelname)s %(name)s: %(message)s")
log = logging.getLogger("formdes")


@asynccontextmanager
async def lifespan(_: FastAPI):
    config.ensure_dirs()
    try:
        await core.start()
        await userprefs_service.load_profiles()
        documents.start_autosave()
    except CoreError as exc:
        log.error("%s", exc)
    yield
    await core.stop()


app = FastAPI(title="formdes engine", version=__version__, lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=config.CORS_ORIGINS, allow_methods=["*"], allow_headers=["*"])
app.include_router(system.router, prefix="/api")
app.include_router(core_api.router, prefix="/api")
app.include_router(documents_api.router, prefix="/api")
app.include_router(intake.router, prefix="/api")
app.include_router(reader.router, prefix="/api")
app.include_router(templates.router, prefix="/api")
app.include_router(make.router, prefix="/api")
app.include_router(books.router, prefix="/api")
app.include_router(find.router, prefix="/api")
app.include_router(preflight.router, prefix="/api")
app.include_router(libraries.router, prefix="/api")
app.include_router(userprefs.router, prefix="/api")
app.include_router(mcp.router)


if (config.DIST_DIR / "index.html").is_file():
    app.mount("/assets", StaticFiles(directory=config.DIST_DIR / "assets"), name="assets")

    @app.get("/{path:path}", include_in_schema=False)
    async def spa(path: str):
        target = (config.DIST_DIR / path).resolve()
        if path and target.is_file() and config.DIST_DIR.resolve() in target.parents:
            return FileResponse(target)
        return FileResponse(config.DIST_DIR / "index.html")
