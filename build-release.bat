@echo off
rem Build Shinatama (+ Mukade) and assemble everything, directly usable, in .\release
rem Usage: build-release.bat   (then run release\Shinatama.exe)
setlocal
cd /d "%~dp0"

where pwsh >nul 2>nul || (echo PowerShell 7.4+ ^(pwsh^) is required. & goto fail)
where npm >nul 2>nul || (echo npm is required. & goto fail)
where cargo >nul 2>nul || (echo cargo is required. & goto fail)

echo [1/5] Frontend build
call npm run build || goto fail

echo [2/5] Native build ^(desktop, hook, terminal-helper; session-helper in release^)
cargo build --locked -p shinatama-desktop --features custom-protocol || goto fail
cargo build --locked -p shinatama-engine --features session-reader --bins || goto fail
rem The session helper of a package is a release build (ADR-0053 v2, INV-006).
cargo build --release --locked -p shinatama-engine --features session-reader --bin shinatama-session-helper || goto fail
cargo build --locked -p shinatama-terminal --bins || goto fail
cargo build --locked -p shinatama-cli || goto fail

echo [3/5] Verified package
if not exist ".verification" mkdir ".verification"
pwsh -NoProfile -File "scripts\package-local.ps1" > ".verification\last-package.json" || goto fail
for /f "usebackq delims=" %%D in (`pwsh -NoProfile -Command "(Get-Content -Raw '.verification\last-package.json' | ConvertFrom-Json).directory"`) do set "STAGE=%%D"
if not defined STAGE (echo Package directory not found in .verification\last-package.json & goto fail)

echo [4/5] Copy to release
robocopy "%STAGE%" "%~dp0release" /MIR /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 (echo Copy to release failed. & goto fail)

rem The product exe is Shinatama.exe; helpers stay beside it under their own names.
if exist "%~dp0release\Shinatama.cmd" del /q "%~dp0release\Shinatama.cmd"
if exist "%~dp0release\Shinatama.exe" del /q "%~dp0release\Shinatama.exe"
move /y "%~dp0release\shinatama-desktop.exe" "%~dp0release\Shinatama.exe" >nul || (echo Could not create Shinatama.exe. & goto fail)
pwsh -NoProfile -Command "$p='%~dp0release\AVVIO.md'; (Get-Content -Raw -LiteralPath $p) -replace 'Shinatama\.cmd','Shinatama.exe' | Set-Content -NoNewline -LiteralPath $p"

rem The command line (shinatama.exe) lives in cli\ so it never collides with Shinatama.exe;
rem it needs the session helper beside it for sessions/find/export.
if not exist "%~dp0release\cli" mkdir "%~dp0release\cli"
copy /y "%~dp0target\debug\shinatama.exe" "%~dp0release\cli\shinatama.exe" >nul || (echo Could not copy the CLI. & goto fail)
copy /y "%~dp0target\release\shinatama-session-helper.exe" "%~dp0release\cli\shinatama-session-helper.exe" >nul || (echo Could not copy the CLI session helper. & goto fail)

echo [5/5] Mukade ^(session library CLI and terminal browser^)
set "MUKADE=%~dp0..\mukade"
if not exist "%MUKADE%\build.bat" (echo Mukade not found at "%MUKADE%". & goto fail)
call "%MUKADE%\build.bat" || goto fail
if not exist "%~dp0release\mukade" mkdir "%~dp0release\mukade"
copy /y "%MUKADE%\target\release\mukade.exe" "%~dp0release\mukade\mukade.exe" >nul || goto fail
copy /y "%MUKADE%\target\release\mukade-tui.exe" "%~dp0release\mukade\mukade-tui.exe" >nul || goto fail

echo.
echo OK. Package %STAGE%
echo Ready: %~dp0release\Shinatama.exe
echo        %~dp0release\cli\shinatama.exe ^(sessions, find, export, forget, check-state, mcp^)
echo        %~dp0release\mukade\mukade.exe and mukade-tui.exe
exit /b 0

:fail
echo.
echo BUILD FAILED.
exit /b 1
