// Linux (X11) raw mouse capture backend.
//
// Architecture: XGrabPointer with confine_to=window on a dedicated worker
// thread. The OS cursor is hidden by passing an empty cursor to the grab (the
// server restores the previous cursor automatically on ungrab - no XFixes
// dependency). After every motion event the cursor is warped back to the
// window center, so the delta of the next event is simply (position - center) -
// never hitting the window edge.
//
// Crash safety: after the grab succeeds, no further X call references the
// window. Warps use root coordinates (dest_w=None) and Stop() only ungrabs, so
// a window destroyed before releaseMouse() can never raise a BadWindow error
// (which would hit Xlib's default exit()-ing error handler).
//
// The event loop is a non-blocking XPending() poll (1 ms) so Stop() never has
// to unblock a stuck XNextEvent(); XNextEvent is only called when XPending > 0.

#include "linux_input_hook.h"

#include <X11/Xlib.h>

#include <atomic>
#include <chrono>
#include <thread>

namespace {

std::thread g_worker;
std::atomic<bool> g_running{false};
Display* g_display = nullptr;
int g_centerX = 0;
int g_centerY = 0;
bool g_firstMotion = true;
MouseEventCallback g_callback;

// 1x1 empty cursor passed to the grab; the server hides the OS cursor while
// the grab is active and restores it on ungrab. Returns None on failure so the
// caller can bail out instead of letting a BadPixmap error hit Xlib's default
// exit()-ing error handler.
Cursor CreateEmptyCursor(Display* dpy) {
    Pixmap pixmap = XCreatePixmap(dpy, DefaultRootWindow(dpy), 1, 1, 1);
    if (pixmap == None) {
        return None;
    }
    XColor color = {};
    color.flags = DoRed | DoGreen | DoBlue;
    Cursor cursor = XCreatePixmapCursor(dpy, pixmap, pixmap, &color, &color, 0, 0);
    XFreePixmap(dpy, pixmap);
    return cursor;
}

void WorkerLoop() {
    while (g_running.load()) {
        while (XPending(g_display) > 0) {
            XEvent ev;
            XNextEvent(g_display, &ev);

            switch (ev.type) {
                case MotionNotify: {
                    int x = ev.xmotion.x_root;
                    int y = ev.xmotion.y_root;

                    if (g_firstMotion) {
                        // Discard the initial jump to the pre-grab cursor
                        // position; warp to center without emitting.
                        g_firstMotion = false;
                        XWarpPointer(g_display, None, None, 0, 0, 0, 0, g_centerX, g_centerY);
                        break;
                    }

                    // Because we warp to center after every event, the offset
                    // from center equals the movement since the last warp.
                    int dx = x - g_centerX;
                    int dy = y - g_centerY;
                    if (dx != 0 || dy != 0) {
                        MouseEvent mouseEvent = {};
                        mouseEvent.kind = 0;
                        mouseEvent.dx = dx;
                        mouseEvent.dy = dy;
                        g_callback(mouseEvent);
                    }
                    // Root coordinates (dest_w = None): safe even if the grab
                    // window was destroyed concurrently.
                    XWarpPointer(g_display, None, None, 0, 0, 0, 0, g_centerX, g_centerY);
                    break;
                }
                case ButtonPress:
                case ButtonRelease: {
                    unsigned int button = ev.xbutton.button;
                    bool down = (ev.type == ButtonPress);

                    // Wheel: 4 = up, 5 = down. Emit only on press (Windows
                    // RawInput semantics: one wheel event per notch).
                    if (button == 4 || button == 5) {
                        if (down) {
                            MouseEvent mouseEvent = {};
                            mouseEvent.kind = 2;
                            mouseEvent.wheel = (button == 4) ? 120 : -120;
                            g_callback(mouseEvent);
                        }
                        break;
                    }

                    // Map X11 button numbers to the JS order
                    // (0=L 1=R 2=M 3=X1 4=X2).
                    uint8_t jsButton;
                    switch (button) {
                        case 1: jsButton = 0; break; // left
                        case 3: jsButton = 1; break; // right
                        case 2: jsButton = 2; break; // middle
                        case 8: jsButton = 3; break; // x1
                        case 9: jsButton = 4; break; // x2
                        default: continue;
                    }

                    MouseEvent mouseEvent = {};
                    mouseEvent.kind = 1;
                    mouseEvent.button = jsButton;
                    mouseEvent.state = down ? 1 : 0;
                    g_callback(mouseEvent);
                    break;
                }
                default:
                    break;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
}

} // namespace

bool LinuxInputHook::Start(void* handle, size_t handleSize, MouseEventCallback cb) {
    if (g_running.load()) {
        return true; // Already grabbing - idempotent.
    }

    if (!handle || handleSize < sizeof(Window)) {
        return false;
    }
    Window win = *reinterpret_cast<Window*>(handle);

    Display* dpy = XOpenDisplay(nullptr);
    if (!dpy) {
        // Native Wayland without XWayland, or no DISPLAY - let the app fall
        // back to DOM pointer lock.
        return false;
    }

    Cursor emptyCursor = CreateEmptyCursor(dpy);
    if (emptyCursor == None) {
        XCloseDisplay(dpy);
        return false;
    }

    // Capture all pointer motion (with and without buttons pressed) plus
    // button press/release, confined to the window, with the cursor hidden.
    int grab = XGrabPointer(
        dpy,
        win,
        False,
        PointerMotionMask | ButtonMotionMask | ButtonPressMask | ButtonReleaseMask,
        GrabModeAsync,
        GrabModeAsync,
        win,
        emptyCursor,
        CurrentTime);
    if (grab != GrabSuccess) {
        XFreeCursor(dpy, emptyCursor);
        XCloseDisplay(dpy);
        return false;
    }
    // Our client-side reference is done; the server keeps the cursor alive
    // for the duration of the grab and frees it on ungrab.
    XFreeCursor(dpy, emptyCursor);

    // Compute the warp center (window center in root coordinates). The window
    // is guaranteed alive here: grabMouse() was just called with its handle.
    Window root, child;
    int winX = 0, winY = 0, rootX = 0, rootY = 0;
    unsigned int width = 0, height = 0, border = 0, depth = 0;
    if (XGetGeometry(dpy, win, &root, &winX, &winY, &width, &height, &border, &depth) == 0) {
        XUngrabPointer(dpy, CurrentTime);
        XSync(dpy, False);
        XCloseDisplay(dpy);
        return false;
    }
    XTranslateCoordinates(dpy, win, root, 0, 0, &rootX, &rootY, &child);

    g_display = dpy;
    g_centerX = rootX + (int)(width / 2);
    g_centerY = rootY + (int)(height / 2);
    g_firstMotion = true;
    g_callback = std::move(cb);
    g_running = true;

    g_worker = std::thread(WorkerLoop);
    return true;
}

void LinuxInputHook::Stop() {
    if (!g_running.load()) {
        if (g_worker.joinable()) {
            g_worker.join();
        }
        return;
    }

    g_running = false;
    if (g_worker.joinable()) {
        g_worker.join();
    }

    if (g_display) {
        // XUngrabPointer restores the pre-grab cursor automatically; no window
        // reference is needed, so a destroyed grab window cannot error.
        XUngrabPointer(g_display, CurrentTime);
        XSync(g_display, False);
        XCloseDisplay(g_display);
    }
    g_display = nullptr;
    g_firstMotion = true;
    g_callback = nullptr;
}

bool LinuxInputHook::IsRunning() {
    return g_running.load();
}
