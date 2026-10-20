import asyncio
import codecs
import re
import urllib.request
from pathlib import Path

from app.core import config
from app.engine.core import CoreError, core

SOURCE = "https://raw.githubusercontent.com/LibreOffice/dictionaries/master/"
BROWSE = "https://github.com/LibreOffice/dictionaries/tree/master/"
CATALOG = {
    "en_GB": "en/en_GB", "en_CA": "en/en_CA", "de_DE": "de/de_DE_frami", "de_CH": "de/de_CH_frami", "fr_FR": "fr_FR/dictionaries/fr",
    "es_ES": "es/es_ES", "it_IT": "it_IT/it_IT", "pt_PT": "pt_PT/pt_PT", "pt_BR": "pt_BR/pt_BR", "nl_NL": "nl_NL/nl_NL",
    "da_DK": "da_DK/da_DK", "sv_SE": "sv_SE/dictionaries/sv_SE", "nb_NO": "no/nb_NO", "pl_PL": "pl_PL/pl_PL", "cs_CZ": "cs_CZ/cs_CZ",
    "hu_HU": "hu_HU/hu_HU", "el_GR": "el_GR/el_GR", "ru_RU": "ru_RU/ru_RU", "uk_UA": "uk_UA/uk_UA", "tr_TR": "tr_TR/tr_TR",
}
CODE = re.compile(r"^[a-z]{2,3}(_[A-Z]{2})?$")
ALIASES = {"microsoft-cp1251": "cp1251", "microsoft-cp1250": "cp1250", "tis620-2533": "tis_620"}


class DictionaryError(ValueError):
    pass


def folder() -> Path:
    return config.DICTIONARIES_DIR


def to_utf8(aff: bytes, dic: bytes) -> tuple[str, str]:
    m = re.search(rb"^\s*SET\s+(\S+)", aff, re.M)
    enc = m.group(1).decode("ascii", "ignore").lower() if m else "utf-8"
    enc = ALIASES.get(enc, enc)
    try:
        codecs.lookup(enc)
    except LookupError:
        enc = "utf-8"
    a = aff.decode(enc, errors="replace").lstrip("﻿")
    d = dic.decode(enc, errors="replace").lstrip("﻿")
    a = re.sub(r"^\s*SET\s+\S+", "SET UTF-8", a, count=1, flags=re.M) if m else "SET UTF-8\n" + a
    return a, d


def _write(code: str, aff: bytes, dic: bytes) -> None:
    if not CODE.match(code or ""):
        raise DictionaryError(f"{code} is not a language code such as de_DE.")
    a, d = to_utf8(aff, dic)
    if not d.strip() or not a.strip():
        raise DictionaryError("These files are empty.")
    folder().mkdir(parents=True, exist_ok=True)
    (folder() / f"{code}.aff").write_text(a, encoding="utf-8")
    (folder() / f"{code}.dic").write_text(d, encoding="utf-8")


async def _reload() -> dict:
    try:
        return (await core.call("spellReload", {}, source="engine")).result
    except CoreError:
        return {}


async def listing() -> dict:
    try:
        r = (await core.call("spellLanguages", {}, source="engine")).result
    except CoreError as exc:
        raise DictionaryError(str(exc)) from exc
    for lang in r.get("languages", []):
        lang["downloadable"] = lang["code"] in CATALOG
        lang["source"] = BROWSE + CATALOG[lang["code"]].rsplit("/", 1)[0] if lang["code"] in CATALOG else None
        lang["removable"] = (folder() / f"{lang['code']}.dic").is_file()
    r["folder"] = str(folder())
    return r


def _fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "FormDesign"})
    with urllib.request.urlopen(req, timeout=90) as resp:
        return resp.read()


async def install(code: str) -> dict:
    stem = CATALOG.get(code)
    if not stem:
        raise DictionaryError(f"There is no dictionary to download for {code}; add Hunspell files instead.")
    try:
        aff, dic = await asyncio.gather(asyncio.to_thread(_fetch, SOURCE + stem + ".aff"), asyncio.to_thread(_fetch, SOURCE + stem + ".dic"))
    except OSError as exc:
        raise DictionaryError(f"The dictionary could not be downloaded: {exc}") from exc
    _write(code, aff, dic)
    return await _reload()


async def add_files(code: str, aff_path: str, dic_path: str) -> dict:
    a, d = Path(aff_path or ""), Path(dic_path or "")
    if a.suffix.lower() != ".aff" or d.suffix.lower() != ".dic" or not a.is_file() or not d.is_file():
        raise DictionaryError("Choose a Hunspell pair: one .aff file and one .dic file.")
    _write(code, a.read_bytes(), d.read_bytes())
    return await _reload()


async def remove(code: str) -> dict:
    if not CODE.match(code or ""):
        raise DictionaryError(f"{code} is not a language code.")
    gone = False
    for ext in (".aff", ".dic"):
        p = folder() / f"{code}{ext}"
        if p.is_file():
            p.unlink()
            gone = True
    if not gone:
        raise DictionaryError(f"No dictionary for {code} in {folder()}.")
    return await _reload()
