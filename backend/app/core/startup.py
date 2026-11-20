import json
import os
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

APP = "formdesign"
NAMES = ("formdesign", "FormDesign")
SHELL_ENV = "FORMDESIGN_SHELL_PID"
ALONE_ENV = "FORMDESIGN_NO_SHELL"
LOCK_NAME = "engine.lock"
ERRORS_NAME = "formdesign-engine-errors.log"
ERRORS_BYTES = 1_048_576
STAND_INS = []
PROC = "/proc"
PLACES = ("..", ".", os.path.join("..", "..", "MacOS"), os.path.join("..", "..", "..", "bin"))
STARTED = time.time()
POLL = 2.0
END_WAIT = 5.0
STEP = 0.1
SLACK = 3.0
OLD_SLACK = 180.0
FILETIME_EPOCH = 116_444_736_000_000_000
WINDOWS = sys.platform == "win32"
SYNCHRONIZE = 0x00100000
QUERY_LIMITED = 0x1000
WAIT_TIMEOUT = 0x102
ACCESS_DENIED = 5
NO_WINDOW = 0x08000000
DETACHED = 0x00000008
NEW_GROUP = 0x00000200
FREE = "free"
BUSY = "busy"
LEFTOVER = "leftover"


class Busy(RuntimeError):
    pass


def keep_errors(folder: Path) -> Path | None:
    if getattr(sys.stderr, "name", None) != os.devnull:
        return None
    path = folder / ERRORS_NAME
    try:
        mode = "w" if path.stat().st_size > ERRORS_BYTES else "a"
    except OSError:
        mode = "a"
    try:
        kept = open(path, mode, encoding="utf-8", errors="replace", buffering=1)
    except OSError:
        return None
    STAND_INS.append(sys.stderr)
    sys.stderr = kept
    return path


def _kernel():
    import ctypes

    lib = ctypes.WinDLL("kernel32", use_last_error=True)
    lib.OpenProcess.restype = ctypes.c_void_p
    lib.OpenProcess.argtypes = (ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32)
    lib.WaitForSingleObject.restype = ctypes.c_uint32
    lib.WaitForSingleObject.argtypes = (ctypes.c_void_p, ctypes.c_uint32)
    lib.CloseHandle.restype = ctypes.c_int
    lib.CloseHandle.argtypes = (ctypes.c_void_p,)
    lib.GetProcessTimes.restype = ctypes.c_int
    lib.GetProcessTimes.argtypes = (ctypes.c_void_p,) + (ctypes.POINTER(ctypes.c_uint64),) * 4
    lib.SetDllDirectoryW.restype = ctypes.c_int
    lib.SetDllDirectoryW.argtypes = (ctypes.c_wchar_p,)
    return lib


def _open_windows(pid: int, access: int):
    import ctypes

    handle = _kernel().OpenProcess(access, 0, pid)
    if handle:
        return handle
    if ctypes.get_last_error() == ACCESS_DENIED:
        raise PermissionError(pid)
    return None


def _alive_windows(pid: int) -> bool:
    try:
        handle = _open_windows(pid, SYNCHRONIZE)
    except PermissionError:
        return True
    if handle is None:
        return False
    try:
        return _kernel().WaitForSingleObject(handle, 0) == WAIT_TIMEOUT
    finally:
        _kernel().CloseHandle(handle)


def _created_windows(pid: int) -> float:
    import ctypes

    try:
        handle = _open_windows(pid, QUERY_LIMITED)
    except PermissionError:
        return 0.0
    if handle is None:
        return 0.0
    try:
        times = [ctypes.c_uint64(0) for _ in range(4)]
        if not _kernel().GetProcessTimes(handle, *[ctypes.byref(item) for item in times]):
            return 0.0
        return (times[0].value - FILETIME_EPOCH) / 10_000_000
    finally:
        _kernel().CloseHandle(handle)


def _end_windows(pid: int) -> None:
    subprocess.run(
        ["taskkill", "/PID", str(pid), "/T", "/F"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=NO_WINDOW,
        timeout=END_WAIT * 2,
        check=False,
    )


def _alive_posix(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    try:
        with open(f"{PROC}/{pid}/stat") as stat:
            return stat.read().rpartition(")")[2].split()[0] != "Z"
    except (OSError, IndexError):
        return True


def _created_posix(pid: int) -> float:
    try:
        with open(f"{PROC}/{pid}/stat") as stat:
            ticks = int(stat.read().rpartition(")")[2].split()[19])
        with open(f"{PROC}/stat") as stat:
            boot = int(next(line for line in stat if line.startswith("btime")).split()[1])
        return boot + ticks / os.sysconf("SC_CLK_TCK")
    except (OSError, IndexError, ValueError, StopIteration):
        pass
    found = subprocess.run(
        ["ps", "-o", "etime=", "-p", str(pid)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        timeout=END_WAIT,
        check=False,
    )
    days, _, clock = found.stdout.decode("ascii", "replace").strip().rpartition("-")
    parts = [int(part) for part in clock.split(":")]
    if not 2 <= len(parts) <= 3:
        return 0.0
    seconds = 0
    for part in parts:
        seconds = seconds * 60 + part
    return time.time() - seconds - int(days or 0) * 86400


def alive(pid: int) -> bool:
    if pid <= 0:
        return False
    return _alive_windows(pid) if WINDOWS else _alive_posix(pid)


def created(pid: int) -> float:
    if pid <= 0:
        return 0.0
    try:
        return _created_windows(pid) if WINDOWS else _created_posix(pid)
    except (OSError, ValueError, subprocess.SubprocessError):
        return 0.0


def moment(value) -> float:
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0 else 0.0


def same(pid: int, born: float) -> bool:
    seen = created(pid)
    return alive(pid) and (not born or (seen > 0 and abs(seen - born) <= SLACK))


def gone_by(pid: int, seconds: float) -> bool:
    until = time.monotonic() + seconds
    while alive(pid):
        if time.monotonic() >= until:
            return False
        time.sleep(STEP)
    return True


def end(pid: int) -> bool:
    try:
        if WINDOWS:
            _end_windows(pid)
        else:
            os.kill(pid, signal.SIGTERM)
        if gone_by(pid, END_WAIT):
            return True
        if not WINDOWS:
            os.kill(pid, signal.SIGKILL)
    except (OSError, subprocess.SubprocessError):
        pass
    return gone_by(pid, END_WAIT)


def number(value) -> int:
    return value if isinstance(value, int) and not isinstance(value, bool) and value > 0 else 0


def read_lock(path: Path) -> dict:
    try:
        found = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    return found if isinstance(found, dict) else {}


def holder(lock: dict, shelled: bool) -> str:
    pid = number(lock.get("pid"))
    if not pid or pid == os.getpid() or not alive(pid):
        return FREE
    born, started, seen = moment(lock.get("born")), moment(lock.get("started")), created(pid)
    if born:
        if not same(pid, born):
            return FREE
    elif started:
        if not seen or not -SLACK <= started - seen <= OLD_SLACK:
            return FREE
    else:
        return BUSY
    if "shell" not in lock:
        return LEFTOVER if shelled else BUSY
    shell = number(lock.get("shell"))
    if not shell:
        return BUSY
    return BUSY if same(shell, moment(lock.get("shellBorn"))) else LEFTOVER


def claim(data: Path, shell: int) -> tuple[Path, int]:
    path = data / LOCK_NAME
    lock = read_lock(path)
    held = number(lock.get("pid"))
    state = holder(lock, shell > 0)
    if state == BUSY:
        raise Busy(f"{APP} is already running on {data} (engine process {held}).")
    if state == LEFTOVER and not end(held):
        raise Busy(f"{APP} could not stop an engine that was left running on {data} (engine process {held}).")
    record = {"pid": os.getpid(), "started": int(STARTED), "born": created(os.getpid()), "shell": shell, "shellBorn": created(shell)}
    fresh = path.with_name(f"{LOCK_NAME}.{os.getpid()}")
    fresh.write_text(json.dumps(record), encoding="utf-8")
    os.replace(fresh, path)
    return path, held if state == LEFTOVER else 0


def shell_pid() -> int:
    raw = os.environ.get(SHELL_ENV, "").strip()
    return int(raw) if raw.isascii() and raw.isdigit() else 0


def alone() -> bool:
    return bool(getattr(sys, "frozen", False)) and not shell_pid() and os.environ.get(ALONE_ENV) != "1"


def app_beside(engine: Path) -> Path | None:
    engine = engine.resolve()
    suffix = ".exe" if WINDOWS else ""
    for place in PLACES:
        for name in NAMES:
            found = (engine.parent / place / f"{name}{suffix}").resolve()
            if found != engine and found.is_file() and os.access(found, os.X_OK):
                return found
    return None


def open_app(app: Path) -> bool:
    env = dict(os.environ)
    if "LD_LIBRARY_PATH_ORIG" in env:
        env["LD_LIBRARY_PATH"] = env.pop("LD_LIBRARY_PATH_ORIG")
    elif getattr(sys, "frozen", False):
        env.pop("LD_LIBRARY_PATH", None)
    try:
        if WINDOWS:
            _kernel().SetDllDirectoryW(None)
        subprocess.Popen(
            [str(app)],
            cwd=str(app.parent),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=(DETACHED | NEW_GROUP) if WINDOWS else 0,
            start_new_session=not WINDOWS,
        )
    except OSError:
        return False
    return True


def watch(pid: int, leave) -> None:
    direct = os.getppid() == pid
    handle = None
    if WINDOWS:
        try:
            handle = _open_windows(pid, SYNCHRONIZE)
        except PermissionError:
            return

    def gone() -> bool:
        if handle is not None:
            return _kernel().WaitForSingleObject(handle, int(POLL * 1000)) != WAIT_TIMEOUT
        time.sleep(POLL)
        return not alive(pid) or (direct and os.getppid() != pid)

    def run() -> None:
        while not gone():
            pass
        leave()

    threading.Thread(target=run, name=f"{APP}-shell-watch", daemon=True).start()
