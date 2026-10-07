@echo off
setlocal
cd /d "%~dp0.."
if not exist frontend\dist\index.html call setup\build_frontend.bat
set "FORMDES_DIST=%cd%\frontend\dist"
cd backend
if not exist venv\Scripts\activate.bat ( echo [formdes] engine venv missing, run formdes.bat install & exit /b 1 )
call venv\Scripts\activate.bat
if not defined FORMDES_HOST set "FORMDES_HOST=127.0.0.1"
if not defined FORMDES_PORT set "FORMDES_PORT=8196"
echo [formdes] serving http://%FORMDES_HOST%:%FORMDES_PORT%
uvicorn app.main:app --host %FORMDES_HOST% --port %FORMDES_PORT%
