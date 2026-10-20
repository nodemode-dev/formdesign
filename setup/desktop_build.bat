@echo off
setlocal
set "ROOT=%~dp0.."
call "%ROOT%\setup\build_frontend.bat" || exit /b 1
call "%ROOT%\setup\freeze_backend.bat" || exit /b 1
cd /d "%ROOT%"
if not exist src-tauri\icons\icon.ico call frontend\node_modules\.bin\tauri.cmd icon assets\formdesign_icon.png
call frontend\node_modules\.bin\tauri.cmd build %*
echo [formdesign] bundles are in src-tauri\target\release\bundle
