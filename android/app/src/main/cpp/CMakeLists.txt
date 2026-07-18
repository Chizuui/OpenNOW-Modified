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

    # Map Android ABI to GStreamer SDK ABI directory name
    if(ANDROID_ABI STREQUAL "arm64-v8a")
        set(_GST_ABI "arm64")
    elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
        set(_GST_ABI "armv7")
    elseif(ANDROID_ABI STREQUAL "x86_64")
        set(_GST_ABI "x86_64")
    elseif(ANDROID_ABI STREQUAL "x86")
        set(_GST_ABI "x86")
    else()
        set(_GST_ABI "${ANDROID_ABI}")
    endif()

    set(_GST_ABI_DIR "${_GST_ROOT}/${_GST_ABI}")
    set(_GST_LIB_DIR "${_GST_ABI_DIR}/lib")
    set(_GST_INCLUDE_DIR "${_GST_ABI_DIR}/include/gstreamer-1.0")
    set(_GST_GLIB_INCLUDE "${_GST_ABI_DIR}/include/glib-2.0")
    set(_GST_GLIBCONFIG_INCLUDE "${_GST_ABI_DIR}/lib/glib-2.0/include")

    if(EXISTS "${_GST_LIB_DIR}" AND EXISTS "${_GST_INCLUDE_DIR}")
        target_compile_definitions(opennow_native PRIVATE GSTREAMER_AVAILABLE=1)

        target_include_directories(opennow_native PRIVATE
            "${_GST_INCLUDE_DIR}"
            "${_GST_GLIB_INCLUDE}"
            "${_GST_GLIBCONFIG_INCLUDE}"
        )

        # Core GStreamer + WebRTC plugin libraries
        set(_GST_LIBS "")
        foreach(_lib
            gstreamer-1.0
            gstwebrtc-1.0
            gstsdp-1.0
            gstrtp-1.0
            gsttag-1.0
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
            pcre2-8
            intl
            iconv
            orc-0.4
        )
            set(_lib_path "${_GST_LIB_DIR}/lib${_lib}.a")
            if(EXISTS "${_lib_path}")
                list(APPEND _GST_LIBS "${_lib_path}")
            else()
                # Check if it's a plugin (in the gstreamer-1.0 subdirectory)
                set(_plugin_path "${_GST_LIB_DIR}/gstreamer-1.0/lib${_lib}.a")
                if(EXISTS "${_plugin_path}")
                    list(APPEND _GST_LIBS "${_plugin_path}")
                else()
                    message(WARNING "[opennow] GStreamer library not found: ${_lib}")
                endif()
            endif()
        endforeach()

        target_link_libraries(opennow_native
            -Wl,--start-group
            ${_GST_LIBS}
            -Wl,--end-group
        )

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
    z
)
