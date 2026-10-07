import json
import secrets
import time
from pathlib import Path

from app.core import config


def new_pack_id() -> str:
    return time.strftime("pk%Y%m%d%H%M%S") + secrets.token_hex(3)


class PackStore:
    def __init__(self) -> None:
        self.cache: dict[str, dict] = {}

    def dir(self, pack_id: str) -> Path:
        safe = "".join(c for c in pack_id if c.isalnum())
        return config.INTAKE_DIR / safe

    def put(self, pack: dict) -> dict:
        d = self.dir(pack["id"])
        d.mkdir(parents=True, exist_ok=True)
        (d / "pack.json").write_text(json.dumps(pack, indent=1))
        self.cache[pack["id"]] = pack
        return pack

    def get(self, pack_id: str | None) -> dict | None:
        if not pack_id:
            return None
        if pack_id in self.cache:
            return self.cache[pack_id]
        f = self.dir(pack_id) / "pack.json"
        if f.is_file():
            try:
                pack = json.loads(f.read_text())
            except ValueError:
                return None
            self.cache[pack_id] = pack
            return pack
        return None

    def listing(self, limit: int = 30) -> list[dict]:
        out = []
        if not config.INTAKE_DIR.is_dir():
            return out
        for d in sorted((p for p in config.INTAKE_DIR.iterdir() if (p / "pack.json").is_file()), key=lambda p: p.stat().st_mtime, reverse=True)[:limit]:
            pack = self.get(d.name)
            if pack:
                out.append({"id": pack["id"], "title": pack.get("title"), "created": pack.get("created"),
                            "words": pack.get("facts", {}).get("words", 0), "images": len(pack.get("images", []))})
        return out


packs = PackStore()
