@echo off
rem Shinatama: build Shinatama (+ its Mukade program), assemble the verified package in .\release with
rem the Mukade terminal browser, Xiox and Marionette downloaded from their pinned GitHub releases,
rem install into D:\Software\Shinatama, clean what the build staged.
rem
rem   build-release.bat                   build, .\release, install, clean
rem   build-release.bat --no-install      stop at .\release
rem   build-release.bat --target <dir>    install folder (default D:\Software\Shinatama)
rem   build-release.bat --ci              npm ci even when node_modules is already there
rem   build-release.bat --jobs N          parallel Rust compilations (CARGO_BUILD_JOBS)
rem
rem The same script, with the same steps and options, is in Shinatama, Mukade, Xiox, Muro and
rem Ethel: only the PROJECT parts (the settings below and the subroutines at the end) differ.
rem   1. tools, version, npm dependencies in NPM_DIR (npm ci only when node_modules is missing, or --ci)
rem   2. build                                                          PROJECT :build
rem   3. .\release made from nothing: the programs and their files      PROJECT :assemble
rem      then release\installer\<App>-<version>-setup.exe from it (.github\scripts\installer.iss,
rem      Inno Setup 6; without it a build here goes on with no installer, in CI it is an error)
rem   4. install: .\release is copied over D:\Software\Shinatama, made if missing. What only the
rem      install holds (data, mukade-data, webview ...) stays as it is; nothing is backed up.
rem      release\installer\, where there is one, holds installers to hand out: it is not installed.
rem      While a program of the install folder is running, it waits for it to close.
rem   5. clean: what the build staged is removed, on success and on failure. PROJECT :clean_project
rem      Cargo's target\ stays (the build cache: the next build is incremental), node_modules stays.
rem
rem Errors always leave through a label: with setlocal on, "exit /b 1" inside a parenthesized
rem block returns 0 to whoever ran the script.

setlocal EnableExtensions
set "ROOT=%~dp0"
set "SELF=%~nx0"
set "RELEASE=%ROOT%release"
cd /d "%ROOT%" || goto :fail

rem ==== PROJECT: settings ========================================================================
set "APP=Shinatama"
rem The program that must be in .\release once it is assembled.
set "APP_EXE=Shinatama.exe"
rem Tools beyond cargo, node, npm and git: PowerShell 7.4+ for the package scripts, gh (GitHub
rem CLI, signed in) for the Mukade, Xiox and Marionette releases.
set "TOOLS=pwsh gh"
rem Parallel Rust jobs when neither CARGO_BUILD_JOBS nor --jobs is given (empty: cargo decides).
set "DEFAULT_JOBS="
rem The folder (relative to the repo) with package.json and node_modules.
set "NPM_DIR=."
rem ===============================================================================================

set "INSTALL=D:\Software\%APP%"
set "DO_INSTALL=1"
set "NPM_CI=0"

:args
if "%~1"=="" goto :args_done
if /i "%~1"=="--no-install" (set "DO_INSTALL=0" & shift & goto :args)
if /i "%~1"=="--ci" (set "NPM_CI=1" & shift & goto :args)
if /i "%~1"=="--target" goto :arg_target
if /i "%~1"=="--jobs" goto :arg_jobs
goto :usage
:arg_target
if "%~2"=="" goto :usage
set "INSTALL=%~f2"
shift & shift & goto :args
:arg_jobs
if "%~2"=="" goto :usage
set "CARGO_BUILD_JOBS=%~2"
shift & shift & goto :args
:args_done
if "%INSTALL:~-1%"=="\" set "INSTALL=%INSTALL:~0,-1%"
if not defined CARGO_BUILD_JOBS if defined DEFAULT_JOBS set "CARGO_BUILD_JOBS=%DEFAULT_JOBS%"

rem --- 1. Tools and dependencies -----------------------------------------------------------------
echo.
echo [1/5] Tools and dependencies
for %%T in (cargo node npm git %TOOLS%) do (
  where %%T >nul 2>nul || (echo [ERROR] %%T not found in PATH. & goto :fail)
)
rem Tauri's build scripts compile the Windows resources with rc.exe (embed-resource). Its own Windows SDK
rem lookup can name an rc.exe that is not there, and only RC_<target> or RC come before it: unless
rem RC_x86_64_pc_windows_msvc is already set, point it at the rc.exe of the newest SDK that has one.
if not defined RC_x86_64_pc_windows_msvc call :find_rc
set "VERSION=?"
rem The version of the release system: its first version file (.github\release.json).
for /f "usebackq delims=" %%V in (`node .github\scripts\release.mjs current`) do set "VERSION=%%V"
set "COMMIT=?"
for /f "usebackq delims=" %%C in (`git rev-parse --short HEAD`) do set "COMMIT=%%C"
set "DIRTY="
git diff --quiet HEAD -- 2>nul || set "DIRTY=, uncommitted changes"
echo     %APP% %VERSION%, commit %COMMIT%%DIRTY%
if defined CARGO_BUILD_JOBS echo     %CARGO_BUILD_JOBS% parallel Rust jobs
if "%DO_INSTALL%"=="1" echo     install into %INSTALL%
if not "%DO_INSTALL%"=="1" echo     no install: .\release only

if "%NPM_CI%"=="1" goto :npm_ci
if exist "%NPM_DIR%\node_modules\.package-lock.json" goto :npm_done
:npm_ci
pushd "%NPM_DIR%" || goto :fail
call npm ci
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :fail
:npm_done

rem --- 2. Build ----------------------------------------------------------------------------------
echo.
echo [2/5] Build
call :build
if errorlevel 1 goto :fail

rem --- 3. Release folder -------------------------------------------------------------------------
echo.
echo [3/5] %RELEASE%
if exist "%RELEASE%\" rmdir /s /q "%RELEASE%"
if exist "%RELEASE%\" goto :release_busy
call :assemble
if errorlevel 1 goto :fail
if not exist "%RELEASE%\%APP_EXE%" goto :release_missing
call :installer
if errorlevel 1 goto :fail

rem --- 4. Install --------------------------------------------------------------------------------
echo.
if "%DO_INSTALL%"=="1" goto :do_install
echo [4/5] Install skipped ^(--no-install^)
goto :installed
:do_install
echo [4/5] Install into %INSTALL%
call :install
if errorlevel 1 goto :fail
:installed

rem --- 5. Clean ----------------------------------------------------------------------------------
echo.
echo [5/5] Clean
call :clean_project

echo.
echo === Done: %APP% %VERSION% (%COMMIT%%DIRTY%)
echo     release    %RELEASE%
if "%DO_INSTALL%"=="1" echo     installed  %INSTALL%\%APP_EXE%
call :pause_if_clicked
endlocal
exit /b 0

rem --- Common subroutines ------------------------------------------------------------------------

:install
if not exist "%INSTALL%\" mkdir "%INSTALL%" || goto :sub_fail
set "SHOWN="
:install_wait
set "RUNNING=0"
for /f "usebackq delims=" %%N in (`powershell -NoProfile -NonInteractive -Command "$d = [IO.Path]::GetFullPath('%INSTALL%') + '\'; @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($d, [StringComparison]::OrdinalIgnoreCase) }).Count"`) do set "RUNNING=%%N"
if "%RUNNING%"=="0" goto :install_copy
if not defined SHOWN echo     %APP% is running from %INSTALL%: close it, the install goes on by itself.
set "SHOWN=1"
ping -n 4 127.0.0.1 >nul
goto :install_wait
:install_copy
robocopy "%RELEASE%" "%INSTALL%" /E /XD "%RELEASE%\installer" /R:10 /W:2 /NFL /NDL /NJH /NP
if errorlevel 8 (echo [ERROR] copy into %INSTALL% failed: a file may still be in use. & goto :sub_fail)
exit /b 0

:installer
rem release\installer\<App>-<version>-setup.exe from release\ (.github\scripts\installer.iss, Inno Setup 6). Without
rem Inno Setup a build on this machine goes on with no installer; in CI it is an error: the release hands it out.
set "ISCC="
for /f "delims=" %%I in ('where ISCC.exe 2^>nul') do if not defined ISCC set "ISCC=%%I"
for %%I in ("%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe" "%ProgramFiles%\Inno Setup 6\ISCC.exe" "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe") do if not defined ISCC if exist "%%~I" set "ISCC=%%~I"
if defined ISCC goto :installer_build
if defined CI (echo [ERROR] Inno Setup 6 ^(ISCC.exe^) not found: the release needs its installer. & goto :sub_fail)
echo     installer skipped: Inno Setup 6 not found ^(winget install JRSoftware.InnoSetup^)
exit /b 0
:installer_build
"%ISCC%" /Q "/DApp=%APP%" "/DVersion=%VERSION%" "/DExe=%APP_EXE%" "/DSource=%RELEASE%" "/O%RELEASE%\installer" "%ROOT%.github\scripts\installer.iss"
if errorlevel 1 (echo [ERROR] the installer was not built. & goto :sub_fail)
for %%F in ("%RELEASE%\installer\*") do echo     installer\%%~nxF  %%~zF bytes
exit /b 0

:find_rc
set "KITS=%ProgramFiles(x86)%\Windows Kits\10\bin"
set "RC_DIR="
for /f "delims=" %%D in ('dir /b /ad /o-n "%KITS%\10.*" 2^>nul') do if not defined RC_DIR if exist "%KITS%\%%D\x64\rc.exe" set "RC_DIR=%KITS%\%%D\x64"
if not defined RC_DIR echo     rc.exe not found in a Windows SDK: Tauri builds that recompile their resources will fail.
if defined RC_DIR set "RC_x86_64_pc_windows_msvc=%RC_DIR%\rc.exe"
if defined RC_DIR echo     rc.exe: %RC_DIR%\rc.exe
exit /b 0

:pause_if_clicked
rem Started with a double-click from Explorer: keep the window open to read the result. Never in CI.
if defined CI exit /b 0
echo %CMDCMDLINE% | "%SystemRoot%\System32\find.exe" /i "%SELF%" >nul && pause
exit /b 0

:sub_fail
exit /b 1

:release_busy
echo [ERROR] %RELEASE% could not be removed: a file in it is in use.
goto :fail

:release_missing
echo [ERROR] %RELEASE%\%APP_EXE% was not produced.
goto :fail

:usage
echo [ERROR] invalid arguments: %*
echo Usage: %SELF% [--no-install] [--target ^<folder^>] [--ci] [--jobs N]
call :pause_if_clicked
endlocal
exit /b 1

:fail
echo.
echo [ERROR] %APP% build-release stopped: read the error above.
call :clean_project
call :pause_if_clicked
endlocal
exit /b 1

rem ==== PROJECT: build, release folder, clean ===================================================
rem Each subroutine returns 0 when done; errors go to :sub_fail. The working folder is the repo.

:build
set "STAGE="
echo     frontend
call npm run build || goto :sub_fail
echo     native ^(desktop, hook, terminal-helper; session-helper in release^)
cargo build --locked -p shinatama-desktop --features custom-protocol || goto :sub_fail
cargo build --locked -p shinatama-engine --features session-reader --bins || goto :sub_fail
rem The session helper of a package is a release build (ADR-0053 v2, INV-006).
cargo build --release --locked -p shinatama-engine --features session-reader --bin shinatama-session-helper || goto :sub_fail
cargo build --locked -p shinatama-terminal --bins || goto :sub_fail
cargo build --locked -p shinatama-cli || goto :sub_fail
echo     mukade.exe ^(embedded: chat service, hooks and session CLI^)
rem mukade.exe is Shinatama's own program (crates/mukade): Mukade's command line from the revision
rem pinned in Cargo.toml plus Shinatama's `shinatama-kit` command, so it is built here, not
rem downloaded. Built on its own, so the chat feature is not unified into the engine's session reader.
cargo build --release --locked -p shinatama-mukade || goto :sub_fail
echo     verified package
if not exist ".verification" mkdir ".verification"
pwsh -NoProfile -File "scripts\package-local.ps1" > ".verification\last-package.json" || goto :sub_fail
for /f "usebackq delims=" %%D in (`pwsh -NoProfile -Command "(Get-Content -Raw '.verification\last-package.json' | ConvertFrom-Json).directory"`) do set "STAGE=%%D"
if not defined STAGE (echo [ERROR] package directory not found in .verification\last-package.json. & goto :sub_fail)
echo     %STAGE%
exit /b 0

:assemble
robocopy "%STAGE%" "%RELEASE%" /E /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 (echo [ERROR] copy of the package into release failed. & goto :sub_fail)

rem The product exe is Shinatama.exe; helpers stay beside it under their own names.
if exist "%RELEASE%\Shinatama.cmd" del /q "%RELEASE%\Shinatama.cmd"
move /y "%RELEASE%\shinatama-desktop.exe" "%RELEASE%\Shinatama.exe" >nul || (echo [ERROR] could not create Shinatama.exe. & goto :sub_fail)
pwsh -NoProfile -Command "$p='%RELEASE%\AVVIO.md'; (Get-Content -Raw -LiteralPath $p) -replace 'Shinatama\.cmd','Shinatama.exe' | Set-Content -NoNewline -LiteralPath $p"

rem The command line (shinatama.exe) lives in cli\ so it never collides with Shinatama.exe;
rem it needs the session helper beside it for sessions/find/export.
mkdir "%RELEASE%\cli" || goto :sub_fail
copy /y "target\debug\shinatama.exe" "%RELEASE%\cli\shinatama.exe" >nul || (echo [ERROR] could not copy the CLI. & goto :sub_fail)
copy /y "target\release\shinatama-session-helper.exe" "%RELEASE%\cli\shinatama-session-helper.exe" >nul || (echo [ERROR] could not copy the CLI session helper. & goto :sub_fail)

echo     mukade\ ^(Shinatama's mukade.exe; the terminal browser of the pinned Mukade release^)
mkdir "%RELEASE%\mukade" || goto :sub_fail
copy /y "target\release\mukade.exe" "%RELEASE%\mukade\mukade.exe" >nul || goto :sub_fail
rem The terminal browser is Mukade's, unchanged: the pinned release's file (scripts\mukade-pin.json).
pwsh -NoProfile -File "scripts\fetch-release.ps1" -Pin "scripts\mukade-pin.json" -Destination "%RELEASE%\mukade" || goto :sub_fail

echo     xiox\ ^(observer of what sessions leave: worktrees, copies, bench and backup folders^)
rem The pinned release's xiox.exe (scripts\xiox-pin.json); no Xiox source is built. Xiox is given
rem Shinatama's mukade\mukade.exe (XIOX_MUKADE_EXE), so no second Mukade goes beside it.
pwsh -NoProfile -File "scripts\fetch-release.ps1" -Pin "scripts\xiox-pin.json" -Destination "%RELEASE%\xiox" || goto :sub_fail

echo     marionette\ ^(the pinned Marionette release, there from the first start, offline too^)
rem The zip of the marionette-v<version> release of acaprino/marionette that scripts\marionette-pin.json names,
rem checked against its SHA-256 and size, with that pin beside it. At start, before the sync, the app
rem installs it into the data folder when that holds no intact copy at least as new (ADR-0052 v2).
pwsh -NoProfile -File "scripts\fetch-release.ps1" -Pin "scripts\marionette-pin.json" -Destination "%RELEASE%\marionette" || goto :sub_fail
copy /y "scripts\marionette-pin.json" "%RELEASE%\marionette\marionette-pin.json" >nul || (echo [ERROR] could not copy the Marionette pin. & goto :sub_fail)

rem Muro, which keeps the plugins of Shinatama's agents in sync (docs\specs\muro-plugin-sync.md): the
rem portable zip of the release scripts\muro-pin.json names, checked and unpacked whole into muro\. No
rem Muro release has what Shinatama needs yet, so there is no pin and no muro\ until one does; the app
rem then says Muro is not in this build.
if not exist "scripts\muro-pin.json" goto :no_muro
echo     muro\ ^(the pinned Muro release, unpacked: Muro.exe, tools\muroctl.exe, tools\mukade.exe^)
pwsh -NoProfile -File "scripts\fetch-release.ps1" -Pin "scripts\muro-pin.json" -Destination "%RELEASE%\muro" || goto :sub_fail
:no_muro

echo     %RELEASE%\Shinatama.exe
echo     %RELEASE%\cli\shinatama.exe ^(sessions, find, export, forget, check-state, mcp^)
echo     %RELEASE%\mukade\mukade.exe and mukade-tui.exe
echo     %RELEASE%\xiox\xiox.exe
echo     %RELEASE%\marionette\ ^(marionette-pin.json and the zip it names^)
exit /b 0

:clean_project
rem The stage scripts\package-local.ps1 made for this build in .verification\portable, its receipt
rem and the record naming it: .\release holds the package now. Stages of earlier builds and the
rem other evidence in .verification are not touched.
if defined STAGE if exist "%STAGE%\" rmdir /s /q "%STAGE%"
if defined STAGE if exist "%STAGE%.receipt.json" del /q "%STAGE%.receipt.json"
if exist ".verification\last-package.json" del /q ".verification\last-package.json"
exit /b 0
