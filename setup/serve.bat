@echo off
setlocal
cd /d "%~dp0.."
if not exist frontend\dist\index.html call setup\build_frontend.bat
set "FORMDESIGN_DIST=%cd%\frontend\dist"
cd backend
if not exist venv\Scripts\activate.bat ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
call venv\Scripts\activate.bat
if not defined FORMDESIGN_HOST set "FORMDESIGN_HOST=127.0.0.1"
if not defined FORMDESIGN_PORT set "FORMDESIGN_PORT=8196"
echo [formdesign] serving http://%FORMDESIGN_HOST%:%FORMDESIGN_PORT%
uvicorn app.main:app --host %FORMDESIGN_HOST% --port %FORMDESIGN_PORT%
