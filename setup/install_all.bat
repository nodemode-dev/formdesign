@echo off
setlocal
cd /d "%~dp0.."
echo [formdes] installing the engine
pushd backend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdes] installing the font library
call setup\install_fonts.bat || echo [formdes] the font library did not install, retry with formdes.bat install-fonts
echo [formdes] building the layout core
call setup\install_core.bat || exit /b 1
echo [formdes] installing the frontend
pushd frontend
call oc_install.bat || ( popd & exit /b 1 )
popd
echo [formdes] done. Boot with formdes.bat
