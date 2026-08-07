#ifndef LINUX_INPUT_HOOK_H
#define LINUX_INPUT_HOOK_H

#include <cstddef>

#include "mouse_event.h"

// Captures raw mouse input on Linux through an X11 pointer grab
// (XGrabPointer + XFixesHideCursor) on a dedicated worker thread. The cursor is
// confined to the target window and re-centered after every move so relative
// deltas stay unbounded (FPS-style raw input).
//
// The handle passed from Electron is the X11 Window id (XID) returned by
// getNativeWindowHandle() - this works under X11 and under XWayland. Under
// native Wayland (no DISPLAY / no XWayland) XOpenDisplay() fails, Start()
// returns false, and the app falls back to DOM pointer lock, which Chromium
// implements natively on Wayland.
//
// If the window is moved or resized after Start(), the warp center is stale
// until the next grab; v1 computes it once at start (the app streams
// fullscreen, where the window does not move).
class LinuxInputHook {
public:
    // Start capturing. `handle` points at the window-handle bytes from
    // Electron (an XID). Safe to call once; repeated calls are idempotent.
    static bool Start(void* handle, size_t handleSize, MouseEventCallback cb);
    static void Stop();
    static bool IsRunning();
};

#endif // LINUX_INPUT_HOOK_H
