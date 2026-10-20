@echo off
setlocal
cd /d "%~dp0"
if not exist venv\Scripts\activate.bat ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
call venv\Scripts\activate.bat
if not defined FORMDESIGN_HOST set "FORMDESIGN_HOST=127.0.0.1"
if not defined FORMDESIGN_PORT set "FORMDESIGN_PORT=8196"
uvicorn app.main:app --reload --reload-dir app --host %FORMDESIGN_HOST% --port %FORMDESIGN_PORT%
