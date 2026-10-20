@echo off
setlocal
cd /d "%~dp0.."
echo [formdesign] installing the engine
pushd backend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdesign] installing the font library
call setup\install_fonts.bat || echo [formdesign] the font library did not install, retry with formdesign.bat install-fonts
echo [formdesign] building the layout core
call setup\install_core.bat || exit /b 1
echo [formdesign] installing the frontend
pushd frontend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdesign] done. Boot with formdesign.bat
