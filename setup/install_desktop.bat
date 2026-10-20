@echo off
setlocal
set "ROOT=%~dp0.."
if not exist "%ROOT%\backend\venv\Scripts\activate.bat" ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
call "%ROOT%\backend\venv\Scripts\activate.bat"
pip install -r "%ROOT%\backend\requirements_desktop.txt" || exit /b 1
where cargo >nul 2>nul || ( echo [formdesign] Rust is not installed. Install it from https://rustup.rs then rerun formdesign.bat install-desktop & exit /b 1 )
if not exist "%ROOT%\frontend\node_modules\.bin\tauri.cmd" ( cd /d "%ROOT%\frontend" && call npm install )
echo [formdesign] desktop toolchain ready
