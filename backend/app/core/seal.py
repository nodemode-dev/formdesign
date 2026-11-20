import hashlib
import hmac
import json
import secrets

SERVICE = "FormDesign"
ACCOUNT = "settings-seal"
_CACHE: dict = {"tried": False, "available": False, "key": None}


def reset() -> None:
    _CACHE.update(tried=False, available=False, key=None)


def _load(create: bool) -> bytes | None:
    if _CACHE["tried"] and (_CACHE["key"] is not None or not _CACHE["available"] or not create):
        return _CACHE["key"]
    _CACHE["tried"] = True
    try:
        import keyring

        stored = keyring.get_password(SERVICE, ACCOUNT)
        if not stored and create:
            keyring.set_password(SERVICE, ACCOUNT, secrets.token_hex(32))
            stored = keyring.get_password(SERVICE, ACCOUNT)
        key = bytes.fromhex(stored) if stored else None
    except Exception:
        _CACHE.update(available=False, key=None)
        return None
    _CACHE.update(available=True, key=key)
    return key


def available() -> bool:
    _load(False)
    return _CACHE["available"]


def digest(values: dict, key: bytes) -> str:
    body = json.dumps(values, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hmac.new(key, body.encode("utf-8"), hashlib.sha256).hexdigest()


def check(values: dict, mark) -> str:
    key = _load(False)
    if not _CACHE["available"]:
        return "unavailable"
    if key is not None and isinstance(mark, str) and hmac.compare_digest(mark, digest(values, key)):
        return "ok"
    return "unsealed" if mark is None else "mismatch"


def sign(values: dict) -> str | None:
    key = _load(True)
    return None if key is None else digest(values, key)
