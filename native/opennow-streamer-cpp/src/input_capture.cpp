#include "input_capture.hpp"
#include <spdlog/spdlog.h>
#include <stdexcept>
#include <sstream>
#include <cctype>
#include <algorithm>
#include <unordered_map>

namespace input {

// ---------------------------------------------------------------------------
// Byte packing helpers
// ---------------------------------------------------------------------------
static void putU32Le(std::vector<uint8_t>& v, uint32_t x) {
    v.push_back(x & 0xFF);
    v.push_back((x >> 8) & 0xFF);
    v.push_back((x >> 16) & 0xFF);
    v.push_back((x >> 24) & 0xFF);
}
static void putU16Be(std::vector<uint8_t>& v, uint16_t x) {
    v.push_back((x >> 8) & 0xFF);
    v.push_back(x & 0xFF);
}
static void putI16Be(std::vector<uint8_t>& v, int16_t x) {
    putU16Be(v, static_cast<uint16_t>(x));
}
static void putU64Be(std::vector<uint8_t>& v, uint64_t x) {
    for (int i = 7; i >= 0; --i)
        v.push_back((x >> (i * 8)) & 0xFF);
}

// ---------------------------------------------------------------------------
// InputEncoder
// ---------------------------------------------------------------------------
InputEncoder::InputEncoder(uint8_t protocolVersion) : protocolVersion_(protocolVersion) {}

// Wrap with 0x23 version marker header (protocol v3)
std::vector<uint8_t> InputEncoder::wrapSingle(uint64_t tsUs, const std::vector<uint8_t>& body) const {
    if (protocolVersion_ < 3) return wrapLegacy(tsUs, body);
    std::vector<uint8_t> out;
    out.reserve(1 + 8 + 1 + body.size());
    out.push_back(WRAPPER_VERSION_MARKER);   // 0x23
    putU64Be(out, tsUs);                      // 8 bytes timestamp
    out.push_back(WRAPPER_SINGLE_INPUT);      // 0x22
    out.insert(out.end(), body.begin(), body.end());
    return out;
}

std::vector<uint8_t> InputEncoder::wrapLegacy(uint64_t tsUs, const std::vector<uint8_t>& body) const {
    std::vector<uint8_t> out;
    out.push_back(WRAPPER_LEGACY_INPUT);  // 0x21
    out.insert(out.end(), body.begin(), body.end());
    return out;
}

std::vector<uint8_t> InputEncoder::encodeHeartbeat() const {
    std::vector<uint8_t> body;
    putU32Le(body, INPUT_HEARTBEAT);
    return wrapSingle(0, body);
}

std::vector<uint8_t> InputEncoder::encodeKeyboard(
    uint32_t type, uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const
{
    std::vector<uint8_t> body;
    body.reserve(18);
    putU32Le(body, type);
    putU16Be(body, keycode);
    putU16Be(body, mods);
    putU16Be(body, scancode);
    putU64Be(body, tsUs);
    return wrapSingle(tsUs, body);
}

std::vector<uint8_t> InputEncoder::encodeKeyDown(
    uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const
{
    return encodeKeyboard(INPUT_KEY_DOWN, keycode, scancode, mods, tsUs);
}

std::vector<uint8_t> InputEncoder::encodeKeyUp(
    uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const
{
    return encodeKeyboard(INPUT_KEY_UP, keycode, scancode, mods, tsUs);
}

std::vector<uint8_t> InputEncoder::encodeMouseMove(int16_t dx, int16_t dy, uint64_t tsUs) const {
    std::vector<uint8_t> body;
    body.reserve(22);
    putU32Le(body, INPUT_MOUSE_REL);
    putI16Be(body, dx);
    putI16Be(body, dy);
    putU16Be(body, 0);     // reserved
    putU32Le(body, 0);     // reserved
    putU64Be(body, tsUs);
    // Mouse move uses legacy wrap (matches Rust encode_mouse_move)
    return wrapLegacy(tsUs, body);
}

std::vector<uint8_t> InputEncoder::encodeMouseButton(
    uint32_t type, uint8_t button, uint64_t tsUs) const
{
    std::vector<uint8_t> body;
    body.reserve(18);
    putU32Le(body, type);
    body.push_back(button);
    body.push_back(0);
    putU32Le(body, 0);  // reserved
    putU64Be(body, tsUs);
    return wrapSingle(tsUs, body);
}

std::vector<uint8_t> InputEncoder::encodeMouseButtonDown(uint8_t button, uint64_t tsUs) const {
    return encodeMouseButton(INPUT_MOUSE_BUTTON_DOWN, button, tsUs);
}
std::vector<uint8_t> InputEncoder::encodeMouseButtonUp(uint8_t button, uint64_t tsUs) const {
    return encodeMouseButton(INPUT_MOUSE_BUTTON_UP, button, tsUs);
}

std::vector<uint8_t> InputEncoder::encodeMouseWheel(int16_t delta, uint64_t tsUs) const {
    std::vector<uint8_t> body;
    body.reserve(22);
    putU32Le(body, INPUT_MOUSE_WHEEL);
    putI16Be(body, 0);
    putI16Be(body, delta);
    putU16Be(body, 0);
    putU32Le(body, 0);
    putU64Be(body, tsUs);
    return wrapSingle(tsUs, body);
}

std::vector<uint8_t> InputEncoder::encodeLockKeysSync(uint8_t state) const {
    std::vector<uint8_t> body;
    putU32Le(body, INPUT_LOCK_KEYS_SYNC);
    body.push_back(state);
    return wrapSingle(0, body);
}

// ---------------------------------------------------------------------------
// ShortcutKey parsing
// ---------------------------------------------------------------------------
std::optional<ShortcutKey> parseShortcut(const std::string& combo) {
    if (combo.empty()) return std::nullopt;

    // Parse e.g. "Ctrl+Alt+F12" or "Alt+Escape"
    static const std::unordered_map<std::string, UINT> kVkMap = {
        {"escape", VK_ESCAPE}, {"esc", VK_ESCAPE},
        {"f1",VK_F1},{"f2",VK_F2},{"f3",VK_F3},{"f4",VK_F4},
        {"f5",VK_F5},{"f6",VK_F6},{"f7",VK_F7},{"f8",VK_F8},
        {"f9",VK_F9},{"f10",VK_F10},{"f11",VK_F11},{"f12",VK_F12},
        {"space", VK_SPACE}, {"enter", VK_RETURN}, {"tab", VK_TAB},
        {"backspace", VK_BACK}, {"delete", VK_DELETE},
        {"home", VK_HOME}, {"end", VK_END},
        {"insert", VK_INSERT}, {"pageup", VK_PRIOR}, {"pagedown", VK_NEXT},
        {"left", VK_LEFT}, {"right", VK_RIGHT}, {"up", VK_UP}, {"down", VK_DOWN},
    };

    ShortcutKey key;
    std::istringstream ss(combo);
    std::string token;
    while (std::getline(ss, token, '+')) {
        std::string lower = token;
        std::transform(lower.begin(), lower.end(), lower.begin(), ::tolower);

        if (lower == "ctrl" || lower == "control") { key.modifiers |= MOD_CONTROL; continue; }
        if (lower == "alt")                         { key.modifiers |= MOD_ALT;     continue; }
        if (lower == "shift")                       { key.modifiers |= MOD_SHIFT;   continue; }
        if (lower == "win" || lower == "meta")      { key.modifiers |= MOD_WIN;     continue; }

        auto it = kVkMap.find(lower);
        if (it != kVkMap.end()) {
            key.vkCode = static_cast<uint16_t>(it->second);
        } else if (lower.size() == 1) {
            key.vkCode = static_cast<uint16_t>(::VkKeyScanA(lower[0]) & 0xFF);
        }
    }

    if (key.vkCode == 0) return std::nullopt;
    return key;
}

// ---------------------------------------------------------------------------
// Scancode → GFN VK mapping (from input.rs layout_mapped_keyboard_keycode)
// ---------------------------------------------------------------------------
uint16_t InputCapture::mapScancode(uint16_t scancode) {
    // Set-1 scancode to GFN position-independent VK
    // Matching Rust's match block exactly
    switch (scancode) {
    case 0x0001: return 0x1B; // Escape
    case 0x0002: return 0x31; // 1
    case 0x0003: return 0x32; case 0x0004: return 0x33;
    case 0x0005: return 0x34; case 0x0006: return 0x35;
    case 0x0007: return 0x36; case 0x0008: return 0x37;
    case 0x0009: return 0x38; case 0x000A: return 0x39;
    case 0x000B: return 0x30; case 0x000C: return 0xBD;
    case 0x000D: return 0xBB; case 0x000E: return 0x08;
    case 0x000F: return 0x09; // Tab
    case 0x0010: return 0x51; case 0x0011: return 0x57;
    case 0x0012: return 0x45; case 0x0013: return 0x52;
    case 0x0014: return 0x54; case 0x0015: return 0x59;
    case 0x0016: return 0x55; case 0x0017: return 0x49;
    case 0x0018: return 0x4F; case 0x0019: return 0x50;
    case 0x001A: return 0xDB; case 0x001B: return 0xDD;
    case 0x001C: return 0x0D; // Enter
    case 0x001D: return 0xA2; // LCtrl
    case 0x001E: return 0x41; case 0x001F: return 0x53;
    case 0x0020: return 0x44; case 0x0021: return 0x46;
    case 0x0022: return 0x47; case 0x0023: return 0x48;
    case 0x0024: return 0x4A; case 0x0025: return 0x4B;
    case 0x0026: return 0x4C; case 0x0027: return 0xBA;
    case 0x0028: return 0xDE; case 0x0029: return 0xC0;
    case 0x002A: return 0xA0; // LShift
    case 0x002B: return 0xDC; case 0x002C: return 0x5A;
    case 0x002D: return 0x58; case 0x002E: return 0x43;
    case 0x002F: return 0x56; case 0x0030: return 0x42;
    case 0x0031: return 0x4E; case 0x0032: return 0x4D;
    case 0x0033: return 0xBC; case 0x0034: return 0xBE;
    case 0x0035: return 0xBF; case 0x0036: return 0xA1;
    case 0x0038: return 0xA4; // LAlt
    case 0x0039: return 0x20; // Space
    case 0x003A: return 0x14; // CapsLock
    case 0x003B: return 0x70; case 0x003C: return 0x71;
    case 0x003D: return 0x72; case 0x003E: return 0x73;
    case 0x003F: return 0x74; case 0x0040: return 0x75;
    case 0x0041: return 0x76; case 0x0042: return 0x77;
    case 0x0043: return 0x78; case 0x0044: return 0x79;
    case 0x0057: return 0x7A; case 0x0058: return 0x7B;
    default:     return 0; // Let the OS-reported VK pass through as-is
    }
}

// ---------------------------------------------------------------------------
// InputCapture
// ---------------------------------------------------------------------------
InputCapture::InputCapture() {
    QueryPerformanceFrequency(&qpcFreq_);
}

InputCapture::~InputCapture() {
    stop();
}

void InputCapture::start(SendCallback sendCb,
                          ShortcutCallback shortcutCb,
                          CaptureChangedCallback captureCb)
{
    sendCb_    = std::move(sendCb);
    shortcutCb_ = std::move(shortcutCb);
    captureCb_  = std::move(captureCb);

    running_.store(true);
    thread_ = std::thread([this] { threadFunc(); });
}

void InputCapture::stop() {
    if (!running_.exchange(false)) return;
    // Post a quit message to the message window's thread
    if (threadId_) {
        PostThreadMessageW(threadId_, WM_QUIT, 0, 0);
    }
    if (thread_.joinable())
        thread_.join();
}

void InputCapture::lockCursor(RECT rect) {
    lockRect_ = rect;
    ClipCursor(&lockRect_);
    cursorLocked_.store(true);
    ShowCursor(FALSE);
    if (captureCb_) captureCb_(true);
}

void InputCapture::releaseCursor() {
    ClipCursor(nullptr);
    cursorLocked_.store(false);
    ShowCursor(TRUE);
    if (captureCb_) captureCb_(false);
}

void InputCapture::setPaused(bool paused) {
    paused_.store(paused);
}

void InputCapture::setProtocolVersion(uint8_t version) {
    encoder_.setProtocolVersion(version);
}

void InputCapture::updateShortcuts(
    const std::string& togglePointerLock,
    const std::string& stopStream,
    const std::string& toggleStats,
    const std::string& toggleFullscreen)
{
    shortcutToggleLock_ = parseShortcut(togglePointerLock);
    shortcutStop_       = parseShortcut(stopStream);
    shortcutStats_      = parseShortcut(toggleStats);
    shortcutFullscreen_ = parseShortcut(toggleFullscreen);
}

uint64_t InputCapture::timestampUs() const {
    LARGE_INTEGER now;
    QueryPerformanceCounter(&now);
    return static_cast<uint64_t>(
        now.QuadPart * 1'000'000ULL / qpcFreq_.QuadPart);
}

void InputCapture::threadFunc() {
    // This thread owns the message-only Raw Input window
    // (exactly matching Geronimo.dll's approach from our RE session)

    WNDCLASSA wc = {};
    wc.lpfnWndProc   = DefWindowProcA;
    wc.hInstance     = GetModuleHandleA(nullptr);
    wc.lpszClassName = "OpenNOW_RawInput";
    RegisterClassA(&wc);

    msgWnd_ = CreateWindowA(
        "OpenNOW_RawInput",
        nullptr,
        0, 0, 0, 0, 0,
        HWND_MESSAGE,   // Message-only window — invisible, no desktop presence
        nullptr,
        GetModuleHandleA(nullptr),
        nullptr);

    if (!msgWnd_) {
        spdlog::error("[Input] Failed to create message window: {}", GetLastError());
        return;
    }

    threadId_ = GetCurrentThreadId();
    registerRawInput(msgWnd_);

    // Send initial lock-keys sync
    if (sendCb_) {
        uint8_t lockState = 0;
        if (GetKeyState(VK_CAPITAL) & 1) lockState |= 0x01;
        if (GetKeyState(VK_NUMLOCK) & 1) lockState |= 0x02;
        if (GetKeyState(VK_SCROLL)  & 1) lockState |= 0x04;
        sendCb_(encoder_.encodeLockKeysSync(lockState), false);
    }

    spdlog::info("[Input] Raw Input thread started (HWND=0x{:X})",
                 reinterpret_cast<uintptr_t>(msgWnd_));

    // Message loop
    MSG msg;
    while (running_.load() && GetMessageA(&msg, nullptr, 0, 0) > 0) {
        if (msg.message == WM_INPUT) {
            processRawInput(reinterpret_cast<HRAWINPUT>(msg.lParam));
        }
        TranslateMessage(&msg);
        DispatchMessageA(&msg);
    }

    unregisterRawInput();
    if (msgWnd_) {
        DestroyWindow(msgWnd_);
        msgWnd_ = nullptr;
    }
    spdlog::info("[Input] Raw Input thread exiting");
}

void InputCapture::registerRawInput(HWND hwnd) {
    RAWINPUTDEVICE rid[2] = {};

    // Mouse: RIDEV_INPUTSINK so we receive input even when not in focus
    rid[0].usUsagePage = 0x01;
    rid[0].usUsage     = 0x02;
    rid[0].dwFlags     = RIDEV_INPUTSINK;
    rid[0].hwndTarget  = hwnd;

    // Keyboard: same
    rid[1].usUsagePage = 0x01;
    rid[1].usUsage     = 0x06;
    rid[1].dwFlags     = RIDEV_INPUTSINK | RIDEV_NOLEGACY; // NOLEGACY: suppress standard key messages
    rid[1].hwndTarget  = hwnd;

    if (!RegisterRawInputDevices(rid, 2, sizeof(rid[0]))) {
        spdlog::error("[Input] RegisterRawInputDevices failed: {}", GetLastError());
    } else {
        spdlog::info("[Input] Raw Input registered (mouse + keyboard)");
    }
}

void InputCapture::unregisterRawInput() {
    RAWINPUTDEVICE rid[2] = {};
    rid[0].usUsagePage = 0x01;
    rid[0].usUsage     = 0x02;
    rid[0].dwFlags     = RIDEV_REMOVE;
    rid[0].hwndTarget  = nullptr;

    rid[1].usUsagePage = 0x01;
    rid[1].usUsage     = 0x06;
    rid[1].dwFlags     = RIDEV_REMOVE;
    rid[1].hwndTarget  = nullptr;

    RegisterRawInputDevices(rid, 2, sizeof(rid[0]));
}

void InputCapture::processRawInput(HRAWINPUT hRaw) {
    UINT size = 0;
    GetRawInputData(hRaw, RID_INPUT, nullptr, &size, sizeof(RAWINPUTHEADER));
    if (size == 0) return;

    std::vector<uint8_t> buf(size);
    if (GetRawInputData(hRaw, RID_INPUT, buf.data(), &size, sizeof(RAWINPUTHEADER))
        != size) return;

    const RAWINPUT* raw = reinterpret_cast<const RAWINPUT*>(buf.data());

    switch (raw->header.dwType) {
    case RIM_TYPEMOUSE:
        processMouse(raw->data.mouse);
        break;
    case RIM_TYPEKEYBOARD:
        processKeyboard(raw->data.keyboard);
        break;
    }
}

void InputCapture::processMouse(const RAWMOUSE& mouse) {
    if (paused_.load() || !sendCb_) return;

    uint64_t ts = timestampUs();

    // Relative mouse movement (game mode)
    if (!(mouse.usFlags & MOUSE_MOVE_ABSOLUTE)) {
        int16_t dx = static_cast<int16_t>(mouse.lLastX);
        int16_t dy = static_cast<int16_t>(mouse.lLastY);
        if (dx != 0 || dy != 0) {
            sendCb_(encoder_.encodeMouseMove(dx, dy, ts), true /* partially reliable */);
        }
    }

    // Mouse buttons
    USHORT btns = mouse.usButtonFlags;

    if (btns & RI_MOUSE_BUTTON_1_DOWN)  sendCb_(encoder_.encodeMouseButtonDown(0, ts), false);
    if (btns & RI_MOUSE_BUTTON_1_UP)    sendCb_(encoder_.encodeMouseButtonUp(0, ts), false);
    if (btns & RI_MOUSE_BUTTON_2_DOWN)  sendCb_(encoder_.encodeMouseButtonDown(1, ts), false);
    if (btns & RI_MOUSE_BUTTON_2_UP)    sendCb_(encoder_.encodeMouseButtonUp(1, ts), false);
    if (btns & RI_MOUSE_BUTTON_3_DOWN)  sendCb_(encoder_.encodeMouseButtonDown(2, ts), false);
    if (btns & RI_MOUSE_BUTTON_3_UP)    sendCb_(encoder_.encodeMouseButtonUp(2, ts), false);
    if (btns & RI_MOUSE_BUTTON_4_DOWN)  sendCb_(encoder_.encodeMouseButtonDown(3, ts), false);
    if (btns & RI_MOUSE_BUTTON_4_UP)    sendCb_(encoder_.encodeMouseButtonUp(3, ts), false);
    if (btns & RI_MOUSE_BUTTON_5_DOWN)  sendCb_(encoder_.encodeMouseButtonDown(4, ts), false);
    if (btns & RI_MOUSE_BUTTON_5_UP)    sendCb_(encoder_.encodeMouseButtonUp(4, ts), false);

    // Vertical wheel
    if (btns & RI_MOUSE_WHEEL) {
        int16_t delta = static_cast<int16_t>(mouse.usButtonData);
        sendCb_(encoder_.encodeMouseWheel(delta, ts), false);
    }
}

void InputCapture::processKeyboard(const RAWKEYBOARD& kb) {
    if (!sendCb_) return;

    bool isUp = (kb.Flags & RI_KEY_BREAK) != 0;
    uint16_t vk = kb.VKey;
    uint16_t sc = kb.MakeCode;

    // Map scancode to GFN VK (position-independent, matching official GFN)
    uint16_t mappedVk = mapScancode(sc);
    if (mappedVk == 0) mappedVk = vk;  // Fallback to OS-reported VK

    // Escape — only send scancode 0x01 (as per input.rs layout_mapped_keyboard_scancode)
    uint16_t mappedSc = (sc == 0x0001) ? sc : 0;

    // Modifier state
    uint16_t mods = 0;
    if (GetKeyState(VK_CONTROL) & 0x8000) mods |= 0x0002; // GFN CTRL mask
    if (GetKeyState(VK_MENU)    & 0x8000) mods |= 0x0004; // GFN ALT mask
    if (GetKeyState(VK_SHIFT)   & 0x8000) mods |= 0x0001; // GFN SHIFT mask

    uint64_t ts = timestampUs();

    // Check shortcuts
    if (!isUp) {
        auto checkShortcut = [&](const std::optional<ShortcutKey>& sk, const std::string& action) {
            if (!sk.has_value()) return;
            bool modMatch =
                ((sk->modifiers & MOD_CONTROL) != 0) == ((mods & 0x0002) != 0) &&
                ((sk->modifiers & MOD_ALT)     != 0) == ((mods & 0x0004) != 0) &&
                ((sk->modifiers & MOD_SHIFT)   != 0) == ((mods & 0x0001) != 0);
            if (modMatch && sk->vkCode == vk && shortcutCb_) {
                shortcutCb_(action);
            }
        };

        checkShortcut(shortcutToggleLock_, "togglePointerLock");
        checkShortcut(shortcutStop_,       "stopStream");
        checkShortcut(shortcutStats_,      "toggleStats");
        checkShortcut(shortcutFullscreen_, "toggleFullscreen");
    }

    if (paused_.load()) return;

    if (isUp) {
        sendCb_(encoder_.encodeKeyUp(mappedVk, mappedSc, mods, ts), false);
    } else {
        sendCb_(encoder_.encodeKeyDown(mappedVk, mappedSc, mods, ts), false);
    }

    // Sync lock keys on CapsLock / NumLock / ScrollLock
    if (vk == VK_CAPITAL || vk == VK_NUMLOCK || vk == VK_SCROLL) {
        uint8_t lockState = 0;
        if (GetKeyState(VK_CAPITAL) & 1) lockState |= 0x01;
        if (GetKeyState(VK_NUMLOCK) & 1) lockState |= 0x02;
        if (GetKeyState(VK_SCROLL)  & 1) lockState |= 0x04;
        sendCb_(encoder_.encodeLockKeysSync(lockState), false);
    }
}

} // namespace input
