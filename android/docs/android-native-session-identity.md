# Android native session identity and transport

The official Play-installed GeForce NOW Android 6.39.39022815 client was inspected
and exercised on a POCO X5 Pro (22101320G). Its native gamepad session uses
`Android-Generic-Touch` as `clientPlatformName`, separately from the Android HTTP
User-Agent. Common headers identify NATIVE / NVIDIA-CLASSIC / ANDROID / PHONE.
The observed native create request uses streamerVersion 1, sdkVersion 1.0,
enhancedStreamMode 1, secureRTSPSupported true, and wssignaling 1, with no
GSStreamerType WebRTC metadata. These protocol choices were already present in
6.36; this was an OpenNOW profile mismatch, not a demonstrated 6.39 regression.

OpenNOW's Portal-sized Android profile previously requested native allocation
while selecting its WebRTC implementation. NVIDIA returned RTSP descriptors;
opening the derived WSS endpoint registered a peer but never produced an SDP
offer. After timeout, reconnects could receive peerRemoved. The same phone,
resolution, H.264 decoder, and login streamed successfully in Website client mode.

Native Android create and RESUME now share the native request profile. Endpoint
resolution records whether the selected descriptor requires native RTSP. The
stream client uses NVST for that descriptor, including during reconnect, without
changing the persisted experimental NVST preference. An explicit WebRTC endpoint
wins over native alternatives. Existing experimental NVST version-14 requests,
alliance identities, touch-friendly profiles, TV profiles, and browser requests
retain their request behavior.

The native Java HTTP User-Agent and the official CloudMatch WebView bridge's
User-Agent differ. OpenNOW uses the current native Android app User-Agent; it
does not claim byte-for-byte parity with the WebView bridge. No OAuth client or
saved authentication migration is part of this fix.

## Validation

On October 28, 2026, the debug APK was installed on the POCO X5 Pro. With both
Website client mode and experimental NVST disabled, a resumed Satisfactory cloud
session and a separately created session reached decoded 1376x640 H.264 video at
approximately 59–60 FPS. The native input handshake completed, the stream menu
opened, and exiting the resumed session stopped the local transport before the
cloud DELETE completed. The observed CREATE and RESUME requests used the native
version-1 profile above. The cloud Epic sign-in screen was rendered; third-party
game-account sign-in and Battlefield 1 gameplay were not tested.

`:app:testDebugUnitTest` passed 1,372 tests and `:app:assembleDebug` passed.
The earlier first-launch tutorial overlay instrumentation check also passed.
This is device confirmation for the reported Xiaomi startup failure, not proof
that every reported native crash has the same cause. The separate build-161
remote ICE JNI abort is documented in `remote-ice-crash.md`.
