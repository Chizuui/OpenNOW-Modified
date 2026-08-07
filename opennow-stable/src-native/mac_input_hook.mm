// macOS raw mouse capture backend.
//
// Architecture: a CGEventTap (kCGHIDEventTap) runs on a dedicated worker
// thread's runloop. The OS cursor is decoupled (CGAssociateMouseAndMouseCursor
// Position(false)), hidden, and re-centered after every move so relative deltas
// never hit a screen edge. Events are pushed to the MouseEventCallback, which
// marshals them onto the JS thread (ThreadSafeFunction).
//
// Permission: kCGHIDEventTap requires the "Input Monitoring" privacy
// permission. CGPreflightListenEventAccess() detects it up front so we can fail
// fast and let the app fall back to DOM pointer lock.
//
// Threading contract: Start() returns only after the worker has installed the
// tap, and g_runLoop is published BEFORE g_tap, so Stop() can never miss the
// runloop reference and deadlock in join().

#include "mac_input_hook.h"

#import <Cocoa/Cocoa.h>
#import <CoreFoundation/CoreFoundation.h>
#import <CoreGraphics/CoreGraphics.h>

#include <atomic>
#include <chrono>
#include <thread>

namespace {

std::thread g_worker;
std::atomic<bool> g_running{false};
std::atomic<CFRunLoopRef> g_runLoop{nullptr};
// CGEventTapRef is (and has always been) a typedef of CFMachPortRef, and
// newer macOS SDKs no longer declare the typedef - use the underlying CF
// type directly so this compiles on any SDK.
std::atomic<CFMachPortRef> g_tap{nullptr};
// Discards the very first move event after grab, whose delta is the jump from
// the pre-grab cursor position to the grab position.
std::atomic<bool> g_firstMotion{true};
// When true, the next kCGEventMouseMoved is likely the synthetic echo of the
// previous warp; it is dropped only if it carries a zero delta.
std::atomic<bool> g_skipNextWarpEvent{false};
CGPoint g_center = {0, 0};
MouseEventCallback g_callback;

// Best-effort window center. The handle is the NSView* from Electron's
// getNativeWindowHandle(); guarded with @try because we cannot prove a raw
// pointer is a live Objective-C object. Falls back to the main display center.
CGPoint ComputeCenter(void* handle, size_t handleSize) {
    if (handle && handleSize >= sizeof(void*)) {
        @try {
            NSView* view = (__bridge NSView*)handle;
            NSWindow* window = [view window];
            if (window && [window contentView]) {
                NSRect screenRect = [window convertRectToScreen:[[window contentView] bounds]];
                // Cocoa screen coordinates are bottom-left origin; CG event
                // coordinates are top-left origin - flip Y.
                CGFloat screenHeight = (CGFloat)CGDisplayPixelsHigh(CGMainDisplayID());
                return CGPointMake(CGRectGetMidX(screenRect), screenHeight - CGRectGetMidY(screenRect));
            }
        } @catch (...) {
            // Invalid/stale handle: fall through to the main display center.
        }
    }
    CGRect bounds = CGDisplayBounds(CGMainDisplayID());
    return CGPointMake(CGRectGetMidX(bounds), CGRectGetMidY(bounds));
}

CGEventRef EventTapCallback(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void* refcon) {
    (void)proxy;
    (void)refcon;

    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        CFMachPortRef tap = g_tap.load();
        if (tap) {
            CGEventTapEnable(tap, true);
        }
        return event;
    }

    if (!g_running.load() || !g_callback) {
        return event;
    }

    switch (type) {
        case kCGEventMouseMoved: {
            int32_t dx = (int32_t)CGEventGetIntegerValueField(event, kCGMouseEventDeltaX);
            int32_t dy = (int32_t)CGEventGetIntegerValueField(event, kCGMouseEventDeltaY);

            // First event after grab: discard the jump to the pre-grab cursor
            // position, then center without emitting.
            if (g_firstMotion.exchange(false)) {
                CGWarpMouseCursorPosition(g_center);
                g_skipNextWarpEvent = true;
                return event;
            }

            // The synthetic echo of the previous warp carries a zero delta;
            // drop it. If the echo never arrived, a genuine zero-delta event is
            // dropped instead - either way no user movement is lost.
            if (g_skipNextWarpEvent.exchange(false)) {
                if (dx == 0 && dy == 0) {
                    return event;
                }
            }

            if (dx != 0 || dy != 0) {
                MouseEvent mouseEvent = {};
                mouseEvent.kind = 0;
                mouseEvent.dx = dx;
                mouseEvent.dy = dy;
                g_callback(mouseEvent);
            }
            CGWarpMouseCursorPosition(g_center);
            g_skipNextWarpEvent = true;
            break;
        }
        case kCGEventLeftMouseDown:
        case kCGEventLeftMouseUp: {
            MouseEvent mouseEvent = {};
            mouseEvent.kind = 1;
            mouseEvent.button = 0;
            mouseEvent.state = (type == kCGEventLeftMouseDown) ? 1 : 0;
            g_callback(mouseEvent);
            break;
        }
        case kCGEventRightMouseDown:
        case kCGEventRightMouseUp: {
            MouseEvent mouseEvent = {};
            mouseEvent.kind = 1;
            mouseEvent.button = 1;
            mouseEvent.state = (type == kCGEventRightMouseDown) ? 1 : 0;
            g_callback(mouseEvent);
            break;
        }
        case kCGEventOtherMouseDown:
        case kCGEventOtherMouseUp: {
            // CG button numbers: 1=L 2=R 3=Middle 4=X1 5=X2.
            int64_t number = CGEventGetIntegerValueField(event, kCGMouseEventButtonNumber);
            uint8_t button = 2; // middle
            if (number == 4) {
                button = 3; // X1
            } else if (number == 5) {
                button = 4; // X2
            }
            MouseEvent mouseEvent = {};
            mouseEvent.kind = 1;
            mouseEvent.button = button;
            mouseEvent.state = (type == kCGEventOtherMouseDown) ? 1 : 0;
            g_callback(mouseEvent);
            break;
        }
        case kCGEventScrollWheel: {
            // kCGScrollWheelEventDeltaAxis1 is the raw device delta in the
            // physical wheel direction (positive = wheel up, in lines of 1
            // notch), unaffected by the user's "natural scrolling" preference.
            // Scale to Windows-style WHEEL_DELTA units (+120 per notch) so
            // games see the same signs as on Windows. The dedicated inversion
            // flag (kCGScrollWheelEventIsDirectionInvertedFromDevice) is no
            // longer declared in newer macOS SDKs, so raw device deltas are
            // used directly instead.
            int64_t deviceDelta = CGEventGetIntegerValueField(event, kCGScrollWheelEventDeltaAxis1);
            MouseEvent mouseEvent = {};
            mouseEvent.kind = 2;
            mouseEvent.wheel = (int32_t)(deviceDelta * 120);
            if (mouseEvent.wheel != 0) {
                g_callback(mouseEvent);
            }
            break;
        }
        default:
            break;
    }

    return event;
}

void WorkerMain(void* handle, size_t handleSize) {
    g_center = ComputeCenter(handle, handleSize);

    CGEventMask mask = CGEventMaskBit(kCGEventMouseMoved)
        | CGEventMaskBit(kCGEventLeftMouseDown)
        | CGEventMaskBit(kCGEventLeftMouseUp)
        | CGEventMaskBit(kCGEventRightMouseDown)
        | CGEventMaskBit(kCGEventRightMouseUp)
        | CGEventMaskBit(kCGEventOtherMouseDown)
        | CGEventMaskBit(kCGEventOtherMouseUp)
        | CGEventMaskBit(kCGEventScrollWheel);

    // CGEventTapCreate returns CFMachPortRef (CGEventTapRef is a removed
    // typedef in newer SDKs; the CF type is what every API here accepts).
    CFMachPortRef tap = CGEventTapCreate(
        kCGHIDEventTap,
        kCGHeadInsertEventTap,
        kCGEventTapOptionDefault,
        mask,
        EventTapCallback,
        nullptr);

    if (!tap) {
        // Not permitted (Input Monitoring) or tap unavailable - signal failure.
        g_tap = nullptr;
        g_running = false;
        return;
    }

    // Publish the runloop reference BEFORE the tap so a Stop() racing with the
    // tap publication can still stop the runloop.
    CFRunLoopRef runLoop = CFRunLoopGetCurrent();
    g_runLoop = runLoop;
    g_tap = tap;

    CGEventTapEnable(tap, true);

    // Keep the source reference until after the runloop stops so we can remove
    // it explicitly (documented cleanup pattern for event-tap runloop sources).
    CFRunLoopSourceRef source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
    CFRunLoopAddSource(runLoop, source, kCFRunLoopCommonModes);

    // Belt and suspenders: if Stop() already ran (g_running false), do not
    // block this thread forever in CFRunLoopRun().
    if (!g_running.load()) {
        CFRunLoopStop(runLoop);
    }

    // Blocks until CFRunLoopStop() is called from Stop().
    CFRunLoopRun();

    CFRunLoopRemoveSource(runLoop, source, kCFRunLoopCommonModes);
    CFRelease(source);

    CGEventTapEnable(tap, false);
    CFRelease(tap);
    g_tap = nullptr;
    g_runLoop = nullptr;
}

} // namespace

bool MacInputHook::Start(void* handle, size_t handleSize, MouseEventCallback cb) {
    if (g_running.load()) {
        return true; // Already grabbing - idempotent.
    }
    // kCGHIDEventTap requires Input Monitoring permission. Fail fast so the
    // app can fall back to DOM pointer lock (no permission needed).
    if (!CGPreflightListenEventAccess()) {
        return false;
    }

    g_callback = std::move(cb);
    g_firstMotion = true;
    g_skipNextWarpEvent = false;
    g_running = true;

    // Decouple the cursor so we receive raw unbounded deltas, reduce event
    // suppression latency, and hide the OS cursor.
    CGSetLocalEventsSuppressionInterval(0.0);
    CGAssociateMouseAndMouseCursorPosition(false);
    CGDisplayHideCursor(kCGDirectMainDisplay);

    g_worker = std::thread(WorkerMain, handle, handleSize);

    // Wait until the tap is installed or the worker reports failure.
    while (g_running && g_tap.load() == nullptr) {
        std::this_thread::sleep_for(std::chrono::milliseconds(2));
    }

    if (!g_running) {
        // Tap creation failed (permission race, or the tap is unavailable).
        CGDisplayShowCursor(kCGDirectMainDisplay);
        CGAssociateMouseAndMouseCursorPosition(true);
        CGSetLocalEventsSuppressionInterval(0.25);
        g_callback = nullptr;
        if (g_worker.joinable()) {
            g_worker.join();
        }
        return false;
    }

    return true;
}

void MacInputHook::Stop() {
    if (!g_running.load()) {
        if (g_worker.joinable()) {
            g_worker.join();
        }
        return;
    }

    g_running = false;
    CFRunLoopRef runLoop = g_runLoop.load();
    if (runLoop) {
        CFRunLoopStop(runLoop);
    }
    if (g_worker.joinable()) {
        g_worker.join();
    }

    // Restore the OS cursor and re-associate mouse movement.
    CGDisplayShowCursor(kCGDirectMainDisplay);
    CGAssociateMouseAndMouseCursorPosition(true);
    CGSetLocalEventsSuppressionInterval(0.25);
    g_skipNextWarpEvent = false;
    g_callback = nullptr;
}

bool MacInputHook::IsRunning() {
    return g_running.load();
}
