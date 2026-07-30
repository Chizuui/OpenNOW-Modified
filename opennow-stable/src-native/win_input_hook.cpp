#include "win_input_hook.h"
#include <windows.h>
#include <iostream>

std::thread WinInputHook::worker_thread_;
std::atomic<bool> WinInputHook::running_{false};
std::atomic<DWORD> WinInputHook::thread_id_{0};
std::atomic<bool> WinInputHook::grab_mouse_{true};
HHOOK WinInputHook::keyboard_hook_ = nullptr;
HWND WinInputHook::msg_hwnd_ = nullptr;
UdpClient* WinInputHook::udp_client_ = nullptr;

bool WinInputHook::Start(const std::string& ip, int port, bool grab_mouse) {
    if (running_) {
        return true;
    }

    grab_mouse_ = grab_mouse;
    udp_client_ = new UdpClient(ip, port);
    running_ = true;
    worker_thread_ = std::thread(&WinInputHook::MessageLoopThread);

    // Wait until the message loop is up and running_ is confirmed
    while (thread_id_ == 0) {
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
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

    if (udp_client_) {
        delete udp_client_;
        udp_client_ = nullptr;
    }

    thread_id_ = 0;
}

void WinInputHook::MessageLoopThread() {
    thread_id_ = GetCurrentThreadId();

    // Register Window Class for Message-Only Window
    WNDCLASSEX wc = { 0 };
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = WndProc;
    wc.hInstance = GetModuleHandle(nullptr);
    wc.lpszClassName = "OpenNOWInputHookWindow";

    RegisterClassEx(&wc);

    msg_hwnd_ = CreateWindowEx(
        0,
        wc.lpszClassName,
        "OpenNOWInputHook",
        0, 0, 0, 0, 0,
        HWND_MESSAGE, // Message-only window
        nullptr,
        wc.hInstance,
        nullptr
    );

    if (!msg_hwnd_) {
        return;
    }

    // Register Raw Input Mouse Device only if grab_mouse_ is enabled
    if (grab_mouse_) {
        RAWINPUTDEVICE rid;
        rid.usUsagePage = 0x01; // Generic Desktop Controls
        rid.usUsage = 0x02;     // Mouse
        rid.dwFlags = RIDEV_INPUTSINK; // Capture in background
        rid.hwndTarget = msg_hwnd_;

        if (!RegisterRawInputDevices(&rid, 1, sizeof(rid))) {
            // Failed to register
        }
    }

    // Install Keyboard Hook
    keyboard_hook_ = SetWindowsHookEx(WH_KEYBOARD_LL, KeyboardProc, GetModuleHandle(nullptr), 0);

    // Windows Message Loop
    MSG msg;
    while (GetMessage(&msg, nullptr, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }

    // Clean up hook and window
    if (keyboard_hook_) {
        UnhookWindowsHookEx(keyboard_hook_);
        keyboard_hook_ = nullptr;
    }

    if (msg_hwnd_) {
        DestroyWindow(msg_hwnd_);
        msg_hwnd_ = nullptr;
    }

    UnregisterClass(wc.lpszClassName, wc.hInstance);
}

LRESULT CALLBACK WinInputHook::WndProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam) {
    if (uMsg == WM_INPUT) {
        UINT dwSize = 0;
        GetRawInputData((HRAWINPUT)lParam, RID_INPUT, nullptr, &dwSize, sizeof(RAWINPUTHEADER));
        
        if (dwSize > 0) {
            std::vector<BYTE> lpszBuffer(dwSize);
            if (GetRawInputData((HRAWINPUT)lParam, RID_INPUT, lpszBuffer.data(), &dwSize, sizeof(RAWINPUTHEADER)) == dwSize) {
                RAWINPUT* raw = reinterpret_cast<RAWINPUT*>(lpszBuffer.data());
                if (raw->header.dwType == RIM_TYPEMOUSE) {
                    // Extract delta movement
                    LONG x = raw->data.mouse.lLastX;
                    LONG y = raw->data.mouse.lLastY;
                    
                    if (x != 0 || y != 0) {
                        GfnInputPayload payload = { 0 };
                        payload.inputType = 0x01; // Mouse
                        payload.deltaX = x;
                        payload.deltaY = y;
                        
                        if (udp_client_) {
                            udp_client_->SendPayload(reinterpret_cast<const uint8_t*>(&payload), sizeof(payload));
                        }
                    }
                }
            }
        }
    }
    return DefWindowProc(hwnd, uMsg, wParam, lParam);
}

LRESULT CALLBACK WinInputHook::KeyboardProc(int nCode, WPARAM wParam, LPARAM lParam) {
    if (nCode >= 0) {
        KBDLLHOOKSTRUCT* kb = reinterpret_cast<KBDLLHOOKSTRUCT*>(lParam);
        
        uint32_t vkCode = kb->vkCode;
        uint8_t keyState = (wParam == WM_KEYDOWN || wParam == WM_SYSKEYDOWN) ? 1 : 0;
        
        GfnInputPayload payload = { 0 };
        payload.inputType = 0x02; // Keyboard
        payload.keyCode = vkCode;
        payload.keyState = keyState;
        
        if (udp_client_) {
            udp_client_->SendPayload(reinterpret_cast<const uint8_t*>(&payload), sizeof(payload));
        }

        // Swallow Escape key
        if (vkCode == VK_ESCAPE) {
            return 1; // Prevent Chromium/Electron/Windows from receiving ESC
        }
    }
    return CallNextHookEx(keyboard_hook_, nCode, wParam, lParam);
}
