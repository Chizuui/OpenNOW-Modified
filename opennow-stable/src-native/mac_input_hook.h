#ifndef MAC_INPUT_HOOK_H
#define MAC_INPUT_HOOK_H

#include <cstddef>

#include "mouse_event.h"

// Captures raw mouse input on macOS through a CGEventTap (kCGHIDEventTap) on a
// dedicated worker runloop. The OS cursor is hidden and re-centered after every
// move so deltas stay unbounded (FPS-style raw input). The NSView* handle
// returned by Electron's getNativeWindowHandle() is used best-effort to center
// on the window; when unavailable or invalid, the main display center is used.
//
// Requires the "Input Monitoring" privacy permission (kCGHIDEventTap is not
// delivered without it). When the permission is missing, CGEventTapCreate
// returns NULL, Start() returns false, and the app falls back to DOM pointer
// lock - which needs no permission.
class MacInputHook {
public:
    // Start capturing. `handle` points at the window-handle bytes from
    // Electron (the NSView*). Safe to call once; repeated calls are idempotent.
    static bool Start(void* handle, size_t handleSize, MouseEventCallback cb);
    static void Stop();
    static bool IsRunning();
};

#endif // MAC_INPUT_HOOK_H
