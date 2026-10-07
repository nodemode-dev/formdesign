@echo off
setlocal
cd /d "%~dp0..\native"
where cargo >nul 2>nul || ( echo [formdes] the layout core needs Rust: install it from https://rustup.rs and rerun formdes.bat install-core & exit /b 1 )
echo [formdes] building formdes-core (the first build compiles the layout engine: a few minutes)
cargo build --release -p formdes-core || exit /b 1
echo [formdes] layout core ready: native\target\release\formdes-core.exe
