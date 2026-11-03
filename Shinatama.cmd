@echo off
setlocal
if not exist "%~dp0target\debug\shinatama-desktop.exe" goto missing
if not exist "%~dp0target\debug\shinatama-terminal-helper.exe" goto missing
if not exist "%~dp0target\debug\shinatama-session-helper.exe" goto missing
if not exist "%~dp0target\debug\shinatama-hook.exe" goto missing
start "" /d "%~dp0" "%~dp0target\debug\shinatama-desktop.exe"
exit /b 0
:missing
echo La build locale di Shinatama non e ancora disponibile.
echo Compilare l'app prima di usare questo collegamento.
pause
exit /b 1
