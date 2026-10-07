#!/usr/bin/env bash
FORMDES_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export FORMDES_ROOT
fd_say() { printf '[formdes] %s\n' "$*"; }
fd_die() { printf '[formdes] error: %s\n' "$*" >&2; exit 1; }
fd_os() {
  case "$(uname -s 2>/dev/null)" in
    Darwin) echo macos ;;
    Linux) echo linux ;;
    MINGW*|MSYS*|CYGWIN*) echo windows ;;
    *) echo unknown ;;
  esac
}
fd_python() {
  local c
  for c in python3.14 python3.13 python3.12 python3; do
    if command -v "$c" >/dev/null 2>&1 && "$c" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 12) else 1)' 2>/dev/null; then
      echo "$c"
      return 0
    fi
  done
  return 1
}
fd_venv() {
  [ -f "$FORMDES_ROOT/backend/venv/bin/activate" ] || fd_die "engine venv missing, run ./formdes install"
  # shellcheck disable=SC1091
  source "$FORMDES_ROOT/backend/venv/bin/activate"
}
fd_target_triple() {
  if [ -n "${FORMDES_TARGET:-}" ]; then echo "$FORMDES_TARGET"; return 0; fi
  command -v rustc >/dev/null 2>&1 || return 1
  rustc -vV | awk '/^host:/ {print $2}'
}
fd_exe() { [ "$(fd_os)" = windows ] && echo ".exe" || echo ""; }
fd_core() {
  local exe
  exe="$(fd_exe)"
  if [ -n "${FORMDES_CORE:-}" ] && [ -x "$FORMDES_CORE" ]; then echo "$FORMDES_CORE"; return 0; fi
  for p in "$FORMDES_ROOT/native/target/release/formdes-core$exe" "$FORMDES_ROOT/native/bin/formdes-core$exe"; do
    if [ -x "$p" ]; then echo "$p"; return 0; fi
  done
  return 1
}
