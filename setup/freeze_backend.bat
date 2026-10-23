@echo off
setlocal enabledelayedexpansion
set "ROOT=%~dp0.."
if not exist "%ROOT%\backend\venv\Scripts\activate.bat" ( echo [formdesign] engine venv missing, run formdesign.bat install & exit /b 1 )
call "%ROOT%\backend\venv\Scripts\activate.bat"
python -c "import PyInstaller" 2>nul || ( echo [formdesign] PyInstaller missing, run formdesign.bat install-desktop & exit /b 1 )
for /f "tokens=2" %%t in ('rustc -vV ^| findstr /b "host:"') do set "TRIPLE=%%t"
if "%TRIPLE%"=="" ( echo [formdesign] rustc not found; install Rust from https://rustup.rs & exit /b 1 )
if not exist "%ROOT%\native\target\release\formdesign-core.exe" call "%ROOT%\setup\install_core.bat" || exit /b 1
if not exist "%ROOT%\frontend\dist\index.html" call "%ROOT%\setup\build_frontend.bat" || exit /b 1
if not exist "%ROOT%\backend\app\fonts\fonts.json" call "%ROOT%\setup\install_fonts.bat"
cd /d "%ROOT%\backend"
set "DATA=--add-data "%ROOT%\frontend\dist;frontend_dist" --add-data "%ROOT%\backend\app\templates\library;templates/library" --add-data "%ROOT%\backend\app\assets;app/assets""
if exist "%ROOT%\backend\app\fonts" set "DATA=!DATA! --add-data "%ROOT%\backend\app\fonts;fonts""
if exist build\pyinstaller rmdir /s /q build\pyinstaller
if exist dist-engine rmdir /s /q dist-engine
pyinstaller --noconfirm --clean --onefile --name formdesign-engine --distpath dist-engine --workpath build\pyinstaller --specpath build\pyinstaller !DATA! --collect-submodules app --collect-submodules uvicorn --collect-submodules keyring --collect-all pypdfium2 --collect-all pypdfium2_raw --collect-all ezdxf --hidden-import docx --hidden-import openpyxl --hidden-import striprtf "%ROOT%\backend\formdesign_engine.py" || exit /b 1
if not exist "%ROOT%\src-tauri\binaries" mkdir "%ROOT%\src-tauri\binaries"
copy /y "dist-engine\formdesign-engine.exe" "%ROOT%\src-tauri\binaries\formdesign-engine-%TRIPLE%.exe" >nul
copy /y "%ROOT%\native\target\release\formdesign-core.exe" "%ROOT%\src-tauri\binaries\formdesign-core-%TRIPLE%.exe" >nul
"%ROOT%\src-tauri\binaries\formdesign-engine-%TRIPLE%.exe" --selfcheck || exit /b 1
echo [formdesign] sidecars ready in src-tauri\binaries
