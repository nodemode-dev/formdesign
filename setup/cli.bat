@echo off
setlocal
if not defined FORMDESIGN_CWD set "FORMDESIGN_CWD=%CD%"
cd /d "%~dp0..\backend"
if not exist venv\Scripts\activate.bat ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
call venv\Scripts\activate.bat
python -m app.cli %*
