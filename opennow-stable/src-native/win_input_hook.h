#ifndef WIN_INPUT_HOOK_H
#define WIN_INPUT_HOOK_H

#include <windows.h>
#include <thread>
#include <atomic>
#include <functional>
#include <cstdint>

// A single mouse event captured from RawInput and delivered to JS.
// kind: 0 = move (relative delta), 1 = button, 2 = wheel
struct MouseEvent {
    uint8_t kind;
    int32_t dx;      // move: relative X
    int32_t dy;      // move: relative Y
    uint8_t button;  // button: 0=left 1=right 2=middle 3=x1 4=x2
    uint8_t state;   // button: 1=down 0=up
    int32_t wheel;   // wheel: signed notches * WHEEL_DELTA
};

// Captures raw mouse input at the OS level and confines the cursor to a window,
// without swallowing keyboard input. Delta/buttons/wheel are pushed to the
// supplied callback (which marshals them to the JS thread). Keyboard stays on
// the normal DOM path so Escape reaches the game without releasing anything.
class WinInputHook {
public:
    using EventCallback = std::function<void(const MouseEvent&)>;

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
