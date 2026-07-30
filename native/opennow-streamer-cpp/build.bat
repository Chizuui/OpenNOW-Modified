@echo off
:: =============================================================================
:: build.bat — Build OpenNOW C++ Native Streamer
::
:: Requirements:
::   - CMake 3.20+ in PATH
::   - MSVC 2019 or 2022 (Visual Studio Build Tools)
::   - vcpkg (optional, set VCPKG_ROOT if available)
::
:: Usage:
::   build.bat          - Release build
::   build.bat debug    - Debug build
::   build.bat clean    - Clean build directory
:: =============================================================================
setlocal

set BUILD_DIR=%~dp0build
set CONFIG=Release
if "%1"=="debug" set CONFIG=Debug
if "%1"=="clean" (
    echo Cleaning %BUILD_DIR%...
    if exist "%BUILD_DIR%" rmdir /s /q "%BUILD_DIR%"
    echo Done.
    exit /b 0
)

echo.
echo ============================================================
echo  OpenNOW C++ Native Streamer - %CONFIG% Build
echo ============================================================
echo.

:: Check if vcpkg is available
set VCPKG_TOOLCHAIN=
set VCPKG_INSTALLED_OVERRIDE=

:: First: check if project has a local vcpkg_installed already built
set LOCAL_VCPKG_INSTALLED=%~dp0vcpkg_installed
if exist "%LOCAL_VCPKG_INSTALLED%\x64-windows\include" (
    set VCPKG_INSTALLED_OVERRIDE=-DVCPKG_INSTALLED_DIR=%LOCAL_VCPKG_INSTALLED%
)

if defined VCPKG_ROOT (
    set VCPKG_TOOLCHAIN=-DCMAKE_TOOLCHAIN_FILE=%VCPKG_ROOT%\scripts\buildsystems\vcpkg.cmake
    echo Using vcpkg from: %VCPKG_ROOT%
) else (
    echo [WARN] VCPKG_ROOT not set. Dependencies must be installed manually.
    echo        Set VCPKG_ROOT environment variable to enable vcpkg.
    echo.
)

:: Configure
echo [1/2] Configuring CMake...
cmake -S "%~dp0." -B "%BUILD_DIR%" -DCMAKE_BUILD_TYPE=%CONFIG% %VCPKG_TOOLCHAIN% %VCPKG_INSTALLED_OVERRIDE% -DCMAKE_INSTALL_PREFIX="%~dp0output"

if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] CMake configuration failed!
    echo.
    echo If libdatachannel is not found, install it via vcpkg:
    echo   vcpkg install libdatachannel nlohmann-json spdlog
    echo Or set VCPKG_ROOT to your vcpkg installation directory.
    exit /b 1
)

:: Build
echo.
echo [2/2] Building...
cmake --build "%BUILD_DIR%" --config %CONFIG% --parallel

if %ERRORLEVEL% neq 0 (
    echo.
    echo [ERROR] Build failed!
    echo.
    echo [TIP] If you see C++17 errors, make sure you have MSVC 2019 or 2022.
    echo        VS 2015 - MSVC 14.0 - does NOT support C++17.
    echo        Download: https://aka.ms/vs/17/release/vs_BuildTools.exe
    exit /b 1
)

echo.
echo ============================================================
echo  Build complete!
echo  Output: %BUILD_DIR%\bin\opennow-streamer-cpp.exe
echo ============================================================
echo.

:: Copy to parent native output directory for Electron to pick up
set DEST=%~dp0..\build\win-unpacked\resources\opennow-streamer-cpp.exe
if exist "%BUILD_DIR%\bin\opennow-streamer-cpp.exe" (
    echo Copying to Electron resources...
    if not exist "%~dp0..\build\win-unpacked\resources" (
        mkdir "%~dp0..\build\win-unpacked\resources" 2>nul
    )
    copy /Y "%BUILD_DIR%\bin\opennow-streamer-cpp.exe" "%DEST%" 2>nul
    if %ERRORLEVEL% equ 0 (
        echo Copied to: %DEST%
    ) else (
        echo [WARN] Could not copy to Electron resources - not yet built. OK.
    )
)

endlocal
