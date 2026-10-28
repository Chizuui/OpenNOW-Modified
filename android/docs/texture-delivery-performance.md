# Texture delivery performance

The WebRTC texture path keeps one decoder texture in flight. A slow texture
callback or renderer can therefore cause decoded outputs to be discarded even
when MediaCodec input and output counts match. Keep that bounded behavior: an
unbounded queue would trade stutter for increasing input-to-display latency.

## Current adjustments

`buildSrc/.../WebRtcTextureDelivery.java` instruments the pinned
`io.github.webrtc-sdk:android:144.7559.14` dependency in debug and release builds:

- `AndroidVideoDecoder` uses cached entry points for `SurfaceTextureHelper` size
  and rotation updates. These forward the first request and subsequent changes,
  but skip identical requests. Upstream calls
  `setDefaultBufferSize` and posts a handler message for every size request, plus
  another message for rotation. The decoder calls both for every delivered
  output, even at a fixed size and rotation. Original setters used by camera and
  other callers remain unchanged.
- The cache tracks requested values, not values already applied by the worker.
  This prevents redundant posts when the worker is delayed. Each helper owns its
  cache, so a new decoder/helper receives its initial configuration. Serialized
  setters preserve request order and cache only after upstream accepts a call.
- The `decoder-texture-thread` helper and EGL display workers use Android's
  `THREAD_PRIORITY_DISPLAY` (-4). Other SurfaceTextureHelper users, including
  camera capture, retain their default thread priority. Audio scheduling is
  unchanged.

Frame ownership, EGL locking, GPU synchronization, frame dropping, codec choice,
resolution, FPS, and output-path selection remain upstream/user controlled.
There are no extra frame copies or buffers. The transform fails the build if
the expected setters or HandlerThread constructors change; review it when
upgrading WebRTC, and remove it if upstream supplies equivalent behavior.

Source reference: the WebRTC SDK's
[Android source](https://github.com/webrtc-sdk/webrtc/tree/m144_release/sdk/android),
particularly `SurfaceTextureHelper`, `EglThread`, and `AndroidVideoDecoder`.

## Verification and device comparison

BuildSrc tests execute transformed classes to check duplicate suppression,
queued transitions, initialization after reconnect, invalid arguments, failure
retries, and worker priorities. An Android APK build also applies the transform
to the actual pinned dependency rather than only the test fixtures.

For a device comparison, keep the game scene, codec, resolution, FPS, bitrate,
network, and video output unchanged. Compare the previous build with the tuned
build for several minutes, including touch/controller input and resolution
changes. Examine received/decoded FPS and deltas of `outputBuffers`,
`textureSubmissions`, and `discardedOutputBuffers`. Texture submissions are not
proof of physical display presentation. Repeat with Direct (MediaCodec) as a
separate control if texture drops persist.

Automated tests verify the optimization and lifecycle behavior, not a phone's
FPS improvement. Driver stalls, GPU load, incoming-frame gaps, and server game
FPS can still limit smoothness; the reported itel A665L needs a comparison run.
