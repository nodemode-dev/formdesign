@echo off
setlocal enabledelayedexpansion
set "ROOT=%~dp0.."
if not exist "%ROOT%\backend\venv\Scripts\activate.bat" ( echo [formdes] engine venv missing, run formdes.bat install & exit /b 1 )
call "%ROOT%\backend\venv\Scripts\activate.bat"
python -c "import PyInstaller" 2>nul || ( echo [formdes] PyInstaller missing, run formdes.bat install-desktop & exit /b 1 )
for /f "tokens=2" %%t in ('rustc -vV ^| findstr /b "host:"') do set "TRIPLE=%%t"
if "%TRIPLE%"=="" ( echo [formdes] rustc not found; install Rust from https://rustup.rs & exit /b 1 )
if not exist "%ROOT%\native\target\release\formdes-core.exe" call "%ROOT%\setup\install_core.bat" || exit /b 1
if not exist "%ROOT%\frontend\dist\index.html" call "%ROOT%\setup\build_frontend.bat" || exit /b 1
if not exist "%ROOT%\backend\app\fonts\fonts.json" call "%ROOT%\setup\install_fonts.bat"
cd /d "%ROOT%\backend"
set "DATA=--add-data "%ROOT%\frontend\dist;frontend_dist" --add-data "%ROOT%\backend\app\templates\library;templates/library""
if exist "%ROOT%\backend\app\fonts" set "DATA=!DATA! --add-data "%ROOT%\backend\app\fonts;fonts""
if exist build\pyinstaller rmdir /s /q build\pyinstaller
if exist dist-engine rmdir /s /q dist-engine
pyinstaller --noconfirm --clean --onefile --name formdes-engine --distpath dist-engine --workpath build\pyinstaller --specpath build\pyinstaller !DATA! --collect-submodules app --collect-submodules uvicorn --collect-submodules keyring --collect-all pypdfium2 --collect-all pypdfium2_raw --collect-all ezdxf --hidden-import docx --hidden-import openpyxl --hidden-import striprtf "%ROOT%\backend\formdes_engine.py" || exit /b 1
if not exist "%ROOT%\src-tauri\binaries" mkdir "%ROOT%\src-tauri\binaries"
copy /y "dist-engine\formdes-engine.exe" "%ROOT%\src-tauri\binaries\formdes-engine-%TRIPLE%.exe" >nul
copy /y "%ROOT%\native\target\release\formdes-core.exe" "%ROOT%\src-tauri\binaries\formdes-core-%TRIPLE%.exe" >nul
"%ROOT%\src-tauri\binaries\formdes-engine-%TRIPLE%.exe" --selfcheck || exit /b 1
echo [formdes] sidecars ready in src-tauri\binaries
