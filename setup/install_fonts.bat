@echo off
setlocal
cd /d "%~dp0.."
if not exist backend\venv\Scripts\activate.bat ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
if not exist frontend\node_modules\@fontsource ( pushd frontend & call npm install & popd )
call backend\venv\Scripts\activate.bat
python setup\fonts.py frontend\node_modules\@fontsource backend\app\fonts
