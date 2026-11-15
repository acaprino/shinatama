@echo off
rem Install the last build (.\release, made by build-release.bat) over an existing Shinatama.
rem Usage: install-release.bat [install folder]   (default D:\Software\Shinatama)
rem Copies only what release holds: data, mukade-data, webview and other local folders are left as they are.
rem No backup is made: the install is updated in place.
setlocal
cd /d "%~dp0"
set "TARGET=%~1"
if not defined TARGET set "TARGET=D:\Software\Shinatama"

if not exist "release\Shinatama.exe" (echo No build in "%~dp0release". Run build-release.bat first. & goto fail)
if not exist "%TARGET%\Shinatama.exe" (echo No Shinatama install in "%TARGET%". & goto fail)

:wait
tasklist /FI "IMAGENAME eq Shinatama.exe" 2>nul | find /I "Shinatama.exe" >nul
if not errorlevel 1 (
  echo Shinatama is running. Close it, then press a key to install.
  pause >nul
  goto wait
)

echo Installing "%~dp0release" into "%TARGET%"...
robocopy "%~dp0release" "%TARGET%" /E /R:10 /W:2 /NFL /NDL /NJH /NP
if errorlevel 8 (echo Copy failed: a file may still be in use. Close Shinatama's helpers and run this again. & goto fail)

echo.
echo OK. Installed. Start it with "%TARGET%\Shinatama.exe".
exit /b 0

:fail
echo.
echo INSTALL FAILED.
exit /b 1
