@echo off
rem Build Shinatama and assemble the complete, directly usable package in .\release
rem Usage: build-release.bat   (then run release\Shinatama.cmd)
setlocal
cd /d "%~dp0"

where pwsh >nul 2>nul || (echo PowerShell 7.4+ ^(pwsh^) is required. & goto fail)
where npm >nul 2>nul || (echo npm is required. & goto fail)
where cargo >nul 2>nul || (echo cargo is required. & goto fail)

echo [1/4] Frontend build
call npm run build || goto fail

echo [2/4] Native build ^(desktop, session-helper, hook, terminal-helper^)
cargo build --locked -p shinatama-desktop --features custom-protocol || goto fail
cargo build --locked -p shinatama-engine --bins || goto fail
cargo build --locked -p shinatama-terminal --bins || goto fail

echo [3/4] Verified package
if not exist ".verification" mkdir ".verification"
pwsh -NoProfile -File "scripts\package-local.ps1" > ".verification\last-package.json" || goto fail
for /f "usebackq delims=" %%D in (`pwsh -NoProfile -Command "(Get-Content -Raw '.verification\last-package.json' | ConvertFrom-Json).directory"`) do set "STAGE=%%D"
if not defined STAGE (echo Package directory not found in .verification\last-package.json & goto fail)

echo [4/4] Copy to release
robocopy "%STAGE%" "%~dp0release" /MIR /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 (echo Copy to release failed. & goto fail)

echo.
echo OK. Package %STAGE%
echo Ready: %~dp0release\Shinatama.cmd
exit /b 0

:fail
echo.
echo BUILD FAILED.
exit /b 1
