#include "win_input_hook.h"
#include <chrono>

std::thread WinInputHook::worker_thread_;
std::atomic<bool> WinInputHook::running_{false};
std::atomic<DWORD> WinInputHook::thread_id_{0};
HWND WinInputHook::target_hwnd_ = nullptr;
HWND WinInputHook::msg_hwnd_ = nullptr;
WinInputHook::EventCallback WinInputHook::callback_;

bool WinInputHook::IsRunning() {
    return running_;
}

bool WinInputHook::Start(HWND target, EventCallback cb) {
    if (running_) {
        return true;
    }

    target_hwnd_ = target;
    callback_ = std::move(cb);
    running_ = true;
    worker_thread_ = std::thread(&WinInputHook::MessageLoopThread);

    // Wait until the message loop confirms it is up.
    while (running_ && thread_id_ == 0) {
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
    }

    return true;
}

void WinInputHook::Stop() {
    if (!running_) {
        return;
    }

    running_ = false;

    if (thread_id_ != 0) {
        PostThreadMessage(thread_id_, WM_QUIT, 0, 0);
    }

    if (worker_thread_.joinable()) {
        worker_thread_.join();
    }

    // Release cursor confinement + restore visibility on the caller side too,
    // in case the worker exited before cleanup.
    ClipCursor(nullptr);

    thread_id_ = 0;
    target_hwnd_ = nullptr;
    callback_ = nullptr;
}

void WinInputHook::ConfineCursorToTarget() {
    if (!target_hwnd_ || !IsWindow(target_hwnd_)) {
        return;
    }
    RECT rect;
    if (GetClientRect(target_hwnd_, &rect)) {
        POINT topLeft = { rect.left, rect.top };
        POINT bottomRight = { rect.right, rect.bottom };
        ClientToScreen(target_hwnd_, &topLeft);
        ClientToScreen(target_hwnd_, &bottomRight);
        RECT screenRect = { topLeft.x, topLeft.y, bottomRight.x, bottomRight.y };
        ClipCursor(&screenRect);
    }
}

void WinInputHook::MessageLoopThread() {
    thread_id_ = GetCurrentThreadId();

    WNDCLASSEX wc = { 0 };
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = WndProc;
    wc.hInstance = GetModuleHandle(nullptr);
    wc.lpszClassName = "OpenNOWRawMouseWindow";
    RegisterClassEx(&wc);

    msg_hwnd_ = CreateWindowEx(
        0, wc.lpszClassName, "OpenNOWRawMouse",
        0, 0, 0, 0, 0,
        HWND_MESSAGE, nullptr, wc.hInstance, nullptr);

    if (!msg_hwnd_) {
        running_ = false;
        thread_id_ = 0;
        return;
    }

    // Register RawInput mouse. RIDEV_INPUTSINK so we receive input even when the
    // message-only window is not foreground.
    RAWINPUTDEVICE rid;
    rid.usUsagePage = 0x01; // Generic Desktop
    rid.usUsage = 0x02;     // Mouse
    rid.dwFlags = RIDEV_INPUTSINK;
    rid.hwndTarget = msg_hwnd_;
    RegisterRawInputDevices(&rid, 1, sizeof(rid));

    // Hide the OS cursor and confine it to the target window's client area.
    // ShowCursor is a per-queue counter; drive it below zero to force-hide.
    int guard = 0;
    while (ShowCursor(FALSE) >= 0 && guard++ < 16) { /* force hidden */ }
    ConfineCursorToTarget();

    MSG msg;
    while (running_ && GetMessage(&msg, nullptr, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }

    // Cleanup: unregister RawInput, release cursor, restore visibility.
    RAWINPUTDEVICE remove;
    remove.usUsagePage = 0x01;
    remove.usUsage = 0x02;
    remove.dwFlags = RIDEV_REMOVE;
    remove.hwndTarget = nullptr;
    RegisterRawInputDevices(&remove, 1, sizeof(remove));

    ClipCursor(nullptr);
    guard = 0;
    while (ShowCursor(TRUE) < 0 && guard++ < 16) { /* restore */ }

    if (msg_hwnd_) {
        DestroyWindow(msg_hwnd_);
        msg_hwnd_ = nullptr;
    }
    UnregisterClass(wc.lpszClassName, wc.hInstance);
    thread_id_ = 0;
}

LRESULT CALLBACK WinInputHook::WndProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    if (uMsg == WM_INPUT && running_) {
        UINT dwSize = 0;
        GetRawInputData((HRAWINPUT)lParam, RID_INPUT, nullptr, &dwSize, sizeof(RAWINPUTHEADER));
        if (dwSize > 0 && dwSize <= 1024) {
            BYTE buffer[1024];
            if (GetRawInputData((HRAWINPUT)lParam, RID_INPUT, buffer, &dwSize, sizeof(RAWINPUTHEADER)) == dwSize) {
                RAWINPUT* raw = reinterpret_cast<RAWINPUT*>(buffer);
                if (raw->header.dwType == RIM_TYPEMOUSE && callback_) {
                    const RAWMOUSE& m = raw->data.mouse;

                    // Relative movement (RawInput is relative unless MOUSE_MOVE_ABSOLUTE).
                    if ((m.usFlags & MOUSE_MOVE_ABSOLUTE) == 0 && (m.lLastX != 0 || m.lLastY != 0)) {
                        MouseEvent ev = { 0 };
                        ev.kind = 0;
                        ev.dx = m.lLastX;
                        ev.dy = m.lLastY;
                        callback_(ev);
                    }

                    // Buttons.
                    USHORT bf = m.usButtonFlags;
                    auto emitButton = [&](uint8_t button, uint8_t state) {
                        MouseEvent ev = { 0 };
                        ev.kind = 1;
                        ev.button = button;
                        ev.state = state;
                        callback_(ev);
                    };
                    if (bf & RI_MOUSE_LEFT_BUTTON_DOWN)   emitButton(0, 1);
                    if (bf & RI_MOUSE_LEFT_BUTTON_UP)     emitButton(0, 0);
                    if (bf & RI_MOUSE_RIGHT_BUTTON_DOWN)  emitButton(1, 1);
                    if (bf & RI_MOUSE_RIGHT_BUTTON_UP)    emitButton(1, 0);
                    if (bf & RI_MOUSE_MIDDLE_BUTTON_DOWN) emitButton(2, 1);
                    if (bf & RI_MOUSE_MIDDLE_BUTTON_UP)   emitButton(2, 0);
                    if (bf & RI_MOUSE_BUTTON_4_DOWN)      emitButton(3, 1);
                    if (bf & RI_MOUSE_BUTTON_4_UP)        emitButton(3, 0);
                    if (bf & RI_MOUSE_BUTTON_5_DOWN)      emitButton(4, 1);
                    if (bf & RI_MOUSE_BUTTON_5_UP)        emitButton(4, 0);

                    // Wheel.
                    if (bf & RI_MOUSE_WHEEL) {
                        MouseEvent ev = { 0 };
                        ev.kind = 2;
                        ev.wheel = static_cast<int16_t>(m.usButtonData);
                        callback_(ev);
                    }
                }
            }
        }
        // Let the system clean up.
        return DefWindowProc(hwnd, uMsg, wParam, lParam);
    }

    // Re-assert cursor confinement if the window moves/resizes while active.
    if (uMsg == WM_SETCURSOR && running_) {
        return TRUE;
    }

    return DefWindowProc(hwnd, uMsg, wParam, lParam);
}
