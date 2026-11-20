"""One finder and one starter for the agent programs a Form engine runs for the person: Claude Code, Codex, Cursor's agent
and Antigravity, held as a table (PROGRAMS) so another program is one more entry.

A program started from the Dock, the Finder, the Start menu or a desktop launcher gets the system's short PATH, not the one a
terminal has, so a bare PATH search reports programs installed by Homebrew, npm, a version manager or into the home folder as
missing. This file looks in order: the person's setting, the app's development override variable, the engine's PATH, the known
install folders for the system, then the person's own shell PATH (read once and kept). It builds the one environment every
start uses (the program's own folder first on PATH, so a node script finds the node beside it), starts tabs, Chat and checks
the same way, answers "installed" three ways (runs, found but does not start, not found) and keeps the record of where it
looked so the message can say so.

Shared by the Form family: standard library only, nothing imported from the app, copied unchanged between engines and held
to the hash recorded for VERSION. The app passes in what is its own: the setting's value, the names of its override
variables, its base environment and how long to wait."""

from __future__ import annotations

import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

VERSION = "1.0.1"
PROGRAMS: dict = {
    "claude": {"title": "Claude Code", "names": ("claude",), "setting": "claudeBin",
               "install": "curl -fsSL https://claude.ai/install.sh | bash", "install_windows": "irm https://claude.ai/install.ps1 | iex",
               "home": (".claude/local", ".claude/bin"), "windows": ("home:.claude/local", "home:.claude/bin", "local:Programs/claude", "roaming:Programs/claude")},
    "codex": {"title": "Codex", "names": ("codex",), "setting": "codexBin", "install": "npm install -g @openai/codex",
              "windows": ("local:Programs/OpenAI/Codex/bin",), "mac_files": ("Codex.app/Contents/Resources/codex", "Codex.app/Contents/Resources/bin/codex")},
    "cursor": {"title": "Cursor", "names": ("cursor-agent", "agent"), "setting": "cursorBin", "install": "curl https://cursor.com/install -fsS | bash",
               "home": (".local/share/cursor-agent/versions/*",), "windows": ("local:cursor-agent",), "real": {"agent": "cursor"}},
    "antigravity": {"title": "Antigravity", "names": ("agy",), "setting": "antigravityBin",
                    "install": "curl -fsSL https://antigravity.google/cli/install.sh | bash", "install_windows": "irm https://antigravity.google/cli/install.ps1 | iex",
                    "windows_search": ("antigravity", "agy")},
}
KEEP_ENV = frozenset({"CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CONFIG_DIR"})
LIBRARY_VARS = ("LD_LIBRARY_PATH", "DYLD_LIBRARY_PATH")
WINDOWS_ENDINGS = (".exe", ".cmd", ".bat")
MARK = "__FORM_AGENT_LOOKUP__"
HOW = {"setting": "your setting", "override": "the development override", "path": "the engine's PATH", "folder": "a known install folder", "shell": "your shell's PATH"}
RETRY_SECONDS = 60.0

_lock = threading.RLock()
_shell: dict = {}
_checked: dict = {}


@dataclass
class Found:
    program: str
    title: str
    path: str | None = None
    how: str = ""
    state: str = "missing"
    version: str | None = None
    error: str = ""
    looked: list[str] = field(default_factory=list)


def register(key: str, title: str, names: tuple | list, **more) -> None:
    """Adds a program to the table, or replaces one, without changing this file. The keys of an entry: title (the name
    shown to people), names (the program's file names, first wins), setting (the family's name for its path setting),
    install and install_windows (the install command), home (folders under the home folder on macOS and Linux; a * stands
    for version folders, newest first), windows (folders as local:, roaming: or home: and a path), mac_files (files inside
    an application bundle), windows_search (name fragments of folders one and two levels under %LOCALAPPDATA%) and real
    (for a name other programs also use: the word its real path must contain)."""
    PROGRAMS[key] = {"title": title, "names": tuple(names), **more}


def install(program: str, platform: str | None = None) -> str:
    spec = PROGRAMS[program]
    return (spec.get("install_windows") if windows(platform) else "") or spec.get("install", "")


def forget() -> None:
    with _lock:
        _shell.clear()
        _checked.clear()


def windows(platform: str | None = None) -> bool:
    return (platform or sys.platform).startswith("win")


def runnable(path: str | os.PathLike, platform: str | None = None) -> bool:
    p = Path(path).expanduser()
    return p.is_file() and (windows(platform) or os.access(p, os.X_OK))


def short(path: str | os.PathLike, home: str | os.PathLike | None = None) -> str:
    text, base = str(path), str(home or Path.home())
    return "~" + text[len(base):] if base not in ("", os.sep) and (text == base or text.startswith(base + os.sep)) else text


def _version_key(p: Path) -> tuple:
    return tuple(int(x) for x in re.findall(r"\d+", p.name)) or (0,)


def _versions(base: Path, tail: str = "") -> list[Path]:
    try:
        kept = [d for d in base.iterdir() if d.is_dir()]
    except OSError:
        return []
    return [d / tail if tail else d for d in sorted(kept, key=_version_key, reverse=True)]


def system_folders(platform: str | None = None) -> list[Path]:
    platform = platform or sys.platform
    if windows(platform):
        return []
    if platform == "darwin":
        return [Path("/opt/homebrew/bin"), Path("/usr/local/bin"), Path("/opt/local/bin")]
    return [Path("/usr/local/bin"), Path("/usr/bin"), Path("/snap/bin"), Path("/home/linuxbrew/.linuxbrew/bin")]


def folders(program: str, home: str | os.PathLike | None = None, env: dict | None = None, platform: str | None = None) -> list[Path]:
    platform = platform or sys.platform
    home = Path(home) if home else Path.home()
    env = os.environ if env is None else env
    win = windows(platform)
    spec = PROGRAMS[program]
    out: list[Path] = []
    for key in ("NPM_CONFIG_PREFIX", "npm_config_prefix"):
        if env.get(key):
            out.append(Path(env[key]) if win else Path(env[key]) / "bin")
    for key, tail in (("PNPM_HOME", ""), ("VOLTA_HOME", "bin"), ("BUN_INSTALL", "bin"), ("NVM_SYMLINK", "")):
        if env.get(key):
            out.append(Path(env[key]) / tail if tail else Path(env[key]))
    if win:
        roots = {"home": home, "roaming": Path(env.get("APPDATA") or home / "AppData" / "Roaming"), "local": Path(env.get("LOCALAPPDATA") or home / "AppData" / "Local")}
        out += [home / ".local" / "bin", roots["roaming"] / "npm", roots["local"] / "npm", roots["local"] / "pnpm", roots["local"] / "Volta" / "bin",
                home / ".bun" / "bin", home / ".volta" / "bin"]
        for place in spec.get("windows", ()):
            root, _, rest = place.partition(":")
            out.append(roots[root].joinpath(*rest.split("/")))
        return list(dict.fromkeys(out))
    out += [home / ".local" / "bin", home / ".npm-global" / "bin", home / ".bun" / "bin", home / ".volta" / "bin", home / ".local" / "share" / "pnpm"]
    if platform == "darwin":
        out.append(home / "Library" / "pnpm")
    out += _versions(home / ".nvm" / "versions" / "node", "bin")
    out += _versions(home / ".local" / "share" / "fnm" / "node-versions", "installation/bin")
    out += _versions(home / "Library" / "Application Support" / "fnm" / "node-versions", "installation/bin")
    out += _versions(home / ".asdf" / "installs" / "nodejs", "bin")
    out += _versions(home / ".local" / "share" / "mise" / "installs" / "node", "bin")
    out += [home / ".asdf" / "shims", home / ".local" / "share" / "mise" / "shims"]
    for place in spec.get("home", ()):
        base, star, tail = place.partition("*")
        out += _versions(home.joinpath(*base.strip("/").split("/")), tail.strip("/")) if star else [home.joinpath(*place.split("/"))]
    out += system_folders(platform)
    return list(dict.fromkeys(out))


def _searched(root: Path, fragments: tuple) -> list[Path]:
    out: list[Path] = []
    named = lambda d: any(f in d.name.lower() for f in fragments)
    try:
        first = sorted(d for d in root.iterdir() if d.is_dir())
    except OSError:
        return out
    for one in first:
        if named(one):
            out.append(one)
        try:
            out += [two for two in sorted(one.iterdir()) if two.is_dir() and (named(one) or named(two))]
        except OSError:
            continue
    return out


def candidates(program: str, home: str | os.PathLike | None = None, env: dict | None = None, platform: str | None = None) -> list[Path]:
    platform = platform or sys.platform
    home = Path(home) if home else Path.home()
    env = os.environ if env is None else env
    spec = PROGRAMS[program]
    endings = WINDOWS_ENDINGS if windows(platform) else ("",)
    places = folders(program, home, env, platform)
    if windows(platform) and spec.get("windows_search"):
        places += _searched(Path(env.get("LOCALAPPDATA") or home / "AppData" / "Local"), tuple(spec["windows_search"]))
    out = [folder / (name + ending) for folder in dict.fromkeys(places) for name in spec["names"] for ending in endings]
    if platform == "darwin":
        out += [apps.joinpath(*inside.split("/")) for apps in (Path("/Applications"), home / "Applications") for inside in spec.get("mac_files", ())]
    return out


def _trusted(program: str, path: str | os.PathLike) -> bool:
    name = Path(path).name.lower()
    stem = name.rsplit(".", 1)[0] if name.endswith(WINDOWS_ENDINGS) else name
    word = PROGRAMS[program].get("real", {}).get(stem)
    return not word or word in os.path.realpath(path).lower()


def _on_path(program: str, path: str) -> str | None:
    if not path:
        return None
    for name in PROGRAMS[program]["names"]:
        hit = shutil.which(name, path=path)
        if hit and _trusted(program, hit):
            return os.path.abspath(hit)
    return None


def login_shell(env: dict | None = None, platform: str | None = None) -> str:
    env = os.environ if env is None else env
    chosen = env.get("SHELL")
    if chosen:
        return chosen
    if (platform or sys.platform) == "darwin" and os.path.exists("/bin/zsh"):
        return "/bin/zsh"
    return shutil.which("bash") or "/bin/sh"


def _stop(proc: subprocess.Popen) -> None:
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (OSError, AttributeError):
        pass
    try:
        proc.kill()
    except OSError:
        pass
    try:
        proc.communicate(timeout=2)
    except (OSError, ValueError, subprocess.SubprocessError):
        pass


def _ask_shell(shell: str, flags: list[str], env: dict, timeout: float) -> list[str] | None:
    script = f"printf '%s%s%s' {MARK} (string join : $PATH) {MARK}" if Path(shell).name == "fish" else f'printf "%s%s%s" {MARK} "$PATH" {MARK}'
    try:
        proc = subprocess.Popen([shell, *flags, "-c", script], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                env=env, text=True, errors="replace", start_new_session=True)
    except OSError:
        return None
    try:
        out, _ = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        _stop(proc)
        return None
    hit = re.search(re.escape(MARK) + "(.*?)" + re.escape(MARK), out or "", re.S)
    parts = [p for p in hit.group(1).split(os.pathsep) if p] if hit else []
    return parts or None


def _registry_path() -> list[str]:
    try:
        import winreg
    except ImportError:
        return []
    out: list[str] = []
    for root, key in ((winreg.HKEY_CURRENT_USER, "Environment"), (winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")):
        try:
            with winreg.OpenKey(root, key) as opened:
                value, _kind = winreg.QueryValueEx(opened, "Path")
        except OSError:
            continue
        out += [p.strip() for p in winreg.ExpandEnvironmentStrings(str(value)).split(";") if p.strip()]
    return list(dict.fromkeys(out))


def shell_path(env: dict | None = None, timeout: float = 10.0, fresh: bool = False, platform: str | None = None) -> tuple[list[str], str]:
    """The person's own PATH: on macOS and Linux what their login, interactive shell ends up with (login only when the
    interactive start gives nothing), on Windows the user and system Path values in the registry. Asked once and kept; an
    empty answer is asked again after a minute, or at once with fresh."""
    env = dict(os.environ if env is None else env)
    with _lock:
        if _shell and not fresh and (_shell["path"] or time.monotonic() - _shell["at"] < RETRY_SECONDS):
            return list(_shell["path"]), _shell["note"]
        if windows(platform):
            path = _registry_path()
            note = "the Path values in the registry" if path else "the Path values in the registry (none could be read)"
        else:
            shell = login_shell(env, platform)
            name = Path(shell).name
            path = _ask_shell(shell, ["-l", "-i"], env, timeout)
            note = f"your shell ({name}, login and interactive)"
            if path is None:
                path = _ask_shell(shell, ["-l"], env, timeout)
                note = f"your shell ({name}, login only: the interactive start gave no PATH within {timeout:g} s)"
            if path is None:
                path, note = [], f"your shell ({name} gave no PATH within {timeout:g} s)"
        _shell.update(path=list(path), note=note, at=time.monotonic())
        return list(path), note


def find(program: str, setting: str | None = None, override_vars: tuple = (), env: dict | None = None, home: str | os.PathLike | None = None,
         platform: str | None = None, shell_timeout: float = 10.0, fresh: bool = False) -> Found:
    """Where the program is, in the family's order. setting is the person's path for it ("" when empty, None when the app
    has no such setting); override_vars are the names of the app's development override variables, read from env."""
    env = os.environ if env is None else env
    platform = platform or sys.platform
    spec = PROGRAMS[program]
    found = Found(program, spec["title"])

    def at(path: str | os.PathLike, how: str) -> Found:
        found.path, found.how, found.state = str(path), how, "found"
        return found

    if setting is not None:
        chosen = setting.strip()
        if chosen and runnable(chosen, platform):
            return at(Path(chosen).expanduser(), "setting")
        found.looked.append(f"your setting ({chosen}, which is not a program that can run)" if chosen else "your setting (empty)")
    for var in override_vars:
        value = (env.get(var) or "").strip()
        if value and runnable(value, platform):
            return at(Path(value).expanduser(), "override")
        if value:
            found.looked.append(f"{var} ({value}, which is not a program that can run)")
    engine_path = env.get("PATH", "")
    hit = _on_path(program, engine_path)
    if hit:
        return at(hit, "path")
    found.looked.append(f"PATH ({engine_path or 'empty'})")
    places = candidates(program, home, env, platform)
    for place in places:
        if runnable(place, platform) and _trusted(program, place):
            return at(place, "folder")
    found.looked.append(", ".join(dict.fromkeys(short(place.parent, home) for place in places)))
    dirs, note = shell_path(env, shell_timeout, fresh, platform)
    hit = _on_path(program, os.pathsep.join(dirs))
    if hit:
        return at(hit, "shell")
    found.looked.append(note)
    return found


def child_env(path: str | os.PathLike | None = None, base: dict | None = None, extra: dict | None = None, front: tuple | list = (),
              home: str | os.PathLike | None = None, platform: str | None = None, shell_timeout: float = 10.0, keep: frozenset | set = KEEP_ENV) -> dict:
    """The environment every start of an agent program gets. PATH is the program's own folder, the app's own folders
    (front), the person's shell PATH, the known install folders that exist, then the engine's PATH. CLAUDE* variables other
    than keep are left out, and a frozen engine's library path is put back as it was. extra is applied last."""
    base = dict(os.environ if base is None else base)
    env = {k: v for k, v in base.items() if not k.startswith("CLAUDE") or k in keep}
    for name in LIBRARY_VARS:
        original = base.get(name + "_ORIG")
        if original is not None:
            env[name] = original
        elif getattr(sys, "frozen", False) or base.get("_MEIPASS2"):
            env.pop(name, None)
    dirs, _note = shell_path(base, shell_timeout, False, platform)
    known = [str(f) for program in PROGRAMS for f in folders(program, home, base, platform) if f.is_dir()]
    parts = ([str(Path(path).parent)] if path else []) + [str(f) for f in front] + dirs + known + base.get("PATH", "").split(os.pathsep)
    env["PATH"] = os.pathsep.join(dict.fromkeys(p for p in parts if p))
    env.update(extra or {})
    return env


def _quote(word: str, fish: bool) -> str:
    if word and re.fullmatch(r"[A-Za-z0-9_@%+=:,./-]+", word):
        return word
    if fish:
        return "'" + word.replace("\\", "\\\\").replace("'", "\\'") + "'"
    return "'" + word.replace("'", "'\"'\"'") + "'"


def start_argv(cmd: list, env: dict | None = None, platform: str | None = None) -> list[str]:
    """How a program is started, the same for a tab, Chat and a check: through the person's login shell on macOS and Linux
    (exec, so the process is the program), directly on Windows with the command interpreter in front of a .cmd or .bat."""
    words = [str(c) for c in cmd]
    env = os.environ if env is None else env
    if windows(platform):
        return [env.get("COMSPEC") or "cmd.exe", "/c", *words] if words[0].lower().endswith((".cmd", ".bat")) else words
    shell = login_shell(env, platform)
    fish = Path(shell).name == "fish"
    return [shell, "-l", "-c", "exec " + " ".join(_quote(w, fish) for w in words)]


def run(cmd: list, env: dict, timeout: float = 15.0, platform: str | None = None) -> subprocess.CompletedProcess:
    return subprocess.run(start_argv(cmd, env, platform), capture_output=True, text=True, errors="replace", timeout=timeout, env=env, stdin=subprocess.DEVNULL)


def _last_line(text: str) -> str:
    lines = [line.strip() for line in (text or "").splitlines() if line.strip() and "use -[v]S to pass options" not in line]
    return lines[-1][:300] if lines else ""


def _version(text: str) -> str | None:
    words = (text or "").split()
    return next((w for w in words if any(ch.isdigit() for ch in w)), words[0] if words else None)


def _stamp(path: str) -> tuple:
    try:
        st = os.stat(path)
        return (path, st.st_mtime_ns, st.st_size)
    except OSError:
        return (path, 0, 0)


def check(found: Found, env: dict | None = None, timeout: float = 15.0, fresh: bool = False, platform: str | None = None) -> Found:
    """Whether a found program starts: `--version` in the child environment, started as a tab would start it. The state
    becomes runs (with the version) or broken (with the error line). An answer is kept for that file until it changes or
    fresh asks again; a broken answer is asked again after a minute."""
    if not found.path:
        return found
    key = _stamp(found.path)
    with _lock:
        kept = None if fresh else _checked.get(key)
    if kept is not None and kept[0] != "runs" and time.monotonic() - kept[3] >= RETRY_SECONDS:
        kept = None
    if kept is None:
        env = child_env(found.path, platform=platform) if env is None else env
        try:
            out = run([found.path, "--version"], env, timeout, platform)
            if out.returncode == 0:
                kept = ("runs", _version(out.stdout), "", time.monotonic())
            else:
                kept = ("broken", None, _last_line(out.stderr) or _last_line(out.stdout) or f"it ended with code {out.returncode}", time.monotonic())
        except subprocess.TimeoutExpired:
            kept = ("broken", None, f"it gave no answer to --version in {timeout:g} seconds", time.monotonic())
        except OSError as exc:
            kept = ("broken", None, exc.strerror or str(exc), time.monotonic())
        with _lock:
            _checked[key] = kept
    found.state, found.version, found.error = kept[0], kept[1], kept[2]
    return found


def sentence(found: Found, install: str = "", otherwise: str = "") -> str:
    """The truthful message, built from the record. install is the app's install command for the program; otherwise is
    what else the person can do in this app, as a clause ("give its path in Preferences")."""
    if found.state == "missing":
        ways = ([f"install it with: {install}"] if install else []) + ([otherwise] if otherwise else [])
        tail = " " + ", or ".join(ways) + "." if ways else ""
        return f"{found.title} was not found. Looked in: {'; '.join(found.looked)}." + (tail[:1] + tail[1:2].upper() + tail[2:] if tail else "")
    where = f"{found.path} ({HOW.get(found.how, found.how)})"
    if found.state == "broken":
        tail = f" To use another copy, {otherwise}." if otherwise else ""
        return f"{found.title} was found at {where} but does not start: {found.error}.{tail}"
    return f"{found.title} is at {where}."


def answer(found: Found, install: str = "", otherwise: str = "") -> dict:
    """The fields a status route adds for one program: installed is true only when the program runs, and finding is
    the three-way answer (runs, broken, missing)."""
    return {"installed": found.state == "runs", "path": found.path, "version": found.version, "finding": found.state, "how": found.how,
            "looked": list(found.looked), "error": found.error, "said": sentence(found, install, otherwise)}
