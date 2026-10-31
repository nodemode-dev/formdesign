import logging
import mimetypes
from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from app import __version__
from app.core import config
from app.core.local_guard import LocalGuard
from app.engine.core import CoreError, core
from app.engine.documents import documents
from app.services import userprefs as userprefs_service
from app.services import workflow as workflow_service
from app.routers import agents as agents_api
from app.routers import colour as colour_api
from app.routers import motion as motion_api
from app.routers import workflow as workflow_api
from app.routers import assistant as assistant_api
from app.routers import cloudfonts as cloudfonts_api
from app.routers import plugins as plugins_api
from app.routers import claude as claude_api
from app.routers import commands as commands_api
from app.routers import actionsets as actionsets_api
from app.routers import books, core_api, dictionaries, documents as documents_api, find, intake, libraries, make, mcp, preflight, review, system, templates, terminal, userprefs, versions as versions_api, website
from app.services.pty import terminals

mimetypes.add_type("text/javascript", ".js")
mimetypes.add_type("text/javascript", ".mjs")
mimetypes.add_type("text/css", ".css")

logging.basicConfig(level=logging.INFO, format="[formdesign] %(levelname)s %(name)s: %(message)s")
log = logging.getLogger("formdesign")


@asynccontextmanager
async def lifespan(_: FastAPI):
    config.ensure_dirs()
    try:
        await core.start()
        workflow_service.load_user()
        core.guard = workflow_service.guard
        await userprefs_service.load_profiles()
        documents.start_autosave()
    except CoreError as exc:
        log.error("%s", exc)
    yield
    terminals.close_all()
    await core.stop()


app = FastAPI(title="FormDesign engine", version=__version__, lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=config.CORS_ORIGINS, allow_methods=["*"], allow_headers=["*"])
app.add_middleware(LocalGuard, origins=config.window_origins())
app.include_router(system.router, prefix="/api")
app.include_router(core_api.router, prefix="/api")
app.include_router(versions_api.router, prefix="/api")
app.include_router(documents_api.router, prefix="/api")
app.include_router(intake.router, prefix="/api")
app.include_router(templates.router, prefix="/api")
app.include_router(make.router, prefix="/api")
app.include_router(books.router, prefix="/api")
app.include_router(find.router, prefix="/api")
app.include_router(preflight.router, prefix="/api")
app.include_router(libraries.router, prefix="/api")
app.include_router(userprefs.router, prefix="/api")
app.include_router(dictionaries.router, prefix="/api")
app.include_router(website.router, prefix="/api")
app.include_router(terminal.router, prefix="/api")
app.include_router(agents_api.router, prefix="/api")
app.include_router(colour_api.router, prefix="/api")
app.include_router(assistant_api.router, prefix="/api")
app.include_router(review.router, prefix="/api")
app.include_router(motion_api.router, prefix="/api")
app.include_router(workflow_api.router, prefix="/api")
app.include_router(cloudfonts_api.router, prefix="/api")
app.include_router(plugins_api.router, prefix="/api")
app.include_router(claude_api.router, prefix="/api")
app.include_router(commands_api.router, prefix="/api")
app.include_router(actionsets_api.router, prefix="/api")
app.include_router(website.sites)
app.include_router(mcp.router)


if (config.DIST_DIR / "index.html").is_file():
    app.mount("/assets", StaticFiles(directory=config.DIST_DIR / "assets"), name="assets")

    @app.get("/{path:path}", include_in_schema=False)
    async def spa(path: str):
        target = (config.DIST_DIR / path).resolve()
        if path and target.is_file() and config.DIST_DIR.resolve() in target.parents:
            return FileResponse(target)
        return FileResponse(config.DIST_DIR / "index.html")
