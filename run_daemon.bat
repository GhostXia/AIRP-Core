@echo off
title AIRP-Core Daemon SSE Gateway

REM Set environment variables for compilation and paths
set RUSTUP_HOME=D:\.rustup
set PATH=D:\msys64\mingw64\bin;%PATH%

echo ===================================================
echo   AIRP-Core Streaming RP Backend
echo   Compiling latest changes and launching daemon...
echo ===================================================
echo.

REM 1. Compile and run daemon (incrementally builds only modified files)
cargo run --release -- daemon --port 8000

REM 2. Automatically copy the compiled executable to root directory for easy access
if exist target\release\airp-core.exe (
    copy target\release\airp-core.exe . >nul
    echo [Success] Latest airp-core.exe has been copied to the root directory!
)

echo.
pause
