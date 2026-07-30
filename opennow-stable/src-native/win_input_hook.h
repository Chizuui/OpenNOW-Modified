#ifndef WIN_INPUT_HOOK_H
#define WIN_INPUT_HOOK_H

#include <string>
#include <thread>
#include <atomic>
#include "udp_client.h"

// Define the payload structure
#pragma pack(push, 1)
struct GfnInputPayload {
    uint8_t inputType; // 0x01 = Mouse, 0x02 = Keyboard
    int32_t deltaX;    // Untuk Mouse
    int32_t deltaY;    // Untuk Mouse
    uint32_t keyCode;  // Virtual Key Code (VK_*) untuk Keyboard
    uint8_t keyState;  // 1 = KeyDown, 0 = KeyUp
};
#pragma pack(pop)

class WinInputHook {
public:
    static bool Start(const std::string& ip, int port, bool grab_mouse = true);
    static void Stop();

private:
    static void MessageLoopThread();
    static LRESULT CALLBACK WndProc(HWND hwnd, UINT uMsg, WPARAM wParam, LPARAM lParam);
    static LRESULT CALLBACK KeyboardProc(int nCode, WPARAM wParam, LPARAM lParam);

    static std::thread worker_thread_;
    static std::atomic<bool> running_;
    static std::atomic<DWORD> thread_id_;
    static std::atomic<bool> grab_mouse_;
    static HHOOK keyboard_hook_;
    static HWND msg_hwnd_;
    static UdpClient* udp_client_;
};

#endif // WIN_INPUT_HOOK_H
