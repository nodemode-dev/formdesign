@echo off
setlocal
set "ROOT=%~dp0.."
for /f "tokens=2" %%t in ('rustc -vV ^| findstr /b "host:"') do set "TRIPLE=%%t"
if not exist "%ROOT%\src-tauri\binaries\formdes-engine-%TRIPLE%.exe" call "%ROOT%\setup\freeze_backend.bat" || exit /b 1
cd /d "%ROOT%"
call frontend\node_modules\.bin\tauri.cmd dev %*
