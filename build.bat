@echo off
setlocal enabledelayedexpansion

:: Default Paths
set "GST_PATH=D:\Program Files (x86)\GStreamer\msvc_x86_64"

:load_config
:: Apply GStreamer environment variable if it exists
if exist "%GST_PATH%" (
    set "GSTREAMER_1_0_ROOT_MSVC_X86_64=%GST_PATH%"
    set "PATH=%GST_PATH%\bin;%PATH%"
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
set /p choice="Choose an option (1-7): "
if "%choice%"=="1" goto build_stable
if "%choice%"=="2" goto build_gstreamer
if "%choice%"=="3" goto build_standalone
if "%choice%"=="4" goto build_standalone_nogst
if "%choice%"=="5" goto build_clean
if "%choice%"=="6" goto config_menu
if "%choice%"=="7" exit /b 0
goto menu

:config_menu
cls
echo ===================================================
echo                 Configure Paths Menu
echo ===================================================
echo  Current Settings:
echo  [GStreamer]   : !GST_PATH!
echo ===================================================
echo  1. Change GStreamer Path
echo  2. Back to Main Menu
echo ===================================================
set /p cfg_choice="Choose an option (1-2): "
if "%cfg_choice%"=="1" (
    set /p GST_PATH="Enter GStreamer Path: "
    goto config_menu
)
if "%cfg_choice%"=="2" goto load_config
goto config_menu

:build_stable
echo [Build] Building Electron client (renderer + main)...
cd opennow-stable
call npm run build
cd ..
echo [Build] Stable build completed. Output: opennow-stable\dist + dist-electron.
pause
goto menu

:build_gstreamer
echo [Build] Building native streamer (Rust + GStreamer)...
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run native:build
cd ..
echo [Build] Native streamer build completed. Output: native\opennow-streamer\bin.
pause
goto menu

:build_standalone
echo [Build] Building standalone installer with bundled GStreamer runtime...
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run dist
cd ..
echo [Build] Standalone build completed. Check opennow-stable\dist-release folder.
pause
goto menu

:build_standalone_nogst
echo [Build] Building standalone installer WITHOUT bundled GStreamer runtime...
echo       Note: target machine must have GStreamer installed system-wide.
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_BUNDLE_GSTREAMER_RUNTIME=0"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run dist
cd ..
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
