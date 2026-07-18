-keep class org.webrtc.** { *; }
-keep class org.jni_zero.** { *; }
-keep class kotlinx.serialization.** { *; }
-keepclassmembers class com.opencloudgaming.opennow.** {
    @kotlinx.serialization.Serializable *;
}

# ── GStreamer / JNI bridge ────────────────────────────────────────────────────
# NativeStreamerBridge.onGstEvent() is called directly from C++ via GetMethodID.
# R8 must not rename or remove it in release builds.
-keep class com.opencloudgaming.opennow.NativeStreamerBridge {
    public void onGstEvent(java.lang.String, java.lang.String);
}
# Keep all native (JNI) method declarations so their names match the C++ symbols.
-keepclasseswithmembernames class com.opencloudgaming.opennow.NativeStreamerBridge {
    native <methods>;
}
-keepclasseswithmembernames class com.opencloudgaming.opennow.NativeCodecProbe {
    native <methods>;
}
