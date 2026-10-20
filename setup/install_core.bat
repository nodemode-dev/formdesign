@echo off
setlocal
cd /d "%~dp0..\native"
where cargo >nul 2>nul || ( echo [formdesign] the layout core needs Rust: install it from https://rustup.rs and rerun formdesign.bat install-core & exit /b 1 )
echo [formdesign] building formdesign-core (the first build compiles the layout engine: a few minutes)
cargo build --release -p formdesign-core || exit /b 1
echo [formdesign] layout core ready: native\target\release\formdesign-core.exe
