@echo off
setlocal
cd /d "%~dp0..\frontend"
if not exist node_modules call npm install
call npm run build
