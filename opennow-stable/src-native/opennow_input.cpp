#include <napi.h>

#include "mouse_event.h"

#ifdef _WIN32
#include "win_input_hook.h"
#elif defined(__APPLE__)
#include "mac_input_hook.h"
#else
#include "linux_input_hook.h"
#endif

// Thread-safe function used to marshal mouse events from the OS capture worker
// thread back onto the JS thread. One JS callback receives every event.
static Napi::ThreadSafeFunction g_tsfn;
static bool g_tsfnActive = false;

// Called on the worker thread for each raw mouse event. Copies the event and
// schedules a JS call. Never touches V8 directly.
static void OnMouseEvent(const MouseEvent& ev) {
    if (!g_tsfnActive) {
        return;
    }
    MouseEvent* copy = new MouseEvent(ev);
    napi_status status = g_tsfn.BlockingCall(copy, [](Napi::Env env, Napi::Function jsCallback, MouseEvent* data) {
        Napi::Object obj = Napi::Object::New(env);
        obj.Set("kind", Napi::Number::New(env, data->kind));
        obj.Set("dx", Napi::Number::New(env, data->dx));
        obj.Set("dy", Napi::Number::New(env, data->dy));
        obj.Set("button", Napi::Number::New(env, data->button));
        obj.Set("state", Napi::Number::New(env, data->state));
        obj.Set("wheel", Napi::Number::New(env, data->wheel));
        delete data;
        jsCallback.Call({ obj });
    });
    if (status != napi_ok) {
        delete copy;
    }
}

// ---------------------------------------------------------------------------
// Platform dispatch. `data` points at the window-handle bytes returned by
// Electron's getNativeWindowHandle(): HWND (Windows), NSView* (macOS), X11
// Window id (Linux).
// ---------------------------------------------------------------------------
static bool IsPlatformHookRunning() {
#ifdef _WIN32
    return WinInputHook::IsRunning();
#elif defined(__APPLE__)
    return MacInputHook::IsRunning();
#else
    return LinuxInputHook::IsRunning();
#endif
}

static bool StartPlatformHook(void* data, size_t size, MouseEventCallback cb) {
#ifdef _WIN32
    HWND hwnd = nullptr;
    if (size >= sizeof(HWND)) {
        hwnd = *reinterpret_cast<HWND*>(data);
    }
    if (!hwnd || !IsWindow(hwnd)) {
        return false;
    }
    return WinInputHook::Start(hwnd, std::move(cb));
#elif defined(__APPLE__)
    return MacInputHook::Start(data, size, std::move(cb));
#else
    return LinuxInputHook::Start(data, size, std::move(cb));
#endif
}

static void StopPlatformHook() {
#ifdef _WIN32
    WinInputHook::Stop();
#elif defined(__APPLE__)
    MacInputHook::Stop();
#else
    LinuxInputHook::Stop();
#endif
}

// grabMouse(handleBuffer: Buffer, onEvent: Function) -> boolean
// handleBuffer is Electron's window.getNativeWindowHandle() (pointer-sized
// bytes: HWND / NSView* / X11 Window id).
Napi::Value GrabMouse(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();

    if (info.Length() < 2 || !info[0].IsBuffer() || !info[1].IsFunction()) {
        Napi::TypeError::New(env, "Expected (Buffer handle, Function onEvent)").ThrowAsJavaScriptException();
        return env.Null();
    }

    if (IsPlatformHookRunning()) {
        // Already grabbing — treat as success (idempotent).
        return Napi::Boolean::New(env, true);
    }

    Napi::Buffer<uint8_t> handleBuffer = info[0].As<Napi::Buffer<uint8_t>>();

    g_tsfn = Napi::ThreadSafeFunction::New(
        env,
        info[1].As<Napi::Function>(),
        "OpenNOWRawMouse",
        0,   // unlimited queue
        1);  // one thread will call
    g_tsfnActive = true;

    bool ok = StartPlatformHook(
        handleBuffer.Data(),
        handleBuffer.Length(),
        [](const MouseEvent& ev) { OnMouseEvent(ev); });

    if (!ok) {
        g_tsfnActive = false;
        g_tsfn.Release();
    }
    return Napi::Boolean::New(env, ok);
}

// releaseMouse() -> null
Napi::Value ReleaseMouse(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    StopPlatformHook();
    if (g_tsfnActive) {
        g_tsfnActive = false;
        g_tsfn.Release();
    }
    return env.Null();
}

void CleanupHook(void* /*arg*/) {
    StopPlatformHook();
    if (g_tsfnActive) {
        g_tsfnActive = false;
        g_tsfn.Release();
    }
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    napi_add_env_cleanup_hook(env, CleanupHook, nullptr);
    exports.Set(Napi::String::New(env, "grabMouse"), Napi::Function::New(env, GrabMouse));
    exports.Set(Napi::String::New(env, "releaseMouse"), Napi::Function::New(env, ReleaseMouse));
    return exports;
}

NODE_API_MODULE(opennow_input, Init)
