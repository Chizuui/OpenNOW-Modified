cmake_minimum_required(VERSION 3.20)
project(opennow-streamer-cpp VERSION 1.0.0 LANGUAGES CXX)

# =============================================================================
# COMPILER REQUIREMENTS
# MSVC 19.14+ (Visual Studio 2017 15.7+) required for C++17 support.
# Install "Build Tools for Visual Studio 2019/2022" if your VS is older.
# Download: https://aka.ms/vs/17/release/vs_BuildTools.exe
# =============================================================================

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Subsystem = CONSOLE so stdin/stdout IPC works correctly with Node.js spawn.
# (Same reason as the Rust streamer — no windows_subsystem = "windows")
if(MSVC)
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} /SUBSYSTEM:CONSOLE")
    # Enable multi-processor compilation
    add_compile_options(/MP /W4 /WX- /wd4100 /wd4505)
    # UTF-8 source encoding
    add_compile_options(/utf-8)
    # Optimization flags for Release
    string(REPLACE "/O2" "/O2 /Oi /Ot" CMAKE_CXX_FLAGS_RELEASE "${CMAKE_CXX_FLAGS_RELEASE}")
    # Static link MSVC runtime so binary works without vcruntime140.dll / msvcp140.dll
    # This is essential when launched as a child process by Electron (Node.js spawn)
    # where the MSVC redistributable DLLs may not be on PATH.
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
endif()

# =============================================================================
# Find packages via vcpkg or system
# If using vcpkg, set -DCMAKE_TOOLCHAIN_FILE=%VCPKG_ROOT%/scripts/buildsystems/vcpkg.cmake
# =============================================================================

# libdatachannel — WebRTC peer connection, ICE, DTLS, SRTP, data channels
find_package(LibDataChannel CONFIG QUIET)
if(NOT LibDataChannel_FOUND)
    # Try PkgConfig fallback
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(LIBDATACHANNEL libdatachannel)
    endif()
    if(NOT LIBDATACHANNEL_FOUND)
        message(FATAL_ERROR
            "libdatachannel not found!\n"
            "Install via vcpkg: vcpkg install libdatachannel\n"
            "Or set LIBDATACHANNEL_DIR to the install prefix."
        )
    endif()
endif()

# nlohmann-json (header-only)
find_package(nlohmann_json CONFIG QUIET)
if(NOT nlohmann_json_FOUND)
    # Try to fetch it
    include(FetchContent)
    FetchContent_Declare(
        nlohmann_json
        URL https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz
        URL_HASH SHA256=d6c65aca6b1ed68e7a182f4757257b107ae403032760ed6ef121c9d55e81757d
    )
    FetchContent_MakeAvailable(nlohmann_json)
endif()

# spdlog (header-only mode)
find_package(spdlog CONFIG QUIET)
if(NOT spdlog_FOUND)
    include(FetchContent)
    FetchContent_Declare(
        spdlog
        URL https://github.com/gabime/spdlog/archive/refs/tags/v1.14.1.tar.gz
    )
    FetchContent_MakeAvailable(spdlog)
endif()

# =============================================================================
# Source files
# =============================================================================

set(SOURCES
    src/main.cpp
    src/protocol.cpp
    src/ipc.cpp
    src/peer_connection.cpp
    src/video_decoder.cpp
    src/input_capture.cpp
)

set(HEADERS
    include/protocol.hpp
    include/ipc.hpp
    include/peer_connection.hpp
    include/video_decoder.hpp
    include/input_capture.hpp
)

add_executable(opennow-streamer-cpp ${SOURCES} ${HEADERS})

target_include_directories(opennow-streamer-cpp PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

# =============================================================================
# Link libraries
# =============================================================================

# libdatachannel
if(LibDataChannel_FOUND)
    target_link_libraries(opennow-streamer-cpp PRIVATE LibDataChannel::LibDataChannel)
elseif(LIBDATACHANNEL_FOUND)
    target_include_directories(opennow-streamer-cpp PRIVATE ${LIBDATACHANNEL_INCLUDE_DIRS})
    target_link_libraries(opennow-streamer-cpp PRIVATE ${LIBDATACHANNEL_LIBRARIES})
endif()

# nlohmann-json
target_link_libraries(opennow-streamer-cpp PRIVATE nlohmann_json::nlohmann_json)

# spdlog
target_link_libraries(opennow-streamer-cpp PRIVATE spdlog::spdlog)

# Windows system libraries — Direct3D 11, DXGI, Windows Media Foundation
target_link_libraries(opennow-streamer-cpp PRIVATE
    d3d11
    dxgi
    d3dcompiler
    mf
    mfplat
    mfreadwrite
    mfuuid
    evr
    avrt          # Audio/Video Real-Time (MMCSS thread priority)
    dwmapi        # DwmSetWindowAttribute
    uxtheme
    winmm
    ole32
    oleaut32
    uuid
)

# =============================================================================
# Output placement — put next to opennow-streamer.exe in build artifacts
# =============================================================================

set_target_properties(opennow-streamer-cpp PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin"
    RUNTIME_OUTPUT_DIRECTORY_RELEASE "${CMAKE_BINARY_DIR}/bin"
    RUNTIME_OUTPUT_DIRECTORY_DEBUG "${CMAKE_BINARY_DIR}/bin"
    OUTPUT_NAME "opennow-streamer-cpp"
    PDB_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin"
)

# =============================================================================
# Install target — copy to native build output that Electron picks up
# =============================================================================

install(TARGETS opennow-streamer-cpp
    RUNTIME DESTINATION .
)
