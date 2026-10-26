import re
import subprocess
import tomllib

from conftest import REPO, STRIP_EXEMPT

STRIPPING = re.compile(r"strip\s*=\s*True|--strip\b")


def tracked(pattern):
    try:
        out = subprocess.run(["git", "ls-files", "--", pattern], cwd=REPO, capture_output=True, text=True, check=True).stdout
        return [REPO / line for line in out.splitlines() if line]
    except (OSError, subprocess.CalledProcessError):
        return [p for p in REPO.rglob(pattern.split("/")[-1]) if not {"target", "node_modules", ".git", "venv"} & set(p.parts)]


def stripped_profiles(data, trail=("profile",)):
    for key, value in data.items():
        if isinstance(value, dict):
            yield from stripped_profiles(value, trail + (key,))
        elif key == "strip" and value not in (False, "none"):
            yield ".".join(trail)


def test_no_cargo_release_profile_strips():
    found = []
    for path in tracked("*Cargo.toml"):
        rel = path.relative_to(REPO).as_posix()
        if rel in STRIP_EXEMPT:
            continue
        data = tomllib.loads(path.read_text(encoding="utf-8"))
        found += [f"{rel} [{p}]" for p in stripped_profiles(data.get("profile", {}))]
    assert not found, found


def test_no_frozen_engine_build_strips():
    found = []
    for pattern in ("*.spec", "setup/freeze_backend*"):
        for path in tracked(pattern):
            text = path.read_text(encoding="utf-8", errors="replace")
            found += [f"{path.relative_to(REPO).as_posix()}: {m.group(0)}" for m in STRIPPING.finditer(text)]
    assert not found, found
