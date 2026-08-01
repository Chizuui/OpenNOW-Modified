#include <napi.h>
#include "win_input_hook.h"
#include <windows.h>

// Thread-safe function used to marshal mouse events from the RawInput worker
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

// grabMouse(hwndBuffer: Buffer, onEvent: Function) -> boolean
// hwndBuffer is Electron's window.getNativeWindowHandle() (pointer-sized bytes).
Napi::Value GrabMouse(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();

    if (info.Length() < 2 || !info[0].IsBuffer() || !info[1].IsFunction()) {
        Napi::TypeError::New(env, "Expected (Buffer hwnd, Function onEvent)").ThrowAsJavaScriptException();
        return env.Null();
    }

    if (WinInputHook::IsRunning()) {
        // Already grabbing — treat as success (idempotent).
        return Napi::Boolean::New(env, true);
    }

    Napi::Buffer<uint8_t> handleBuffer = info[0].As<Napi::Buffer<uint8_t>>();
    HWND hwnd = nullptr;
    if (handleBuffer.Length() >= sizeof(HWND)) {
        hwnd = *reinterpret_cast<HWND*>(handleBuffer.Data());
    }
    if (!hwnd || !IsWindow(hwnd)) {
        Napi::TypeError::New(env, "Invalid window handle").ThrowAsJavaScriptException();
        return env.Null();
    }

    g_tsfn = Napi::ThreadSafeFunction::New(
        env,
        info[1].As<Napi::Function>(),
        "OpenNOWRawMouse",
        0,   // unlimited queue
        1);  // one thread will call
    g_tsfnActive = true;

    bool ok = WinInputHook::Start(hwnd, OnMouseEvent);
    if (!ok) {
        g_tsfnActive = false;
        g_tsfn.Release();
    }
    return Napi::Boolean::New(env, ok);
}

// releaseMouse() -> null
Napi::Value ReleaseMouse(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    WinInputHook::Stop();
    if (g_tsfnActive) {
        g_tsfnActive = false;
        g_tsfn.Release();
    }
    return env.Null();
}

void CleanupHook(void* /*arg*/) {
    WinInputHook::Stop();
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
