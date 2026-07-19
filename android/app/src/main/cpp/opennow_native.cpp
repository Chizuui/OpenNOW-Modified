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

// Declare the GStreamer static plugins with C linkage to prevent name mangling
extern "C" {
    GST_PLUGIN_STATIC_DECLARE(coreelements);
    GST_PLUGIN_STATIC_DECLARE(webrtc);
    GST_PLUGIN_STATIC_DECLARE(playback);
    GST_PLUGIN_STATIC_DECLARE(videoconvertscale);
    GST_PLUGIN_STATIC_DECLARE(opengl);
    GST_PLUGIN_STATIC_DECLARE(audioconvert);
    GST_PLUGIN_STATIC_DECLARE(audioresample);
    GST_PLUGIN_STATIC_DECLARE(opensles);
    GST_PLUGIN_STATIC_DECLARE(nice);
    GST_PLUGIN_STATIC_DECLARE(dtls);
    GST_PLUGIN_STATIC_DECLARE(srtp);
    GST_PLUGIN_STATIC_DECLARE(rtp);
    GST_PLUGIN_STATIC_DECLARE(rtpmanager);
    GST_PLUGIN_STATIC_DECLARE(typefindfunctions);
    GST_PLUGIN_STATIC_DECLARE(videoparsersbad);
    GST_PLUGIN_STATIC_DECLARE(androidmedia);
    GST_PLUGIN_STATIC_DECLARE(sctp);
    GST_PLUGIN_STATIC_DECLARE(opus);
    GST_PLUGIN_STATIC_DECLARE(opusparse);
}

// This function is automatically called by gst_init() to register static plugins
extern "C" void gst_init_static_plugins(void) {
    GST_PLUGIN_STATIC_REGISTER(coreelements);
    GST_PLUGIN_STATIC_REGISTER(webrtc);
    GST_PLUGIN_STATIC_REGISTER(playback);
    GST_PLUGIN_STATIC_REGISTER(videoconvertscale);
    GST_PLUGIN_STATIC_REGISTER(opengl);
    GST_PLUGIN_STATIC_REGISTER(audioconvert);
    GST_PLUGIN_STATIC_REGISTER(audioresample);
    GST_PLUGIN_STATIC_REGISTER(opensles);
    GST_PLUGIN_STATIC_REGISTER(nice);
    GST_PLUGIN_STATIC_REGISTER(dtls);
    GST_PLUGIN_STATIC_REGISTER(srtp);
    GST_PLUGIN_STATIC_REGISTER(rtp);
    GST_PLUGIN_STATIC_REGISTER(rtpmanager);
    GST_PLUGIN_STATIC_REGISTER(typefindfunctions);
    GST_PLUGIN_STATIC_REGISTER(videoparsersbad);
    GST_PLUGIN_STATIC_REGISTER(androidmedia);
    GST_PLUGIN_STATIC_REGISTER(sctp);
    GST_PLUGIN_STATIC_REGISTER(opus);
    GST_PLUGIN_STATIC_REGISTER(opusparse);
}

#include <atomic>
#include <mutex>
#include <thread>

// ─── Global pipeline state ───────────────────────────────────────────────────

struct GstPipelineContext {
    GstElement  *pipeline    = nullptr;
    GstElement  *webrtcbin   = nullptr;
    GstElement  *videosink   = nullptr;
    GstElement  *videoqueue  = nullptr; // low-latency video queue
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
    if (!candidate || strlen(candidate) == 0) {
        LOGI("GStreamer ICE gathering complete (null/empty candidate)");
        return;
    }
    LOGI("GStreamer ICE candidate mline=%u cand=%s", mline_index, candidate);
    // Encode as "mlineIndex|candidate" so Kotlin can split and forward to signaling.
    std::string data = std::to_string(mline_index) + "|" + candidate;
    post_event("ice-candidate", data.c_str());
}

struct SetLocalDescriptionContext {
    gchar *sdp_str;
    GstWebRTCSessionDescription *answer;
};

// Called when set-local-description promise completes.
static void on_local_description_set(GstPromise *promise, gpointer user_data) {
    auto *ctx = static_cast<SetLocalDescriptionContext *>(user_data);
    const GstStructure *reply = gst_promise_get_reply(promise);

    if (reply && gst_structure_has_field(reply, "error")) {
        LOGE("GStreamer failed to set-local-description");
        post_event("error", "set-local-description-failed");
    } else {
        LOGI("GStreamer set-local-description success — sending SDP answer");
        post_event("sdp-answer", ctx->sdp_str);
    }

    g_free(ctx->sdp_str);
    gst_webrtc_session_description_free(ctx->answer);
    delete ctx;
    gst_promise_unref(promise);
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

    auto *ctx = new SetLocalDescriptionContext{sdp_str, answer};
    GstPromise *set_promise = gst_promise_new_with_change_func(on_local_description_set, ctx, nullptr);
    g_signal_emit_by_name(g_ctx.webrtcbin, "set-local-description", answer, set_promise);
}

// Called when negotiation is needed (we only log here as we handle answer creation on demand).
static void on_negotiation_needed(GstElement *webrtcbin, gpointer /*user_data*/) {
    LOGI("GStreamer negotiation needed (ignored as we are the answerer)");
}

// ICE state name helpers
static const char *ice_connection_state_name(guint state) {
    switch (state) {
        case 0: return "new";
        case 1: return "checking";
        case 2: return "connected";
        case 3: return "completed";
        case 4: return "failed";
        case 5: return "disconnected";
        case 6: return "closed";
        default: return "unknown";
    }
}

static const char *ice_gathering_state_name(guint state) {
    switch (state) {
        case 0: return "new";
        case 1: return "gathering";
        case 2: return "complete";
        default: return "unknown";
    }
}

static const char *peer_connection_state_name(guint state) {
    switch (state) {
        case 0: return "new";
        case 1: return "connecting";
        case 2: return "connected";
        case 3: return "disconnected";
        case 4: return "failed";
        case 5: return "closed";
        default: return "unknown";
    }
}

static const char *signaling_state_name(guint state) {
    switch (state) {
        case 0: return "stable";
        case 1: return "have-local-offer";
        case 2: return "have-remote-offer";
        case 3: return "have-local-pranswer";
        case 4: return "have-remote-pranswer";
        case 5: return "closed";
        default: return "unknown";
    }
}

static void on_ice_connection_state_change(GstElement *webrtcbin, GParamSpec * /*pspec*/, gpointer /*user_data*/) {
    guint state = 0;
    g_object_get(webrtcbin, "ice-connection-state", &state, nullptr);
    const char *name = ice_connection_state_name(state);
    LOGI("GStreamer ICE connection state → %s (%u)", name, state);
    post_event("status", (std::string("ice-connection-") + name).c_str());
}

static void on_ice_gathering_state_change(GstElement *webrtcbin, GParamSpec * /*pspec*/, gpointer /*user_data*/) {
    guint state = 0;
    g_object_get(webrtcbin, "ice-gathering-state", &state, nullptr);
    const char *name = ice_gathering_state_name(state);
    LOGI("GStreamer ICE gathering state → %s (%u)", name, state);
    post_event("status", (std::string("ice-gathering-") + name).c_str());
}

static void on_connection_state_change(GstElement *webrtcbin, GParamSpec * /*pspec*/, gpointer /*user_data*/) {
    guint state = 0;
    g_object_get(webrtcbin, "connection-state", &state, nullptr);
    const char *name = peer_connection_state_name(state);
    LOGI("GStreamer Peer Connection state → %s (%u)", name, state);
    post_event("status", (std::string("peer-connection-") + name).c_str());
}

static void on_signaling_state_change(GstElement *webrtcbin, GParamSpec * /*pspec*/, gpointer /*user_data*/) {
    guint state = 0;
    g_object_get(webrtcbin, "signaling-state", &state, nullptr);
    const char *name = signaling_state_name(state);
    LOGI("GStreamer WebRTC signaling state → %s (%u)", name, state);
    post_event("status", (std::string("webrtc-signaling-") + name).c_str());
}

// Dynamically scan registered GStreamer element factories to find the best Android MediaCodec decoder for a codec.
static std::string find_best_amc_decoder(const std::string &codec) {
    std::string best_factory = "";
    int best_score = -1;

    GList *factories = gst_element_factory_list_get_elements(GST_ELEMENT_FACTORY_TYPE_DECODER | GST_ELEMENT_FACTORY_TYPE_MEDIA_VIDEO, GST_RANK_NONE);
    for (GList *l = factories; l != nullptr; l = l->next) {
        GstElementFactory *f = GST_ELEMENT_FACTORY(l->data);
        const gchar *name = gst_plugin_feature_get_name(GST_PLUGIN_FEATURE(f));
        if (!name) continue;

        std::string fname = name;
        // Target MediaCodec elements: amcviddec-xxxx
        if (fname.rfind("amcviddec-", 0) != 0) continue;

        // Check codec match
        bool matches = false;
        if (codec == "H265" || codec == "HEVC") {
            matches = (fname.find("hevc") != std::string::npos || fname.find("h265") != std::string::npos);
        } else if (codec == "H264") {
            matches = (fname.find("avc") != std::string::npos || fname.find("h264") != std::string::npos);
        } else if (codec == "AV1") {
            matches = (fname.find("av1") != std::string::npos);
        }

        if (matches) {
            int score = 10; // Base score for hardware amcviddec
            if (fname.find("c2") != std::string::npos && (fname.find("mtk") != std::string::npos || fname.find("mediatek") != std::string::npos)) {
                score = 100; // Prefer Codec2 MediaTek hardware decoder (usually more stable)
            } else if (fname.find("omx") != std::string::npos && (fname.find("mtk") != std::string::npos || fname.find("mediatek") != std::string::npos)) {
                score = 90;  // OMX MediaTek hardware decoder
            } else if (fname.find("c2") != std::string::npos) {
                score = 80;  // Standard Codec2 hardware decoder
            } else if (fname.find("google") != std::string::npos || fname.find("android") != std::string::npos) {
                score = 1;   // Software decoders are last resort
            }

            if (score > best_score) {
                best_score = score;
                best_factory = fname;
            }
        }
    }
    gst_plugin_feature_list_free(factories);
    return best_factory;
}

// Context passed to the notify::caps lambda so it can self-disconnect.
struct CapsWatchCtx { GstElement *src; gulong handler_id; };

// Called when a new decoded video/audio pad is added by webrtcbin.
static void on_pad_added(GstElement *src, GstPad *pad, gpointer /*user_data*/) {
    GstCaps *pad_caps = gst_pad_get_current_caps(pad);
    if (!pad_caps) {
        // Caps not yet negotiated — subscribe to caps change and handle then.
        LOGW("on_pad_added: pad has no caps yet, subscribing to notify::caps");
        // Keep a ref on pad and src so the lambda can use them safely.
        // Store the handler ID in a heap-allocated gulong so we can self-disconnect.
        gst_object_ref(pad);
        auto *caps_ctx = new CapsWatchCtx{ static_cast<GstElement *>(gst_object_ref(src)), 0 };
        caps_ctx->handler_id = g_signal_connect(pad, "notify::caps",
            G_CALLBACK(+[](GObject *obj, GParamSpec *, gpointer user_data) {
                GstPad *p = GST_PAD(obj);
                auto *ctx = static_cast<CapsWatchCtx *>(user_data);
                GstCaps *caps = gst_pad_get_current_caps(p);
                if (caps) {
                    LOGI("on_pad_added(notify::caps): now have caps, calling on_pad_added again");
                    on_pad_added(ctx->src, p, nullptr);
                    gst_caps_unref(caps);
                    g_signal_handler_disconnect(obj, ctx->handler_id);
                    gst_object_unref(ctx->src);
                    gst_object_unref(p);   // matches ref taken before connect
                    delete ctx;
                }
            }), caps_ctx);
        return;
    }

    GstStructure *str = gst_caps_get_structure(pad_caps, 0);
    const gchar *media_type = gst_structure_get_name(str);
    LOGI("on_pad_added: pad caps media_type=%s", media_type ? media_type : "unknown");
    post_event("status", (std::string("pad-added-caps-") + (media_type ? media_type : "unknown")).c_str());

    std::string media_kind;
    if (media_type && g_str_equal(media_type, "application/x-rtp")) {
        const gchar *media = gst_structure_get_string(str, "media");
        if (media) {
            media_kind = media;
            LOGI("on_pad_added: resolved RTP media kind=%s", media);
            post_event("status", (std::string("pad-added-rtp-media-") + media).c_str());
        }
    } else if (media_type) {
        if (g_str_has_prefix(media_type, "video/")) {
            media_kind = "video";
        } else if (g_str_has_prefix(media_type, "audio/")) {
            media_kind = "audio";
        }
    }
    gst_caps_unref(pad_caps);


    GstElement *pipeline = g_ctx.pipeline;
    if (!pipeline) return;

    if (media_kind == "video") {
        // Already linked?
        if (g_ctx.videosink) {
            LOGW("on_pad_added: video chain already created, skipping");
            return;
        }

        // 1. Get the encoding name from RTP caps to create matching depay/parser/decoder
        std::string depay_factory;
        std::string parser_factory;
        std::string amc_decoder_factory;
        const gchar *encoding = gst_structure_get_string(str, "encoding-name");
        LOGI("on_pad_added: RTP video encoding-name=%s", encoding ? encoding : "null");
        post_event("status", (std::string("video-encoding-") + (encoding ? encoding : "null")).c_str());

        if (encoding) {
            std::string enc_upper = encoding;
            for (auto &c : enc_upper) c = toupper(c);
            if (enc_upper == "H265" || enc_upper == "HEVC") {
                depay_factory = "rtph265depay";
                parser_factory = "h265parse";
                amc_decoder_factory = find_best_amc_decoder("HEVC");
            } else if (enc_upper == "H264") {
                depay_factory = "rtph264depay";
                parser_factory = "h264parse";
                amc_decoder_factory = find_best_amc_decoder("H264");
            } else if (enc_upper == "AV1") {
                depay_factory = "rtpav1depay";
                parser_factory = "";
                amc_decoder_factory = find_best_amc_decoder("AV1");
            }
        }

        if (depay_factory.empty()) {
            LOGW("on_pad_added: unknown video encoding, falling back to H265 depayloader");
            depay_factory = "rtph265depay";
            parser_factory = "h265parse";
            amc_decoder_factory = find_best_amc_decoder("HEVC");
        }

        GstElement *depay   = gst_element_factory_make(depay_factory.c_str(), nullptr);
        GstElement *parser  = parser_factory.empty() ? nullptr : gst_element_factory_make(parser_factory.c_str(), nullptr);
        GstElement *vconv   = gst_element_factory_make("videoconvert", nullptr);
        GstElement *vsink   = gst_element_factory_make("autovideosink",  "vsink");

        // Force parser to repeat codec config (SPS/PPS) headers in the stream
        if (parser) {
            g_object_set(parser, "config-interval", -1, nullptr);
            LOGI("on_pad_added: configured config-interval=-1 on parser");
        }

        // Try direct hardware MediaCodec decoder first to bypass decodebin
        GstElement *decoder = nullptr;
        if (!amc_decoder_factory.empty()) {
            decoder = gst_element_factory_make(amc_decoder_factory.c_str(), nullptr);
            if (decoder) {
                LOGI("on_pad_added: successfully instantiated direct hardware decoder: %s", amc_decoder_factory.c_str());
                post_event("status", (std::string("video-decoder-amc-") + amc_decoder_factory).c_str());
            }
        }
        bool using_amc = (decoder != nullptr);

        if (!using_amc) {
            LOGW("on_pad_added: direct AMC decoder not found or failed, falling back to decodebin");
            decoder = gst_element_factory_make("decodebin", nullptr);
        }

        if (!depay || (!parser && !parser_factory.empty()) || !decoder || !vconv || !vsink) {
            LOGE("on_pad_added: failed to create video decode elements");
            if (depay)   gst_object_unref(depay);
            if (parser)  gst_object_unref(parser);
            if (decoder) gst_object_unref(decoder);
            if (vconv)   gst_object_unref(vconv);
            if (vsink)   gst_object_unref(vsink);
            return;
        }

        // Low-latency sink config
        g_object_set(vsink, "sync", FALSE, "async", FALSE, "qos", FALSE, nullptr);
        if (g_ctx.native_window) {
            g_object_set(vsink, "window-handle", (guintptr) g_ctx.native_window, nullptr);
        }

        if (parser) {
            gst_bin_add_many(GST_BIN(pipeline), depay, parser, decoder, vconv, vsink, nullptr);
            gst_element_sync_state_with_parent(depay);
            gst_element_sync_state_with_parent(parser);
        } else {
            gst_bin_add_many(GST_BIN(pipeline), depay, decoder, vconv, vsink, nullptr);
            gst_element_sync_state_with_parent(depay);
        }
        gst_element_sync_state_with_parent(decoder);
        gst_element_sync_state_with_parent(vconv);
        gst_element_sync_state_with_parent(vsink);

        // Link: depay -> [parser] -> decoder
        if (parser) {
            if (!gst_element_link_many(depay, parser, decoder, nullptr)) {
                LOGE("on_pad_added: failed to link depay -> parser -> decoder");
                post_event("error", "failed-to-link-depay-parser-decoder");
            }
        } else {
            if (!gst_element_link(depay, decoder)) {
                LOGE("on_pad_added: failed to link depay -> decoder");
                post_event("error", "failed-to-link-depay-decoder");
            }
        }

        if (using_amc) {
            // Static linking if using direct AMC decoder (no dynamic pad needed)
            if (!gst_element_link_many(decoder, vconv, vsink, nullptr)) {
                LOGE("on_pad_added: failed to link decoder -> videoconvert -> vsink");
                post_event("error", "failed-to-link-decoder-vconv-vsink");
            } else {
                LOGI("on_pad_added: statically linked video chain (using direct hardware decoder)");
                post_event("status", "video-pad-linked");
            }
        } else {
            // Fallback: If using decodebin, link videoconvert -> vsink and wait for dynamic pad
            if (!gst_element_link(vconv, vsink)) {
                LOGE("on_pad_added: failed to link videoconvert -> vsink");
                post_event("error", "failed-to-link-vconv-vsink");
            }

            g_signal_connect(decoder, "pad-added", G_CALLBACK(+[](GstElement *, GstPad *decoded_pad, gpointer user_data) {
                GstElement *vconv_inner = static_cast<GstElement *>(user_data);
                GstPad *vconv_sink = gst_element_get_static_pad(vconv_inner, "sink");
                if (!gst_pad_is_linked(vconv_sink)) {
                    GstCaps *dcaps = gst_pad_get_current_caps(decoded_pad);
                    if (dcaps) {
                        const gchar *dtype = gst_structure_get_name(gst_caps_get_structure(dcaps, 0));
                        if (dtype && g_str_has_prefix(dtype, "video/")) {
                            GstPadLinkReturn r = gst_pad_link(decoded_pad, vconv_sink);
                            if (r == GST_PAD_LINK_OK) {
                                LOGI("on_pad_added: decoded video pad linked -> videoconvert");
                                post_event("status", "video-pad-linked");
                            } else {
                                LOGE("on_pad_added: decoded video pad link failed: %d", r);
                                std::string ev = std::string("failed-to-link-dynamic-pad-") + std::to_string(r);
                                post_event("error", ev.c_str());
                            }
                        }
                        gst_caps_unref(dcaps);
                    }
                }
                gst_object_unref(vconv_sink);
            }), vconv);
        }

        // Link webrtcbin video pad -> depay
        GstPad *depay_sink = gst_element_get_static_pad(depay, "sink");
        GstPadLinkReturn ret = gst_pad_link(pad, depay_sink);
        gst_object_unref(depay_sink);
        if (ret != GST_PAD_LINK_OK) {
            LOGE("on_pad_added: failed to link webrtcbin video pad -> depay: %d", ret);
        } else {
            LOGI("on_pad_added: webrtcbin video pad linked -> depay");
            post_event("status", "video-pad-linked-to-depay");
        }

        g_ctx.videosink = vsink;

    } else if (media_kind == "audio") {
        // Audio path: rtpopusdepay → opusdec → audioconvert → audioresample → openslessink
        // We bypass decodebin here because we know the codec is always Opus (from the SDP),
        // and decodebin was failing with "missing plug-in" when the Opus decoder wasn't
        // discovered through its caps-negotiation path.
        GstElement *depay   = gst_element_factory_make("rtpopusdepay", nullptr);
        GstElement *opusdec = gst_element_factory_make("opusdec",      nullptr);
        GstElement *aconv   = gst_element_factory_make("audioconvert",  nullptr);
        GstElement *aresamp = gst_element_factory_make("audioresample", nullptr);
        GstElement *asink   = gst_element_factory_make("openslessink",  nullptr);
        if (!asink) {
            LOGW("on_pad_added: openslessink not available, trying autoaudiosink");
            asink = gst_element_factory_make("autoaudiosink", nullptr);
        }
        if (!depay || !opusdec || !aconv || !aresamp || !asink) {
            LOGE("on_pad_added: failed to create audio elements (depay=%p opusdec=%p aconv=%p aresamp=%p asink=%p)",
                 (void*)depay, (void*)opusdec, (void*)aconv, (void*)aresamp, (void*)asink);
            if (depay)   gst_object_unref(depay);
            if (opusdec) gst_object_unref(opusdec);
            if (aconv)   gst_object_unref(aconv);
            if (aresamp) gst_object_unref(aresamp);
            if (asink)   gst_object_unref(asink);
            return;
        }

        g_object_set(asink, "sync", FALSE, nullptr);
        gst_bin_add_many(GST_BIN(pipeline), depay, opusdec, aconv, aresamp, asink, nullptr);
        gst_element_sync_state_with_parent(depay);
        gst_element_sync_state_with_parent(opusdec);
        gst_element_sync_state_with_parent(aconv);
        gst_element_sync_state_with_parent(aresamp);
        gst_element_sync_state_with_parent(asink);

        if (!gst_element_link_many(depay, opusdec, aconv, aresamp, asink, nullptr)) {
            LOGE("on_pad_added: failed to link audio chain depay -> opusdec -> audioconvert -> audioresample -> audiosink");
        } else {
            LOGI("on_pad_added: audio chain linked: depay -> opusdec -> audioconvert -> audioresample -> audiosink");
        }

        // Link webrtcbin audio pad -> depay
        GstPad *depay_sink = gst_element_get_static_pad(depay, "sink");
        GstPadLinkReturn ret = gst_pad_link(pad, depay_sink);
        gst_object_unref(depay_sink);
        if (ret != GST_PAD_LINK_OK) {
            LOGE("on_pad_added: failed to link webrtcbin audio pad -> depay: %d", ret);
        } else {
            LOGI("on_pad_added: webrtcbin audio pad linked -> depay");
            post_event("status", "audio-pad-linked-to-depay");
        }
    } else {
        LOGI("on_pad_added: ignoring unknown pad media type: %s", media_type ? media_type : "null");
    }
}


// Called by GstBin when a new element is dynamically added (deep within the bin structure).
// We use this to configure the dynamic rtpjitterbuffer for low latency on GStreamer versions
// that do not expose the "latency" property directly on webrtcbin (like GStreamer 1.18.5).
static void on_deep_element_added(GstBin * /*bin*/, GstBin * /*sub_bin*/, GstElement *element, gpointer /*user_data*/) {
    const gchar *name = GST_ELEMENT_NAME(element);
    if (name && g_str_has_prefix(name, "rtpjitterbuffer")) {
        LOGI("on_deep_element_added: detected rtpjitterbuffer '%s', setting latency to 2ms and drop-on-latency=true", name);
        g_object_set(element,
                     "latency", (guint) 2,
                     "drop-on-latency", TRUE,
                     nullptr);
    }
}

// ─── JNI methods ─────────────────────────────────────────────────────────────
// NOTE: JNI_OnLoad is intentionally NOT defined here. libgstreamer-1.0.a
// (gstandroid.c.o) provides its own JNI_OnLoad which registers the JavaVM
// and application Context via GStreamer.nativeInit(). Defining a second
// JNI_OnLoad would shadow GStreamer's, breaking the androidmedia plugin.
//
// HOWEVER: gstandroid.c.o's JNI_OnLoad calls FindClass to register the
// nativeInit JNI method, which may fail if the Kotlin class is not yet loaded
// at library-load time. In that case, _context and _class_loader remain null,
// and androidmedia cannot enumerate MediaCodec decoders.
//
// gstSetAndroidContext is our fallback: it directly calls the gstandroid symbol
// Java_org_freedesktop_gstreamer_GStreamer_nativeInit, which is always linked
// into our .so, bypassing the JNI registration dependency.

// We cache the application context globally in our own variable.
static jobject g_android_context = nullptr;
static jobject g_android_class_loader = nullptr;

extern "C" jobject gst_android_get_application_context(void) {
    return g_android_context;
}

extern "C" jobject gst_android_get_application_class_loader(void) {
    return g_android_class_loader;
}

extern "C" JavaVM *gst_android_get_java_vm(void) {
    return g_ctx.jvm;
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstSetAndroidContext(
        JNIEnv *env, jobject /*thiz*/, jobject context) {
    if (!context) {
        LOGE("gstSetAndroidContext: null context");
        return;
    }
    
    // Cache the JavaVM immediately
    if (!g_ctx.jvm) {
        env->GetJavaVM(&g_ctx.jvm);
        LOGI("gstSetAndroidContext: cached JVM %p", (void*)g_ctx.jvm);
    }

    LOGI("gstSetAndroidContext: caching Android context for androidmedia plugin");
    if (g_android_context) {
        env->DeleteGlobalRef(g_android_context);
    }
    g_android_context = env->NewGlobalRef(context);

    // Retrieve class loader from context
    jclass context_class = env->GetObjectClass(context);
    jmethodID get_class_loader_mid = env->GetMethodID(context_class, "getClassLoader", "()Ljava/lang/ClassLoader;");
    if (get_class_loader_mid) {
        jobject class_loader = env->CallObjectMethod(context, get_class_loader_mid);
        if (class_loader) {
            if (g_android_class_loader) {
                env->DeleteGlobalRef(g_android_class_loader);
            }
            g_android_class_loader = env->NewGlobalRef(class_loader);
            LOGI("gstSetAndroidContext: cached ClassLoader %p", (void*)g_android_class_loader);
        } else {
            LOGW("gstSetAndroidContext: getClassLoader returned null");
        }
    } else {
        LOGE("gstSetAndroidContext: failed to find getClassLoader method on Context class");
    }
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstNativeInit(
        JNIEnv *env, jobject thiz) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    if (g_ctx.running.load()) {
        LOGW("gstNativeInit called while pipeline already running — ignoring");
        return JNI_TRUE;
    }

    // Cache the JavaVM pointer so post_event() can attach GLib/GStreamer
    // worker threads to JNI. GStreamer's JNI_OnLoad (gstandroid.c.o) has
    // already run by this point; we just need our own copy of the pointer.
    if (!g_ctx.jvm) {
        env->GetJavaVM(&g_ctx.jvm);
        LOGI("gstNativeInit: cached JVM %p", (void*)g_ctx.jvm);
    }

    // Cache bridge object reference for async callbacks.
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

    // Log context status
    LOGI("gstNativeInit: cached context context=%p", (void*)g_android_context);
    if (!g_android_context) {
        LOGW("gstNativeInit: Android context is null — hardware decoders may fail. "
             "Call gstSetAndroidContext before gstNativeInit.");
    }



    gst_init(nullptr, nullptr);
    LOGI("GStreamer initialized: %s", gst_version_string());

    // Print all registered video decoders to diagnostics via post_event
    GList *factories = gst_element_factory_list_get_elements(GST_ELEMENT_FACTORY_TYPE_DECODER | GST_ELEMENT_FACTORY_TYPE_MEDIA_VIDEO, GST_RANK_NONE);
    for (GList *l = factories; l != nullptr; l = l->next) {
        GstElementFactory *f = GST_ELEMENT_FACTORY(l->data);
        const gchar *name = gst_plugin_feature_get_name(GST_PLUGIN_FEATURE(f));
        std::string ev_name = std::string("factory-decoder-") + name;
        post_event("status", ev_name.c_str());
    }
    gst_plugin_feature_list_free(factories);

    post_event("status", "gstreamer-initialized");
    return JNI_TRUE;
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstSetSurface(
        JNIEnv *env, jobject /*thiz*/, jobject surface) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);

    ANativeWindow *new_window = surface ? ANativeWindow_fromSurface(env, surface) : nullptr;
    LOGI("gstSetSurface: window=%p (previous=%p)", new_window, g_ctx.native_window);

    if (g_ctx.native_window) {
        ANativeWindow_release(g_ctx.native_window);
    }
    g_ctx.native_window = new_window;

    // If the pipeline is already up, update glimagesink immediately.
    if (g_ctx.videosink) {
        if (new_window) {
            g_object_set(g_ctx.videosink, "window-handle", (guintptr) new_window, nullptr);
            LOGI("gstSetSurface: updated glimagesink window-handle");
        } else {
            g_object_set(g_ctx.videosink, "window-handle", (guintptr) 0, nullptr);
            LOGI("gstSetSurface: cleared glimagesink window-handle");
        }
    }
}

// Helper: percent-encode characters in a URI component that would break URI parsing
// We only encode the minimal set: @ : / ? # which can appear in TURN credentials
static std::string percent_encode_credential(const std::string &raw) {
    std::string out;
    out.reserve(raw.size() * 3);
    for (unsigned char c : raw) {
        // Characters that must be percent-encoded inside a URI authority segment
        if (c == '@' || c == ':' || c == '/' || c == '?' || c == '#' ||
            c == '[' || c == ']' || c == ' ' || c == '%') {
            char buf[4];
            snprintf(buf, sizeof(buf), "%%%02X", c);
            out += buf;
        } else {
            out += static_cast<char>(c);
        }
    }
    return out;
}

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstAddIceServer(
        JNIEnv *env, jobject /*thiz*/, jstring jurl, jstring jusername, jstring jcredential) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);
    if (!g_ctx.webrtcbin) {
        LOGW("gstAddIceServer: called but webrtcbin is null (pipeline not ready)");
        return;
    }

    const char *url        = env->GetStringUTFChars(jurl, nullptr);
    const char *username   = jusername   ? env->GetStringUTFChars(jusername,   nullptr) : nullptr;
    const char *credential = jcredential ? env->GetStringUTFChars(jcredential, nullptr) : nullptr;

    LOGI("gstAddIceServer: url=%s user=%s hasCred=%s",
         url,
         username   ? username   : "(none)",
         credential ? "yes"      : "no");

    bool handled = false;

    if (g_str_has_prefix(url, "stun:")) {
        // Normalise "stun:<host>:<port>" → "stun://<host>:<port>"
        std::string stun_uri = url;
        if (stun_uri.find("stun://") == std::string::npos) {
            stun_uri.replace(0, 5, "stun://");
        }
        g_object_set(g_ctx.webrtcbin, "stun-server", stun_uri.c_str(), nullptr);
        LOGI("gstAddIceServer: set stun-server → %s", stun_uri.c_str());
        handled = true;

    } else if (g_str_has_prefix(url, "turn:") || g_str_has_prefix(url, "turns:")) {
        // Determine scheme and normalise the URI
        bool is_turns = g_str_has_prefix(url, "turns:");
        std::string turn_uri = url;
        if (is_turns) {
            // "turns:<host>…" → "turns://<host>…"
            if (turn_uri.find("turns://") == std::string::npos) {
                turn_uri.replace(0, 6, "turns://");
            }
        } else {
            // "turn:<host>…" → "turn://<host>…"
            if (turn_uri.find("turn://") == std::string::npos) {
                turn_uri.replace(0, 5, "turn://");
            }
        }

        // Embed credentials as "turn://user:password@host:port"
        // Percent-encode the credentials so special chars (@, :, /) don't break the URI.
        if (username && credential) {
            std::string encoded_user = percent_encode_credential(username);
            std::string encoded_pass = percent_encode_credential(credential);
            std::string prefix = is_turns ? "turns://" : "turn://";
            size_t pos = turn_uri.find(prefix);
            if (pos != std::string::npos) {
                turn_uri.insert(pos + prefix.size(),
                                encoded_user + ":" + encoded_pass + "@");
            }
        } else {
            LOGW("gstAddIceServer: TURN server has no credentials — relay may not work");
        }

        gboolean added = FALSE;
        g_signal_emit_by_name(g_ctx.webrtcbin, "add-turn-server", turn_uri.c_str(), &added);
        LOGI("gstAddIceServer: add-turn-server scheme=%s result=%s uri=%s",
             is_turns ? "turns" : "turn",
             added ? "SUCCESS" : "FAILED",
             turn_uri.c_str());
        if (!added) {
            LOGW("gstAddIceServer: add-turn-server returned FALSE — server was NOT registered. "
                 "This may indicate the URI is malformed or libnice does not support this scheme.");
        }
        handled = true;
    }

    if (!handled) {
        LOGW("gstAddIceServer: unrecognised URL scheme, ignoring: %s", url);
    }

    env->ReleaseStringUTFChars(jurl, url);
    if (username)   env->ReleaseStringUTFChars(jusername,   username);
    if (credential) env->ReleaseStringUTFChars(jcredential, credential);
}

extern "C" JNIEXPORT jboolean JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstCreatePipeline(
        JNIEnv *env, jobject /*thiz*/, jobject surface) {
    std::lock_guard<std::mutex> lock(g_ctx_mutex);

    // Surface is optional at pipeline creation time — it can be attached later
    // via gstSetSurface() when the SurfaceHolder becomes available.
    ANativeWindow *window = surface ? ANativeWindow_fromSurface(env, surface) : nullptr;
    if (g_ctx.native_window) {
        ANativeWindow_release(g_ctx.native_window);
    }
    g_ctx.native_window = window;
    LOGI("gstCreatePipeline: surface=%p native_window=%p", surface ? (void*)1 : nullptr, window);

    // Create a bare pipeline — webrtcbin has dynamic source pads so it cannot
    // be statically linked to downstream elements.  The decode chains are
    // assembled in on_pad_added when webrtcbin emits each media pad.
    GstElement *pipeline = gst_pipeline_new("opennow-pipeline");
    GstElement *webrtcbin = gst_element_factory_make("webrtcbin", "webrtc");
    if (!pipeline || !webrtcbin) {
        LOGE("gstCreatePipeline: could not create pipeline or webrtcbin element");
        post_event("error", "pipeline-create-failed: missing webrtcbin plugin");
        if (webrtcbin) gst_object_unref(webrtcbin);
        if (pipeline)  gst_object_unref(pipeline);
        return JNI_FALSE;
    }

    // Configure bundle policy before anything else.
    gst_util_set_object_arg(G_OBJECT(webrtcbin), "bundle-policy", "max-bundle");

    gst_bin_add(GST_BIN(pipeline), webrtcbin);  // pipeline takes ownership

    // ── Low-latency configuration ─────────────────────────────────────────────
    // 1. Listen to deep-element-added to configure rtpjitterbuffer latency.
    g_signal_connect(pipeline, "deep-element-added", G_CALLBACK(on_deep_element_added), nullptr);

    // 2. Wire up WebRTC signal handlers.
    g_signal_connect(webrtcbin, "on-ice-candidate",                G_CALLBACK(on_ice_candidate),                nullptr);
    g_signal_connect(webrtcbin, "on-negotiation-needed",           G_CALLBACK(on_negotiation_needed),           nullptr);
    g_signal_connect(webrtcbin, "pad-added",                       G_CALLBACK(on_pad_added),                    nullptr);
    g_signal_connect(webrtcbin, "notify::ice-connection-state",    G_CALLBACK(on_ice_connection_state_change),  nullptr);
    g_signal_connect(webrtcbin, "notify::ice-gathering-state",     G_CALLBACK(on_ice_gathering_state_change),   nullptr);
    g_signal_connect(webrtcbin, "notify::connection-state",        G_CALLBACK(on_connection_state_change),      nullptr);
    g_signal_connect(webrtcbin, "notify::signaling-state",         G_CALLBACK(on_signaling_state_change),       nullptr);

    // 3. Attach a bus watch to surface GStreamer errors / state changes into diagnostics.
    GstBus *bus = gst_element_get_bus(pipeline);
    gst_bus_add_watch(bus, [](GstBus * /*bus*/, GstMessage *msg, gpointer /*user_data*/) -> gboolean {
        switch (GST_MESSAGE_TYPE(msg)) {
            case GST_MESSAGE_ERROR: {
                GError *err = nullptr;
                gchar  *dbg = nullptr;
                gst_message_parse_error(msg, &err, &dbg);
                LOGE("GStreamer BUS ERROR from '%s': %s | debug: %s",
                     GST_OBJECT_NAME(msg->src),
                     err ? err->message : "(null)",
                     dbg ? dbg : "(none)");
                std::string ev = std::string("gst-error:") +
                                 (err ? err->message : "unknown") + " src=" +
                                 GST_OBJECT_NAME(msg->src);
                post_event("error", ev.c_str());
                g_clear_error(&err);
                g_free(dbg);
                break;
            }
            case GST_MESSAGE_WARNING: {
                GError *err = nullptr;
                gchar  *dbg = nullptr;
                gst_message_parse_warning(msg, &err, &dbg);
                LOGW("GStreamer BUS WARNING from '%s': %s | debug: %s",
                     GST_OBJECT_NAME(msg->src),
                     err ? err->message : "(null)",
                     dbg ? dbg : "(none)");
                g_clear_error(&err);
                g_free(dbg);
                break;
            }
            case GST_MESSAGE_EOS:
                LOGI("GStreamer BUS: EOS");
                post_event("status", "gst-eos");
                break;
            case GST_MESSAGE_STATE_CHANGED: {
                if (GST_MESSAGE_SRC(msg) == GST_OBJECT(g_ctx.pipeline)) {
                    GstState old_s, new_s, pending;
                    gst_message_parse_state_changed(msg, &old_s, &new_s, &pending);
                    LOGI("GStreamer pipeline state: %s -> %s (pending: %s)",
                         gst_element_state_get_name(old_s),
                         gst_element_state_get_name(new_s),
                         gst_element_state_get_name(pending));
                }
                break;
            }
            default:
                break;
        }
        return G_SOURCE_CONTINUE;
    }, nullptr);
    gst_object_unref(bus);

    // 3. Pre-bind the rendering window to glimagesink (may be null if surface not yet available).
    //    The actual glimagesink will be created in on_pad_added; store it for later.
    g_ctx.pipeline      = pipeline;
    g_ctx.webrtcbin     = webrtcbin;
    g_ctx.videosink     = nullptr;   // created dynamically in on_pad_added
    g_ctx.videoqueue    = nullptr;

    // Start the GLib main loop on a dedicated thread.
    g_ctx.loop = g_main_loop_new(nullptr, FALSE);
    g_ctx.running.store(true);
    g_ctx.loop_thread = std::thread([]() {
        LOGI("GStreamer GLib main loop starting");
        g_main_loop_run(g_ctx.loop);
        LOGI("GStreamer GLib main loop exited");
    });

    // Set pipeline to PLAYING so webrtcbin can start ICE gathering immediately.
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
                LOGI("gstSetRemoteOffer: remote description set — creating answer now");
                post_event("status", "remote-offer-set");

                // Trigger answer creation now that the remote offer has been applied.
                GstPromise *answer_promise = gst_promise_new_with_change_func(on_answer_created, nullptr, nullptr);
                g_signal_emit_by_name(g_ctx.webrtcbin, "create-answer", nullptr, answer_promise);
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
        g_ctx.pipeline   = nullptr;
        g_ctx.webrtcbin  = nullptr;
        g_ctx.videosink  = nullptr;
        g_ctx.videoqueue = nullptr;
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

extern "C" JNIEXPORT void JNICALL
Java_com_opencloudgaming_opennow_NativeStreamerBridge_gstSetSurface(
        JNIEnv * /*env*/, jobject /*thiz*/, jobject /*surface*/) {}

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
