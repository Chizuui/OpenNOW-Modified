cmake_minimum_required(VERSION 3.22.1)

project(opennow_native)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(opennow_native SHARED opennow_native.cpp)

target_compile_options(opennow_native PRIVATE -fexceptions -frtti)

find_library(log-lib log)
find_library(android-lib android)
find_library(mediandk-lib mediandk)

# ─── Optional GStreamer integration ──────────────────────────────────────────
# Set GSTREAMER_ROOT_ANDROID to the GStreamer Android SDK root (the directory
# that contains libs/arm64-v8a, include/gstreamer-1.0, etc.) and the build
# will compile GStreamer support into the native library.
#
# Example (local.properties or via -P flag):
#   GSTREAMER_ROOT_ANDROID=/opt/gstreamer-1.24.0/android-arm64
#
# The variable can also be set as an environment variable and picked up with
# the $ENV{} syntax below.

if(DEFINED GSTREAMER_ROOT_ANDROID)
    set(_GST_ROOT "${GSTREAMER_ROOT_ANDROID}")
elseif(DEFINED ENV{GSTREAMER_ROOT_ANDROID})
    set(_GST_ROOT "$ENV{GSTREAMER_ROOT_ANDROID}")
endif()

if(DEFINED _GST_ROOT AND EXISTS "${_GST_ROOT}")
    message(STATUS "[opennow] GStreamer root found: ${_GST_ROOT}")

    # GStreamer Android SDK lays out libraries under libs/<abi>/
    set(_GST_LIB_DIR "${_GST_ROOT}/libs/${ANDROID_ABI}")
    set(_GST_INCLUDE_DIR "${_GST_ROOT}/include/gstreamer-1.0")
    set(_GST_GLIB_INCLUDE "${_GST_ROOT}/include/glib-2.0")
    set(_GST_GLIBCONFIG_INCLUDE "${_GST_ROOT}/lib/glib-2.0/include")

    if(EXISTS "${_GST_LIB_DIR}" AND EXISTS "${_GST_INCLUDE_DIR}")
        target_compile_definitions(opennow_native PRIVATE GSTREAMER_AVAILABLE=1)

        target_include_directories(opennow_native PRIVATE
            "${_GST_INCLUDE_DIR}"
            "${_GST_GLIB_INCLUDE}"
            "${_GST_GLIBCONFIG_INCLUDE}"
        )

        # Core GStreamer + WebRTC plugin libraries
        foreach(_lib
            gstreamer-1.0
            gstwebrtc-1.0
            gstsdp-1.0
            gstapp-1.0
            gstbase-1.0
            gstaudio-1.0
            gstvideo-1.0
            gstpbutils-1.0
            gstnet-1.0
            glib-2.0
            gobject-2.0
            gmodule-2.0
            gio-2.0
            ffi
            nice
        )
            find_library(_lib_path_${_lib} ${_lib}
                PATHS "${_GST_LIB_DIR}"
                NO_DEFAULT_PATH
            )
            if(_lib_path_${_lib})
                target_link_libraries(opennow_native "${_lib_path_${_lib}}")
            else()
                message(WARNING "[opennow] GStreamer library not found: ${_lib}")
            endif()
        endforeach()

        # EGL and OpenGL ES needed by GStreamer GL/video sinks
        find_library(egl-lib EGL)
        find_library(glesv2-lib GLESv2)
        find_library(opensles-lib OpenSLES)
        target_link_libraries(opennow_native ${egl-lib} ${glesv2-lib} ${opensles-lib})

        message(STATUS "[opennow] GStreamer support ENABLED for ABI ${ANDROID_ABI}")
    else()
        message(WARNING "[opennow] GSTREAMER_ROOT_ANDROID set but expected dirs not found — GStreamer disabled.")
    endif()
else()
    message(STATUS "[opennow] GSTREAMER_ROOT_ANDROID not set — GStreamer support DISABLED (WebRTC-only build).")
endif()
# ─────────────────────────────────────────────────────────────────────────────

target_link_libraries(
    opennow_native
    ${android-lib}
    ${log-lib}
    ${mediandk-lib}
)
