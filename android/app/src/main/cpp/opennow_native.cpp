#include <jni.h>
#include <media/NdkMediaCodec.h>
#include <media/NdkMediaFormat.h>
#include <sstream>
#include <string>
#include <android/log.h>

#define OPENNOW_TAG "opennow_native"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO,  OPENNOW_TAG, __VA_ARGS__)
#define LOGW(...) __android_log_print(ANDROID_LOG_WARN,  OPENNOW_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, OPENNOW_TAG, __VA_ARGS__)

// ─── Existing NDK codec probe ────────────────────────────────────────────────

extern "C" JNIEXPORT jstring JNICALL
Java_com_opencloudgaming_opennow_NativeCodecProbe_nativeRuntimeSummary(JNIEnv *env, jobject) {
    std::ostringstream out;
    out << "{";
    out << "\"nativeLibrary\":\"opennow_native\",";
    out << "\"mediaNdk\":true,";
    out << "\"rtpPacketSize\":1140,";
    out << "\"inputProtocolVersion\":3,";
#ifdef GSTREAMER_AVAILABLE
    out << "\"gstreamer\":true";
#else
    out << "\"gstreamer\":false";
#endif
    out << "}";
    const std::string value = out.str();
    return env->NewStringUTF(value.c_str());
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeCodecProbe_nativeDecoderAvailable(JNIEnv *env, jobject, jstring mimeType) {
    if (mimeType == nullptr) {
        return JNI_FALSE;
    }

    const char *rawMimeType = env->GetStringUTFChars(mimeType, nullptr);
    if (rawMimeType == nullptr) {
        return JNI_FALSE;
    }

    AMediaCodec *codec = AMediaCodec_createDecoderByType(rawMimeType);
    env->ReleaseStringUTFChars(mimeType, rawMimeType);
    if (codec == nullptr) {
        return JNI_FALSE;
    }

    AMediaCodec_delete(codec);
    return JNI_TRUE;
}

// ─── GStreamer JNI bridge ────────────────────────────────────────────────────
// This section is compiled only when GSTREAMER_AVAILABLE is defined (i.e. when
// GSTREAMER_ROOT_ANDROID is set and the SDK is found at CMake configure time).
//
// Architecture overview
// ─────────────────────
//  Kotlin NativeStreamerBridge.kt              C++ below (JNI)
//  ──────────────────────────────    ←→    ────────────────────────────────
//  gstNativeInit()                           gst_init(); create GLib loop
//  gstCreatePipeline(surface)                build webrtcbin pipeline; attach surface → ANativeWindow
//  gstSetRemoteOffer(sdp)                    feed SDP offer to webrtcbin
//  gstAddRemoteIce(sdp, mid, mlineIdx)       add remote ICE candidate
//  gstDestroyPipeline()                      tear down pipeline + GLib loop
//
//  Kotlin receives async callbacks via NativeStreamerBridge.onGstEvent(tag, data)
//  which is posted from the GLib main loop thread.

#ifdef GSTREAMER_AVAILABLE

#include <gst/gst.h>
#include <gst/webrtc/webrtc.h>
#include <gst/sdp/gstsdpmessage.h>
#include <android/native_window_jni.h>

#include <atomic>
#include <mutex>
#include <thread>

// ─── Global pipeline state ───────────────────────────────────────────────────

struct GstPipelineContext {
    GstElement  *pipeline    = nullptr;
    GstElement  *webrtcbin   = nullptr;
    GstElement  *videosink   = nullptr;
    GMainLoop   *loop        = nullptr;
    std::thread  loop_thread;
    std::atomic<bool> running{false};

    ANativeWindow *native_window = nullptr;

    // JVM references for posting events back to Kotlin
    JavaVM  *jvm            = nullptr;
    jobject  bridge_obj     = nullptr; // global ref to NativeStreamerBridge instance
    jmethodID on_event_mid  = nullptr;
};

static GstPipelineContext g_ctx;
static std::mutex g_ctx_mutex;

// ─── Helpers ─────────────────────────────────────────────────────────────────

static void post_event(const char *tag, const char *data) {
    if (!g_ctx.jvm || !g_ctx.bridge_obj || !g_ctx.on_event_mid) return;

    JNIEnv *env = nullptr;
    bool attached = false;
    int ret = g_ctx.jvm->GetEnv(reinterpret_cast<void **>(&env), JNI_VERSION_1_6);
    if (ret == JNI_EDETACHED) {
        g_ctx.jvm->AttachCurrentThread(&env, nullptr);
        attached = true;
    }
    if (!env) return;

    jstring jtag  = env->NewStringUTF(tag  ? tag  : "");
    jstring jdata = env->NewStringUTF(data ? data : "");
    env->CallVoidMethod(g_ctx.bridge_obj, g_ctx.on_event_mid, jtag, jdata);
    env->DeleteLocalRef(jtag);
    env->DeleteLocalRef(jdata);

    if (attached) g_ctx.jvm->DetachCurrentThread();
}

// ─── WebRTC signal handlers ───────────────────────────────────────────────────

// Called by webrtcbin when a local ICE candidate is gathered.
static void on_ice_candidate(GstElement * /*webrtc*/, guint mline_index, gchar *candidate, gpointer /*user_data*/) {
    LOGI("GStreamer ICE candidate mline=%u cand=%s", mline_index, candidate);
    // Encode as "mlineIndex|candidate" so Kotlin can split and forward to signaling.
    std::string data = std::to_string(mline_index) + "|" + candidate;
    post_event("ice-candidate", data.c_str());
}

// Called when webrtcbin is ready to negotiate and has created a local offer/answer.
static void on_answer_created(GstPromise *promise, gpointer /*user_data*/) {
    const GstStructure *reply = gst_promise_get_reply(promise);
    GstWebRTCSessionDescription *answer = nullptr;
    gst_structure_get(reply, "answer", GST_TYPE_WEBRTC_SESSION_DESCRIPTION, &answer, nullptr);
    gst_promise_unref(promise);

    if (!answer) {
        LOGE("GStreamer on_answer_created: no answer in promise reply");
        post_event("error", "answer-creation-failed");
        return;
    }

    gchar *sdp_str = gst_sdp_message_as_text(answer->sdp);
    LOGI("GStreamer answer SDP created");

    // Set local description first.
    GstPromise *set_promise = gst_promise_new();
    g_signal_emit_by_name(g_ctx.webrtcbin, "set-local-description", answer, set_promise);
    gst_promise_interrupt(set_promise);
    gst_promise_unref(set_promise);

    // Forward the SDP answer text to Kotlin so it can be sent via signaling.
    post_event("sdp-answer", sdp_str);
    g_free(sdp_str);
    gst_webrtc_session_description_free(answer);
}

// Called after the remote offer is applied; triggers answer creation.
static void on_negotiation_needed(GstElement *webrtcbin, gpointer /*user_data*/) {
    LOGI("GStreamer negotiation needed — creating answer");
    GstPromise *promise = gst_promise_new_with_change_func(on_answer_created, nullptr, nullptr);
    g_signal_emit_by_name(webrtcbin, "create-answer", nullptr, promise);
}

// Called when a new decoded video/audio pad is added by webrtcbin.
static void on_pad_added(GstElement * /*src*/, GstPad *pad, gpointer /*user_data*/) {
    GstPad *sink_pad = gst_element_get_static_pad(g_ctx.videosink, "sink");
    if (gst_pad_is_linked(sink_pad)) {
        gst_object_unref(sink_pad);
        return;
    }
    GstCaps *new_pad_caps = gst_pad_get_current_caps(pad);
    if (!new_pad_caps) {
        gst_object_unref(sink_pad);
        return;
    }
    const gchar *name = gst_structure_get_name(gst_caps_get_structure(new_pad_caps, 0));
    LOGI("GStreamer pad added: %s", name ? name : "unknown");
    if (name && g_str_has_prefix(name, "video/")) {
        GstPadLinkReturn link_ret = gst_pad_link(pad, sink_pad);
        if (link_ret != GST_PAD_LINK_OK) {
            LOGE("GStreamer failed to link video pad: %d", link_ret);
        } else {
            LOGI("GStreamer video pad linked to sink");
            post_event("status", "video-pad-linked");
        }
    }
    gst_caps_unref(new_pad_caps);
    gst_object_unref(sink_pad);
}

// ─── JNI methods ─────────────────────────────────────────────────────────────

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstNativeInit(
        JNIEnv *env, jobject thiz) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    if (g_ctx.running.load()) {
        LOGW("gstNativeInit called while pipeline already running — ignoring");
        return JNI_TRUE;
    }

    // Cache JVM and bridge object reference for async callbacks.
    env->GetJavaVM(&g_ctx.jvm);
    if (g_ctx.bridge_obj) {
        env->DeleteGlobalRef(g_ctx.bridge_obj);
    }
    g_ctx.bridge_obj = env->NewGlobalRef(thiz);
    jclass cls = env->GetObjectClass(thiz);
    g_ctx.on_event_mid = env->GetMethodID(cls, "onGstEvent", "(Ljava/lang/String;Ljava/lang/String;)V");
    if (!g_ctx.on_event_mid) {
        LOGE("gstNativeInit: could not find NativeStreamerBridge.onGstEvent method");
        return JNI_FALSE;
    }

    gst_init(nullptr, nullptr);
    LOGI("GStreamer initialized: %s", gst_version_string());
    post_event("status", "gstreamer-initialized");
    return JNI_TRUE;
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstCreatePipeline(
        JNIEnv *env, jobject /*thiz*/, jobject surface) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);

    // Retrieve the native window from the Java Surface object.
    ANativeWindow *window = surface ? ANativeWindow_fromSurface(env, surface) : nullptr;
    if (!window) {
        LOGE("gstCreatePipeline: invalid Surface — no ANativeWindow");
        return JNI_FALSE;
    }
    if (g_ctx.native_window) {
        ANativeWindow_release(g_ctx.native_window);
    }
    g_ctx.native_window = window;

    // Build a minimal WebRTC receive pipeline:
    //   webrtcbin → (dynamic pads) → decodebin → videoconvert → glimagesink
    // Audio is handled by webrtcbin's autoaudiosink branch.
    GError *error = nullptr;
    // Use "autoaudiosink" so GStreamer picks the right audio output for Android.
    // Video is rendered via "glimagesink" which can accept an ANativeWindow.
    const gchar *pipeline_desc =
        "webrtcbin name=webrtc bundle-policy=max-bundle "
        "  webrtc. ! queue ! decodebin name=dbin "
        "  dbin. ! videoconvert ! glimagesink name=vsink "
        "  dbin. ! audioconvert ! audioresample ! autoaudiosink";

    GstElement *pipeline = gst_parse_launch(pipeline_desc, &error);
    if (!pipeline || error) {
        LOGE("gstCreatePipeline: parse error: %s", error ? error->message : "unknown");
        if (error) g_error_free(error);
        if (pipeline) gst_object_unref(pipeline);
        return JNI_FALSE;
    }

    GstElement *webrtcbin = gst_bin_get_by_name(GST_BIN(pipeline), "webrtc");
    GstElement *videosink = gst_bin_get_by_name(GST_BIN(pipeline), "vsink");

    if (!webrtcbin || !videosink) {
        LOGE("gstCreatePipeline: could not find named elements in pipeline");
        gst_object_unref(pipeline);
        return JNI_FALSE;
    }

    // Bind the rendering window to glimagesink.
    g_object_set(videosink, "window-handle", (guintptr) window, nullptr);

    // Wire up WebRTC signal handlers.
    g_signal_connect(webrtcbin, "on-ice-candidate",      G_CALLBACK(on_ice_candidate),      nullptr);
    g_signal_connect(webrtcbin, "on-negotiation-needed", G_CALLBACK(on_negotiation_needed), nullptr);
    g_signal_connect(webrtcbin, "pad-added",             G_CALLBACK(on_pad_added),           nullptr);

    // Store context.
    g_ctx.pipeline  = pipeline;
    g_ctx.webrtcbin = webrtcbin;
    g_ctx.videosink = videosink;

    // Start the GLib main loop on a dedicated thread.
    g_ctx.loop = g_main_loop_new(nullptr, FALSE);
    g_ctx.running.store(true);
    g_ctx.loop_thread = std::thread([]() {
        LOGI("GStreamer GLib main loop starting");
        g_main_loop_run(g_ctx.loop);
        LOGI("GStreamer GLib main loop exited");
    });

    // Set pipeline to PLAYING.
    GstStateChangeReturn ret = gst_element_set_state(pipeline, GST_STATE_PLAYING);
    if (ret == GST_STATE_CHANGE_FAILURE) {
        LOGE("gstCreatePipeline: failed to set pipeline to PLAYING");
        post_event("error", "pipeline-start-failed");
        // Don't tear down here; let Kotlin call gstDestroyPipeline.
    } else {
        LOGI("gstCreatePipeline: pipeline PLAYING (change=%d)", ret);
        post_event("status", "pipeline-ready");
    }

    return JNI_TRUE;
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstSetRemoteOffer(
        JNIEnv *env, jobject /*thiz*/, jstring sdp_jstring) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    if (!g_ctx.webrtcbin) {
        LOGE("gstSetRemoteOffer: no pipeline active");
        return;
    }

    const char *sdp_str = env->GetStringUTFChars(sdp_jstring, nullptr);
    LOGI("gstSetRemoteOffer: applying remote SDP offer");

    GstSDPMessage *sdp_msg = nullptr;
    if (gst_sdp_message_new(&sdp_msg) != GST_SDP_OK ||
        gst_sdp_message_parse_buffer(reinterpret_cast<const guint8 *>(sdp_str),
                                     static_cast<guint>(strlen(sdp_str)), sdp_msg) != GST_SDP_OK) {
        LOGE("gstSetRemoteOffer: SDP parse failed");
        env->ReleaseStringUTFChars(sdp_jstring, sdp_str);
        post_event("error", "sdp-parse-failed");
        return;
    }
    env->ReleaseStringUTFChars(sdp_jstring, sdp_str);

    GstWebRTCSessionDescription *offer =
        gst_webrtc_session_description_new(GST_WEBRTC_SDP_TYPE_OFFER, sdp_msg);

    GstPromise *promise = gst_promise_new_with_change_func(
        [](GstPromise *p, gpointer) {
            const GstStructure *s = gst_promise_get_reply(p);
            if (s && gst_structure_has_field(s, "error")) {
                LOGE("gstSetRemoteOffer: set-remote-description failed");
                post_event("error", "set-remote-description-failed");
            } else {
                LOGI("gstSetRemoteOffer: remote description set — negotiation will proceed");
                post_event("status", "remote-offer-set");
            }
        },
        nullptr, nullptr);

    g_signal_emit_by_name(g_ctx.webrtcbin, "set-remote-description", offer, promise);
    gst_webrtc_session_description_free(offer);
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstAddRemoteIce(
        JNIEnv *env, jobject /*thiz*/,
        jstring candidate_jstr, jstring sdp_mid_jstr, jint sdp_mline_index) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    if (!g_ctx.webrtcbin) return;

    const char *candidate = env->GetStringUTFChars(candidate_jstr, nullptr);
    // sdp_mid is informational; webrtcbin uses mline index.
    LOGI("gstAddRemoteIce: mline=%d cand=%s", sdp_mline_index, candidate);
    g_signal_emit_by_name(g_ctx.webrtcbin, "add-ice-candidate",
                          static_cast<guint>(sdp_mline_index), candidate);
    env->ReleaseStringUTFChars(candidate_jstr, candidate);
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstDestroyPipeline(
        JNIEnv * /*env*/, jobject /*thiz*/) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    LOGI("gstDestroyPipeline: tearing down");

    if (g_ctx.pipeline) {
        gst_element_set_state(g_ctx.pipeline, GST_STATE_NULL);
        gst_object_unref(g_ctx.pipeline);
        g_ctx.pipeline  = nullptr;
        g_ctx.webrtcbin = nullptr;
        g_ctx.videosink = nullptr;
    }

    if (g_ctx.loop) {
        g_main_loop_quit(g_ctx.loop);
        if (g_ctx.loop_thread.joinable()) {
            g_ctx.loop_thread.join();
        }
        g_main_loop_unref(g_ctx.loop);
        g_ctx.loop = nullptr;
    }

    if (g_ctx.native_window) {
        ANativeWindow_release(g_ctx.native_window);
        g_ctx.native_window = nullptr;
    }

    g_ctx.running.store(false);
    post_event("status", "pipeline-destroyed");
}

// ─── GStreamer availability probe ─────────────────────────────────────────────

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstIsAvailable(
        JNIEnv * /*env*/, jobject /*thiz*/) {
    return JNI_TRUE;
}

#else // GSTREAMER_AVAILABLE not defined

// Stub implementations when GStreamer SDK is absent at compile time.
// Kotlin checks isAvailable() and will not enable the GStreamer path.

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstIsAvailable(
        JNIEnv * /*env*/, jobject /*thiz*/) {
    return JNI_FALSE;
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstNativeInit(
        JNIEnv * /*env*/, jobject /*thiz*/) { return JNI_FALSE; }

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstCreatePipeline(
        JNIEnv * /*env*/, jobject /*thiz*/, jobject /*surface*/) { return JNI_FALSE; }

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstSetRemoteOffer(
        JNIEnv * /*env*/, jobject /*thiz*/, jstring /*sdp*/) {}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstAddRemoteIce(
        JNIEnv * /*env*/, jobject /*thiz*/,
        jstring /*candidate*/, jstring /*sdp_mid*/, jint /*mline_index*/) {}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstDestroyPipeline(
        JNIEnv * /*env*/, jobject /*thiz*/) {}

#endif // GSTREAMER_AVAILABLE
