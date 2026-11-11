@echo off
setlocal
cd /d "%~dp0.."
echo [formdesign] installing the engine
pushd backend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdesign] installing the font library
call setup\install_fonts.bat || echo [formdesign] the font library did not install, retry with formdesign.bat install-fonts
if /i not "%~1"=="--lite" goto :core
if exist native\target\release\formdesign-core.exe goto :kept
if exist native\bin\formdesign-core.exe goto :kept
:core
echo [formdesign] building the layout core
call setup\install_core.bat || exit /b 1
goto :frontend
:kept
echo [formdesign] keeping the layout core already built (--lite)
:frontend
echo [formdesign] installing the frontend
pushd frontend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdesign] done. Boot with formdesign.bat
