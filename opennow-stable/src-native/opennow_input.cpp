#include <napi.h>
#include "win_input_hook.h"

// Cleanup hook callback
void CleanupHook(void* arg) {
    WinInputHook::Stop();
}

Napi::Value StartCapture(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();

    if (info.Length() < 2 || !info[0].IsString() || !info[1].IsNumber()) {
        Napi::TypeError::New(env, "String IP and Number Port expected").ThrowAsJavaScriptException();
        return env.Null();
    }

    std::string ip = info[0].As<Napi::String>().Utf8Value();
    int port = info[1].As<Napi::Number>().Int32Value();

    bool grabMouse = true;
    if (info.Length() >= 3 && info[2].IsBoolean()) {
        grabMouse = info[2].As<Napi::Boolean>().Value();
    }

    bool success = WinInputHook::Start(ip, port, grabMouse);
    return Napi::Boolean::New(env, success);
}

Napi::Value StopCapture(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    WinInputHook::Stop();
    return env.Null();
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    // Register environment cleanup hook
    napi_add_env_cleanup_hook(env, CleanupHook, nullptr);

    exports.Set(Napi::String::New(env, "startCapture"), Napi::Function::New(env, StartCapture));
    exports.Set(Napi::String::New(env, "stopCapture"), Napi::Function::New(env, StopCapture));
    
    return exports;
}

NODE_API_MODULE(opennow_input, Init)
