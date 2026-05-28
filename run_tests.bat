@echo off
title AIRP-Core Test Runner

REM Set environment variables
set RUSTUP_HOME=D:\.rustup
set PATH=D:\msys64\mingw64\bin;%PATH%

echo ===================================================
echo   AIRP-Core - Running unit tests...
echo ===================================================
echo.

cargo test

echo.
pause
