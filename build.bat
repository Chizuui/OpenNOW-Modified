@echo off
setlocal enabledelayedexpansion

:: Default Paths
set "GST_PATH=D:\Program Files (x86)\GStreamer\msvc_x86_64"
set "VS_PATH=D:\Program Files (x86)\Microsoft Visual Studio 14.0"
set "VCPKG_PATH=D:\VisualStudioAPP\VisualStudio\VC\vcpkg"

:load_config
:: Apply GStreamer environment variable if it exists
if exist "%GST_PATH%" (
    set "GSTREAMER_1_0_ROOT_MSVC_X86_64=%GST_PATH%\"
    set "PATH=%GST_PATH%\bin;%PATH%"
)

if "%~1"=="" goto menu
if /i "%~1"=="stable" goto build_stable
if /i "%~1"=="gstreamer" goto build_gstreamer
if /i "%~1"=="cpp" goto build_cpp
if /i "%~1"=="standalone" goto build_standalone
if /i "%~1"=="clean" goto build_clean
if /i "%~1"=="config" goto config_menu
goto usage

:usage
echo Usage: build.bat [stable ^| gstreamer ^| cpp ^| standalone ^| clean ^| config]
exit /b 1

:menu
cls
echo ===================================================
echo               OpenNOW Windows Builder              
echo ===================================================
echo  1. Build Stable (Electron Client)
echo  2. Build GStreamer Rust Streamer (Native Rust)
echo  3. Build C++ Native Streamer (Zero GStreamer)
echo  4. Build Standalone Installer (Bundled)
echo  5. Clean Build Artifacts
echo  6. Install C++ Streamer Dependencies (vcpkg)
echo  7. Configure Paths (VS / GStreamer / vcpkg)
echo  8. Exit
echo ===================================================
set /p choice="Choose an option (1-8): "
if "%choice%"=="1" goto build_stable
if "%choice%"=="2" goto build_gstreamer
if "%choice%"=="3" goto build_cpp
if "%choice%"=="4" goto build_standalone
if "%choice%"=="5" goto build_clean
if "%choice%"=="6" goto install_deps
if "%choice%"=="7" goto config_menu
if "%choice%"=="8" exit /b 0
goto menu

:config_menu
cls
echo ===================================================
echo                 Configure Paths Menu               
echo ===================================================
echo  Current Settings:
echo  [GStreamer]   : !GST_PATH!
echo  [VS / MSVC]   : !VS_PATH!
echo  [vcpkg Path]  : !VCPKG_PATH!
echo ===================================================
echo  1. Change GStreamer Path
echo  2. Change Visual Studio / MSVC Path
echo  3. Change vcpkg Path
echo  4. Back to Main Menu
echo ===================================================
set /p cfg_choice="Choose an option (1-4): "
if "%cfg_choice%"=="1" (
    set /p GST_PATH="Enter GStreamer Path: "
    goto config_menu
)
if "%cfg_choice%"=="2" (
    set /p VS_PATH="Enter Visual Studio Path: "
    goto config_menu
)
if "%cfg_choice%"=="3" (
    set /p VCPKG_PATH="Enter vcpkg Path: "
    goto config_menu
)
if "%cfg_choice%"=="4" goto load_config
goto config_menu

:build_stable
echo [Build] Building stable Electron client...
cd opennow-stable
call npm run build
cd ..
echo [Build] Stable build completed.
pause
goto menu

:build_gstreamer
echo [Build] Building native streamer with GStreamer...
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run native:build
cd ..
echo [Build] Native streamer build completed.
pause
goto menu

:build_cpp
echo [Build] Building C++ Native Streamer...
set "VCPKG_ROOT=!VCPKG_PATH!"

:: Initialize MSVC toolchain if possible (avoiding parenthesis bugs with (x86) paths)
if exist "!VS_PATH!\VC\vcvarsall.bat" goto call_vs1
if exist "!VS_PATH!\VC\Auxiliary\Build\vcvarsall.bat" goto call_vs2
echo [Build] Warning: vcvarsall.bat not found at !VS_PATH!.
echo         Assuming MSVC environment is already configured in this terminal.
goto msvc_done

:call_vs1
echo [Build] Activating MSVC environment via !VS_PATH!...
call "!VS_PATH!\VC\vcvarsall.bat" x64
goto msvc_done

:call_vs2
echo [Build] Activating MSVC environment via !VS_PATH!...
call "!VS_PATH!\VC\Auxiliary\Build\vcvarsall.bat" x64
goto msvc_done

:msvc_done
:: Call the C++ streamer build script
cd native\opennow-streamer-cpp
call build.bat
cd ..\..
pause
goto menu

:build_standalone
echo [Build] First compiling C++ Native Streamer for bundle...
set "VCPKG_ROOT=!VCPKG_PATH!"

:: Initialize MSVC toolchain if possible
if exist "!VS_PATH!\VC\vcvarsall.bat" goto call_vs1_std
if exist "!VS_PATH!\VC\Auxiliary\Build\vcvarsall.bat" goto call_vs2_std
goto msvc_done_std

:call_vs1_std
call "!VS_PATH!\VC\vcvarsall.bat" x64
goto msvc_done_std

:call_vs2_std
call "!VS_PATH!\VC\Auxiliary\Build\vcvarsall.bat" x64
goto msvc_done_std

:msvc_done_std
cd native\opennow-streamer-cpp
call build.bat
cd ..\..

echo [Build] Compiling GStreamer Rust Streamer for bundle...
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run native:build
cd ..

echo [Build] Building standalone installer...
cd opennow-stable
set "OPENNOW_NATIVE_STREAMER_FEATURES=gstreamer"
set "OPENNOW_BUNDLE_GSTREAMER_RUNTIME=1"
set "OPENNOW_SKIP_NATIVE_VERIFY=1"
call npm run dist
cd ..
echo [Build] Standalone build completed. Check opennow-stable\dist folder.
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
if exist "native\opennow-streamer\target" (
    rmdir /s /q "native\opennow-streamer\target"
    echo - Removed native\opennow-streamer\target
)
if exist "native\opennow-streamer-cpp\build" (
    rmdir /s /q "native\opennow-streamer-cpp\build"
    echo - Removed native\opennow-streamer-cpp\build
)
echo [Clean] Clean completed.
pause
goto menu

:install_deps
echo [vcpkg] Installing C++ Streamer dependencies...
cd native\opennow-streamer-cpp
call vcpkg install
cd ..\..
echo [vcpkg] Dependency installation done.
pause
goto menu

