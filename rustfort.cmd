@echo off
rem Rustfort launcher: builds the game when something changed, then starts it.
rem   rustfort.cmd                              title screen (Continue, Load, Import from DF, Settings)
rem   rustfort.cmd play --load saves\X.rfsave   any rustfort arguments are passed through
rem Saves go to saves\ and DF imports to export\ next to this file. A Steam install of
rem Dwarf Fortress is found automatically; otherwise choose its folder in Settings or set
rem RUSTFORT_DF_DIR.
setlocal
cd /d "%~dp0"
rem cargo builds into CARGO_TARGET_DIR when that is set, else into target\ here.
set "EXE=target\release\rustfort.exe"
if defined CARGO_TARGET_DIR set "EXE=%CARGO_TARGET_DIR%\release\rustfort.exe"
where cargo >nul 2>nul || set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul && (
  echo Building Rustfort - only changed parts are compiled ...
  cargo build --release -p rf-app || echo Build failed - starting the last working build, if there is one.
)
if not exist "%EXE%" (
  echo Rustfort is not built yet, and Rust - cargo - was not found or the build failed.
  echo Install Rust from https://rustup.rs and run rustfort.cmd again.
  pause
  exit /b 1
)
"%EXE%" %*
set "RC=%errorlevel%"
rem Keep an error message readable when started by double-click; pass the exit code on.
if not "%RC%"=="0" pause
exit /b %RC%
