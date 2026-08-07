#ifndef OPENNOW_MOUSE_EVENT_H
#define OPENNOW_MOUSE_EVENT_H

#include <cstdint>
#include <functional>

// A single mouse event captured at the OS level and delivered to JS.
// kind: 0 = move (relative delta), 1 = button, 2 = wheel
struct MouseEvent {
    uint8_t kind;
    int32_t dx;      // move: relative X
    int32_t dy;      // move: relative Y
    uint8_t button;  // button: 0=left 1=right 2=middle 3=x1 4=x2
    uint8_t state;   // button: 1=down 0=up
    int32_t wheel;   // wheel: signed notches * 120 (WHEEL_DELTA)
};

// Callback pushed to by the platform capture worker thread (never touches V8).
using MouseEventCallback = std::function<void(const MouseEvent&)>;

#endif // OPENNOW_MOUSE_EVENT_H
