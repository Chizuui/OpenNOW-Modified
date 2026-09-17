# Mjolnir private NACK-v2

The dedicated Mjolnir video route sends retransmission requests as ServerControl
command `0x0317` on `control_channel_partially_reliable`. It does not encode these
requests as RTCP Generic NACK. The bundle-only route retains its existing RTCP
Generic NACK sender on `rtcp_on_sctp_private`.

## Wire format

The existing ServerControl header contains a little-endian `u16` command code and
a little-endian `u16` payload length. The NACK-v2 payload is:

| Offset | Type | Meaning |
| --- | --- | --- |
| 0 | `u8` | Version, always 2 |
| 1 | `u8` | Zero-based video stream ordinal, 0 for OpenNOW's single video stream |
| 2 | `u8` | Number of records |
| 3 + 10n | `u16LE` | Missing base RTP sequence for record n |
| 5 + 10n | `u64LE` | Bitmap for base+1 through base+64, modulo 65536 |

Each base is implicitly missing. Bit j requests base+1+j; unset bits do not
request those packets. The payload length is `3 + 10 * record_count`. These
records contain no frame identifiers, timestamps, or SSRCs.

The encoder accepts 1 through 64 missing sequence numbers per command. Sparse
loss can produce multiple records, up to 64; the maximum framed command is 647
bytes. A run of 64 consecutive missing packets occupies one record with 63 set
bits, not 64. Empty or oversized batches are rejected rather than truncated.

## Routing and recovery ownership

`NvstVideoConfig.mjolnir_udp_port` selects the dedicated Mjolnir route.
`send_pending_nack` checks the selected channel before taking a pending request,
uses the existing shared send budget, and restores the attempt after admission
failure. An unavailable channel does not trigger a fallback to another dialect.
The private route does not depend on the RTCP channel being open.

This change retains the 64-packet batch limit, 4 ms retry interval, three-attempt
limit, 52 ms tracking timeout, and existing reference recovery. Receiver Reports,
PLI, IDR, and the shared channel profile are unchanged.

OpenNOW's partial control channel is unordered with a 300 ms lifetime. The
inspected official default is ordered with two SCTP retransmissions. Reusing the
named channel matches routing, not that default reliability policy. Changing the
shared input channel requires separate validation.

## Evidence and limits

The format was independently reconstructed from static analysis of Linux
`libBifrost2.so`, SHA-256
`8400714f98b7db928ef4377515b1ed35be12523b7fa306566e776b76965537c9`.
Addresses below are ELF virtual addresses in that artifact:

- `0x2b2c88` dispatches command `0x0317`; `0x2b74d9` through `0x2b7531`
  serialize the version, stream ordinal, count, and `3 + 10N` length.
- `0x3cea94` calls record producer `0x4499ee`; `0x449be3` and `0x449be8`
  store the bitmap and base sequence.
- `0x3cf030` through `0x3cf1b0` select the NACK-v2 queue; `0x3cf238` through
  `0x3cf3b8` attach the Mjolnir receiver to that queue.
- `0x2c391a` maps logical channel 1 to the channel created at `0x2c4769`
  through `0x2c478f` with label `control_channel_partially_reliable`.
  Logical channel 1 is not SCTP SID 1; OpenNOW's profile uses SID 6.
- `0x284c62` initializes the video ordinal to zero. `0x2b6b14` stores it in
  the feedback owner, and `0x2b74e5` writes its low byte into payload byte 1.

The unit fixtures are authored from this format, not copied packet captures.
`nvst_control` tests cover exact bytes and bounded round trips.
`nvst_nack_tests` uses paired DTLS/SCTP peers to verify delivery, route isolation,
closed channels, admission failure, retry limits, and expiry.

These checks do not establish the active Windows configuration or server
retransmission behavior. A controlled live session must correlate a requested
missing RTP sequence with an authenticated returned packet and successful
recovery before attributing a reduction in hitching to this change. The stderr
diagnostic `NVST NACK sent format=private-v2` records local send admission, not a
server acknowledgement.
