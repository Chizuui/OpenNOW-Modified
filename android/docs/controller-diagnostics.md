# Controller mapping diagnostics

Controller evidence is included automatically in diagnostic exports and the saved last completed
stream's `input.state` / `input.events`. No debug build or verbose logging switch is required.

- `controller.capabilities.<inputId>` records Android's controller name, vendor/product IDs,
  source flags, supported button codes, and motion ranges (min/max/flat/fuzz/resolution and source).
  It is sampled on connection changes and every 30 seconds from the existing controller scan.
  Capability queries run on an IO worker, with one batch of at most four controllers in flight
  and no queued backlog. `inputId` is an ephemeral Android input handle, not a persistent identifier;
  hardware descriptors, serial numbers, and Bluetooth addresses are not collected.
- `controller.raw-key.*` separates devices, Android key codes, raw scan codes, press/release,
  input readiness, gamepad routing, and handling outcomes. Each detail includes the standard
  button mask and the mask after user settings, trigger identity, mouse mode, channel state,
  delivery delay, and dispatch time. Unsupported/numbered controller keys are included even if
  they miss the gamepad router. Key auto-repeat is excluded. `handled` describes the gamepad
  handler, not server receipt or subsequent keyboard/UI fallback.
- `controller.motion.<slot>.<active|neutral>` includes raw X/Y/Z/RZ/RX/RY, hat, LTRIGGER/RTRIGGER/BRAKE/GAS values,
  selected stick pairs, trigger availability, processed physical stick/trigger/button state,
  mouse modes, physical input settings, and send acceptance. Active and neutral output samples
  have separate sampling keys, so a quick stick release can be captured immediately after movement.
  This is the physical contribution; existing `controller.packet.*` evidence shows the merged
  physical/virtual packet. Send acceptance does not prove game/server receipt.
- `controller.motion-not-ready.*` records motion reaching the streamer before input is ready.
- `controller.capture` describes the sampling limits and the count of details suppressed by
  the global rate cap. This is sampled evidence, not an exhaustive input trace.

Controller evidence has reserved capacity: 128 retained records and 64 recent samples, separate
from general mouse/transport diagnostics. Messages are capped at 4,096 characters. Each key or
motion signature formats at most once per second; the controller recorder also permits at most
32 details per one-second window across all signatures. Repeated observations update counts
without formatting messages. `timestamp`/`uptimeMs` date the sampled detail, while `count` and
`lastSeenUptimeMs` cover observations for that retained key. Counts reset on eviction. Both
buffers evict oldest records at capacity; unrelated input cannot consume the reserved capacity.

Extra axis reads and input string formatting are inside lazy sampling suppliers. Capability
queries run outside the input thread and diagnostic lock. There is no per-event file/network write, logcat output, new polling loop, or input-event
backlog. Export serialization/redaction uses the existing report path. All raw event access is
synchronous; recycled Android events are never queued for later reading.

For a mapping report, reproduce with only the affected controller connected, press and release
each affected button, move each stick and trigger individually, then export diagnostics. Record
which physical button was pressed: raw codes alone cannot establish the label on the hardware.
Earlier records can be evicted during long sessions; exporting shortly after reproduction helps.
