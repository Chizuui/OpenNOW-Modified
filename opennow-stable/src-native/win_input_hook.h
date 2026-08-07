#ifndef WIN_INPUT_HOOK_H
#define WIN_INPUT_HOOK_H

#include <windows.h>
#include <thread>
#include <atomic>

#include "mouse_event.h"

// Captures raw mouse input at the OS level and confines the cursor to a window,
// without swallowing keyboard input. Delta/buttons/wheel are pushed to the
// supplied callback (which marshals them to the JS thread). Keyboard stays on
// the normal DOM path so Escape reaches the game without releasing anything.
class WinInputHook {
public:
    using EventCallback = MouseEventCallback;

    // Start capturing for the given top-level window. Safe to call once.
    static bool Start(HWND target, EventCallback cb);
    static void Stop();
    static bool IsRunning();

private:
    static void MessageLoopThread();
    static LRESULT CALLBACK WndProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam);
    static void ConfineCursorToTarget();

    static std::thread worker_thread_;
    static std::atomic<bool> running_;
    static std::atomic<DWORD> thread_id_;
    static HWND target_hwnd_;
    static HWND msg_hwnd_;
    static EventCallback callback_;
};

#endif // WIN_INPUT_HOOK_H
