@echo off
setlocal
cd /d "%~dp0.."
start "FormDesign engine" cmd /k "cd /d %cd%\backend && boot_backend.bat"
start "FormDesign frontend" cmd /k "cd /d %cd%\frontend && boot_frontend.bat"
echo [formdesign] frontend http://localhost:5196 and engine http://127.0.0.1:8196
