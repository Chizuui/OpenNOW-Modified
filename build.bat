@echo off
setlocal enabledelayedexpansion
cd /d "%~dp0"

:: ============================================================
:: OpenNOW Windows Builder
::   build.bat [stable | gstreamer | standalone | standalone-nogst | clean | config]
:: ============================================================

set "CONFIG_FILE=%~dp0build-config.bat"

:: ------------------------------------------------
:: Load persisted config (written by the config menu)
:: ------------------------------------------------
if exist "%CONFIG_FILE%" (
    call "%CONFIG_FILE%"
)

:: ------------------------------------------------
:: Auto-detect the GStreamer SDK.
:: A usable SDK must have BOTH bin\pkg-config.exe (or pkgconf.exe)
:: AND lib\pkgconfig\gstreamer-1.0.pc. The runtime-only installs
:: (e.g. "D:\Program Files (x86)\GStreamer\msvc_x86_64") do NOT ship
:: pkg-config and CANNOT build the native streamer — always prefer a
:: full SDK like "D:\SDK\GStreamer\1.0\msvc_x86_64".
:: ------------------------------------------------
if not defined GST_PATH set "GST_PATH="
call :detect_gst
if defined GST_PATH (
    set "GSTREAMER_1_0_ROOT_MSVC_X86_64=%GST_PATH%"
    set "PATH=%GST_PATH%\bin;%PATH%"
    echo [GStreamer] SDK  : !GST_PATH! ^(gstreamer-1.0 detected^)
) else (
    echo [GStreamer] WARNING: no full GStreamer SDK found ^(needs bin\pkg-config.exe + lib\pkgconfig\gstreamer-1.0.pc^).
    echo [GStreamer] Run "build.bat config" to point it at your SDK, or install the MSVC SDK.
)

if "%~1"=="" goto menu
if /i "%~1"=="stable" goto build_stable
if /i "%~1"=="gstreamer" goto build_gstreamer
if /i "%~1"=="standalone" goto build_standalone
if /i "%~1"=="standalone-nogst" goto build_standalone_nogst
if /i "%~1"=="clean" goto build_clean
if /i "%~1"=="config" goto config_menu
goto usage

:usage
echo Usage: build.bat [stable ^| gstreamer ^| standalone ^| standalone-nogst ^| clean ^| config]
exit /b 1

:menu
cls
echo ===================================================
echo               OpenNOW Windows Builder
echo ===================================================
echo  1. Build Stable (Electron Client)
echo  2. Build Native Streamer (Rust + GStreamer)
echo  3. Build Standalone Installer (Bundled GStreamer)
echo  4. Build Standalone Installer (No GStreamer Bundle)
echo  5. Clean Build Artifacts
echo  6. Configure GStreamer Path
echo  7. Exit
echo ===================================================
if defined GST_PATH echo  GStreamer SDK: !GST_PATH!
echo ===================================================
set /p choice="Choose an option (1-7): "
if "%choice%"=="1" goto build_stable
if "%choice%"=="2" goto build_gstreamer
if "%choice%"=="3" goto build_standalone
if "%choice%"=="4" goto build_standalone_nogst
if "%choice%"=="5" goto build_clean
if "%choice%"=="6" goto config_menu
if "%choice%"=="7" exit /b 0
goto menu

:: ------------------------------------------------
:: GStreamer SDK detection: %1 = candidate path (quoted-safe via %~1)
:: Sets GST_PATH to the first valid SDK, leaves it unchanged if none.
:: ------------------------------------------------
:detect_gst
if defined GST_PATH (
    if exist "%GST_PATH%\bin\pkg-config.exe" if exist "%GST_PATH%\lib\pkgconfig\gstreamer-1.0.pc" goto :eof
    if exist "%GST_PATH%\bin\pkgconf.exe"    if exist "%GST_PATH%\lib\pkgconfig\gstreamer-1.0.pc" goto :eof
    echo [GStreamer] Configured GST_PATH lacks the build toolchain, searching for a full SDK...
    set "GST_PATH="
)
set "GST_PATH="
call :try_gst "D:\SDK\GStreamer\1.0\msvc_x86_64"
call :try_gst "D:\Program Files (x86)\GStreamer\1.0\msvc_x86_64"
call :try_gst "D:\GStreamer\1.0\msvc_x86_64"
call :try_gst "C:\Program Files\gstreamer\1.0\msvc_x86_64"
call :try_gst "C:\gstreamer\1.0\msvc_x86_64"
call :try_gst "D:\Program Files (x86)\GStreamer\msvc_x86_64"
call :try_gst "C:\Program Files\GStreamer\1.0\msvc_x86_64"
goto :eof

:try_gst
if defined GST_PATH goto :eof
if exist "%~1\bin\pkg-config.exe" if exist "%~1\lib\pkgconfig\gstreamer-1.0.pc" set "GST_PATH=%~1"
if defined GST_PATH goto :eof
if exist "%~1\bin\pkgconf.exe" if exist "%~1\lib\pkgconfig\gstreamer-1.0.pc" set "GST_PATH=%~1"
goto :eof

:: ------------------------------------------------
:: Persist the configured SDK path (and future flags) so it survives restarts.
:: ------------------------------------------------
:config_menu
cls
echo ===================================================
echo                 Configure Paths Menu
echo ===================================================
echo  Current Settings:
if defined GST_PATH (
    echo  [GStreamer]   : !GST_PATH!
) else (
    echo  [GStreamer]   : (none detected)
)
echo ===================================================
echo  1. Change GStreamer Path
echo  2. Back to Main Menu
echo ===================================================
set /p cfg_choice="Choose an option (1-2): "
if "%cfg_choice%"=="1" (
    set /p GST_PATH="Enter GStreamer Path: "
    if defined GST_PATH (
        > "%CONFIG_FILE%" echo set "GST_PATH=%GST_PATH%"
        if exist "!GST_PATH!\bin\pkg-config.exe" if exist "!GST_PATH!\lib\pkgconfig\gstreamer-1.0.pc" (
            echo [GStreamer] Saved: %GST_PATH%
        ) else (
            echo [GStreamer] WARNING: that path has no pkg-config/^.pc — native build will fail until fixed.
        )
    )
    goto config_menu
)
if "%cfg_choice%"=="2" goto load_config
goto config_menu

:load_config
if "%~1"=="" goto menu
if /i "%~1"=="stable" goto build_stable
if /i "%~1"=="gstreamer" goto build_gstreamer
if /i "%~1"=="standalone" goto build_standalone
if /i "%~1"=="standalone-nogst" goto build_standalone_nogst
if /i "%~1"=="clean" goto build_clean
goto usage

:: ------------------------------------------------
:: npm dependencies check: builds fail instantly without node_modules.
:: ------------------------------------------------
:ensure_npm
if exist "%~dp0opennow-stable\node_modules" goto :eof
echo [npm] opennow-stable\node_modules not found.
set /p do_install="[npm] Run npm install now? (Y/N): "
if /i "%do_install%"=="Y" (
    pushd "%~dp0opennow-stable"
    call npm install
    popd
) else (
    echo [npm] Skipping install — build will likely fail.
)
goto :eof

:build_stable
echo [Build] Building Electron client (renderer + main)...
call :ensure_npm
pushd opennow-stable
call npm run build
popd
echo [Build] Stable build completed. Output: opennow-stable\dist + dist-electron.
pause
goto menu

:build_gstreamer
echo [Build] Building native streamer (Rust + GStreamer)...
call :ensure_npm
pushd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run native:build
popd
echo [Build] Native streamer build completed. Output: native\opennow-streamer\bin.
pause
goto menu

:build_standalone
echo [Build] Building standalone installer with bundled GStreamer runtime...
call :ensure_npm
pushd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run dist
popd
echo [Build] Standalone build completed. Check opennow-stable\dist-release folder.
pause
goto menu

:build_standalone_nogst
echo [Build] Building standalone installer WITHOUT bundled GStreamer runtime...
echo       Note: target machine must have GStreamer installed system-wide.
call :ensure_npm
pushd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_BUNDLE_GSTREAMER_RUNTIME=0"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run dist
popd
echo [Build] Standalone build completed (no GStreamer bundle). Check opennow-stable\dist-release folder.
pause
goto menu

:build_clean
echo [Clean] Cleaning build folders...
if exist "opennow-stable\dist" (
    rmdir /s /q "opennow-stable\dist"
    echo - Removed opennow-stable\dist
)
if exist "opennow-stable\dist-electron" (
    rmdir /s /q "opennow-stable\dist-electron"
    echo - Removed opennow-stable\dist-electron
)
if exist "opennow-stable\dist-release" (
    rmdir /s /q "opennow-stable\dist-release"
    echo - Removed opennow-stable\dist-release
)
if exist "native\opennow-streamer\target" (
    rmdir /s /q "native\opennow-streamer\target"
    echo - Removed native\opennow-streamer\target
)
echo [Clean] Clean completed.
pause
goto menu
