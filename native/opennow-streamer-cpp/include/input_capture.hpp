#pragma once
// =============================================================================
// input_capture.hpp — Win32 Raw Input capture thread
//
// Registers RIDEV_INPUTSINK for mouse and keyboard so input is received
// even when the GFN overlay or Electron window is not in foreground.
// Mirrors the exact approach used by Geronimo.dll (from our RE session).
//
// The captured input is NOT forwarded directly — it is passed to the caller
// via callback so peer_connection.cpp can encode + send it over the data channel.
// =============================================================================

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <atomic>
#include <functional>
#include <thread>
#include <cstdint>
#include <optional>
#include <string>

namespace input {

// ---------------------------------------------------------------------------
// Input protocol constants (mirrored from input.rs)
// ---------------------------------------------------------------------------
constexpr uint32_t INPUT_HEARTBEAT         = 2;
constexpr uint32_t INPUT_KEY_DOWN          = 3;
constexpr uint32_t INPUT_KEY_UP            = 4;
constexpr uint32_t INPUT_MOUSE_REL         = 7;
constexpr uint32_t INPUT_MOUSE_BUTTON_DOWN = 8;
constexpr uint32_t INPUT_MOUSE_BUTTON_UP   = 9;
constexpr uint32_t INPUT_MOUSE_WHEEL       = 10;
constexpr uint32_t INPUT_LOCK_KEYS_SYNC    = 19;

constexpr uint8_t WRAPPER_LEGACY_INPUT          = 0x21;
constexpr uint8_t WRAPPER_SINGLE_INPUT          = 0x22;
constexpr uint8_t WRAPPER_VERSION_MARKER        = 0x23;
constexpr uint8_t WRAPPER_PARTIALLY_RELIABLE    = 0x26;

// ---------------------------------------------------------------------------
// InputEncoder — encodes Win32 raw input into GFN binary wire format
// (C++ port of Rust InputEncoder in input.rs)
// ---------------------------------------------------------------------------
class InputEncoder {
public:
    explicit InputEncoder(uint8_t protocolVersion = 2);

    void setProtocolVersion(uint8_t v) { protocolVersion_ = v; }
    uint8_t protocolVersion() const    { return protocolVersion_; }

    std::vector<uint8_t> encodeHeartbeat() const;
    std::vector<uint8_t> encodeKeyDown(uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const;
    std::vector<uint8_t> encodeKeyUp  (uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const;
    std::vector<uint8_t> encodeMouseMove  (int16_t dx, int16_t dy, uint64_t tsUs) const;
    std::vector<uint8_t> encodeMouseButtonDown(uint8_t button, uint64_t tsUs) const;
    std::vector<uint8_t> encodeMouseButtonUp  (uint8_t button, uint64_t tsUs) const;
    std::vector<uint8_t> encodeMouseWheel(int16_t delta, uint64_t tsUs) const;
    std::vector<uint8_t> encodeLockKeysSync(uint8_t state) const;

private:
    std::vector<uint8_t> encodeKeyboard(uint32_t type, uint16_t keycode, uint16_t scancode, uint16_t mods, uint64_t tsUs) const;
    std::vector<uint8_t> encodeMouseButton(uint32_t type, uint8_t button, uint64_t tsUs) const;
    std::vector<uint8_t> wrapSingle(uint64_t tsUs, const std::vector<uint8_t>& body) const;
    std::vector<uint8_t> wrapLegacy(uint64_t tsUs, const std::vector<uint8_t>& body) const;

    uint8_t protocolVersion_;
};

// ---------------------------------------------------------------------------
// ShortcutBindings — parsed keyboard shortcuts
// ---------------------------------------------------------------------------
struct ShortcutKey {
    uint16_t vkCode   = 0;
    uint16_t modifiers= 0;  // MOD_ALT|MOD_CTRL|MOD_SHIFT
};
std::optional<ShortcutKey> parseShortcut(const std::string& combo);

// ---------------------------------------------------------------------------
// InputCapture — manages the Raw Input message-only window + send callback
// ---------------------------------------------------------------------------
using SendCallback = std::function<void(std::vector<uint8_t> payload, bool partiallyReliable)>;
using ShortcutCallback = std::function<void(const std::string& action)>;
using CaptureChangedCallback = std::function<void(bool captured)>;

class InputCapture {
public:
    InputCapture();
    ~InputCapture();

    // Start the Raw Input thread. Call once after peer connection is established.
    // sendCb will be called with encoded binary packets ready for the data channel.
    void start(SendCallback sendCb,
               ShortcutCallback shortcutCb,
               CaptureChangedCallback captureCb);

    // Stop the thread and release Raw Input registration
    void stop();

    // Lock the cursor to a rect (called when stream starts capture)
    void lockCursor(RECT rect);

    // Release cursor lock (called when overlay is shown or Escape pressed)
    void releaseCursor();

    // Pause / resume sending input packets (used by "input-paused" command)
    void setPaused(bool paused);

    // Update input protocol version after data channel negotiation
    void setProtocolVersion(uint8_t version);

    // Update shortcut bindings
    void updateShortcuts(const std::string& togglePointerLock,
                         const std::string& stopStream,
                         const std::string& toggleStats,
                         const std::string& toggleFullscreen);

    bool isCaptured() const { return cursorLocked_.load(); }

private:
    void threadFunc();
    void registerRawInput(HWND hwnd);
    void unregisterRawInput();
    void processRawInput(HRAWINPUT hRaw);
    void processMouse(const RAWMOUSE& mouse);
    void processKeyboard(const RAWKEYBOARD& kb);
    uint64_t timestampUs() const;

    // Scancode → GFN VK mapping (mirrored from input.rs layout_mapped_keyboard_keycode)
    static uint16_t mapScancode(uint16_t scancode);

    SendCallback          sendCb_;
    ShortcutCallback      shortcutCb_;
    CaptureChangedCallback captureCb_;

    std::thread           thread_;
    HWND                  msgWnd_{nullptr};
    DWORD                 threadId_{0};

    InputEncoder          encoder_{2};
    std::atomic<bool>     running_{false};
    std::atomic<bool>     paused_{false};
    std::atomic<bool>     cursorLocked_{false};

    // Shortcut bindings
    std::optional<ShortcutKey> shortcutToggleLock_;
    std::optional<ShortcutKey> shortcutStop_;
    std::optional<ShortcutKey> shortcutStats_;
    std::optional<ShortcutKey> shortcutFullscreen_;

    RECT  lockRect_{};
    LARGE_INTEGER qpcFreq_{};
};

} // namespace input
