# Remote ICE candidate JNI abort

The October 18, 2026 Play Console samples for Android 2.0.5/build 161 share
these ARM64 WebRTC offsets across multiple devices and Android versions:

| Frame | Library offset | Matching binary operation |
| --- | --- | --- |
| `nativeAddIceCandidate+60` | `0x878a28` | First Java string conversion: `sdpMid` |
| String conversion | `0x396f64` | Check Java exception after `JniHelper.getStringBytes(sdpMid)` |
| JNI exception handler | `0x395054` | Fatal check following the Java exception |
| Fatal logging | `0x399724` | Enter fatal logger |
| Abort | `0x398ed4` | Call `abort()` |

The locally cached `io.github.webrtc-sdk:android:144.7559.14` AAR contains
`jni/arm64-v8a/libjingle_peerconnection_so.so` with the exact reported GNU
Build ID, `0cc2410c540e806e`. Its SHA-256 is
`5d68547da3fad7679846187577a0333c10d9ff4d39322bce61f366426924612e`.
Offsets were inspected with the Android NDK's `llvm-objdump`; Java helper
bytecode was inspected with `javap` against the same AAR's `classes.jar`.

The helper calls `str.getBytes("ISO-8859-1")`. A null string throws
`NullPointerException`. The native exception check then aborts the process,
so Kotlin `runCatching` cannot recover it. This occurs before the native
peer handle is obtained or the ICE candidate is parsed.

`StreamSignaling.kt` can construct `IceCandidate` with a null `sdpMid` when
the provider omits it or sends JSON null. `RemoteIceCandidateBuffer` now
normalizes that null to an empty string before buffering or immediate
delivery. It preserves the supplied `sdpMLineIndex` and candidate SDP.
Named media IDs are retained. WebRTC's
[native media-section lookup](https://webrtc.googlesource.com/src/+/refs/heads/main/pc/sdp_offer_answer.cc)
uses the media-line index when MID is empty; inventing a MID such as `"0"`
would instead select by name and could reject a valid candidate.

`RemoteIceCandidateBufferTest` exercises queued and immediate delivery for
null, empty, and named MIDs, passing each result through the actual bundled
`JniHelper.getStringBytes` method. Before the fix, the null case fails with
`NullPointerException`; after normalization it converts successfully and
retains the media-line index and SDP.

The reports contain neither the original Java exception text nor the
candidate payload. The binary trace and regression test establish a matching,
source-reachable null-string failure and its prevention. Confirmation that
these production crash groups stop recurring still requires a release and
affected-device monitoring. Other native crash signatures need their own
matching binary traces.
