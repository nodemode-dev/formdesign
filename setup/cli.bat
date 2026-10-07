@echo off
setlocal
cd /d "%~dp0..\backend"
if not exist venv\Scripts\activate.bat ( echo [formdes] engine venv missing, run formdes.bat install & exit /b 1 )
call venv\Scripts\activate.bat
python -m app.cli %*
