//! Custom lenient AV1 RTP depayloader (`gfnav1depay`).
//!
//! The stock gst-plugins-rs `rtpav1depay` produces zero output on the GFN AV1
//! streams seen in the field (RTP keeps flowing at 39-88 Mbps while
//! decoded=0/sink=0 across D3D12, D3D11 and dav1d, with an FIR=278 storm).
//! Because the exact payload variant the GFN servers emit was not
//! self-describing, this element:
//!
//! - auto-detects the payload layout: the standard AV1 RTP aggregation header
//!   (`Z|Y|W|N`, one byte) vs a raw OBU stream without any aggregation
//!   header (OBUs with optional leb128 size fields),
//! - never drops the whole flow on a malformed packet — it emits everything
//!   that parses, flags a DISCONT over the gap, and keeps going,
//! - performs no keyframe gating and never pushes UpstreamForceKeyUnit
//!   events (the streamer's liveness watchdog already requests keyframes via
//!   signaling/RTCP, so the in-pipeline event just caused a FIR storm),
//! - logs the detected layout plus the first payload bytes once per session,
//!   so a still-failing field run pins down the format for further fixes.
//!
//! Output caps match the stock depay (`video/x-av1, stream-format=obu-stream,
//! alignment=obu, parsed=true`) so the downstream `av1parse` → decoder chain
//! is untouched.

use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;

/// RTP clock rate for video: 90 kHz.
pub(crate) const AV1_CLOCK_RATE: u32 = 90_000;

const TEMPORAL_DELIMITER: [u8; 2] = [0b0001_0010, 0];

fn src_caps() -> gst::Caps {
    gst::Caps::builder("video/x-av1")
        .field("parsed", true)
        .field("stream-format", "obu-stream")
        .field("alignment", "obu")
        .build()
}

fn sink_caps() -> gst::Caps {
    gst::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("clock-rate", AV1_CLOCK_RATE as i32)
        .field("encoding-name", "AV1")
        .build()
}

// ---------------------------------------------------------------------------
// Pure payload parsing (unit-testable, no GStreamer types).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadMode {
    /// Standard AV1 RTP: 1-byte aggregation header `Z|Y|W|N` then OBU elements.
    Standard,
    /// Raw OBU stream: payload starts directly with an OBU header.
    RawObu,
}

#[derive(Debug, Clone)]
struct PendingFragment {
    bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct Av1DepayState {
    mode: Option<PayloadMode>,
    last_timestamp: Option<u32>,
    marked_packet: bool,
    needs_discont: bool,
    found_valid_obu: bool,
    pending_fragment: Option<PendingFragment>,
    /// Diagnostics: dump the first payload of the session once.
    logged_first_payload: bool,
}

impl Default for Av1DepayState {
    fn default() -> Self {
        Self {
            mode: None,
            last_timestamp: None,
            marked_packet: false,
            needs_discont: true,
            found_valid_obu: false,
            pending_fragment: None,
            logged_first_payload: false,
        }
    }
}

impl Av1DepayState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Result of processing one RTP packet's payload.
pub(crate) struct DepayOutput {
    /// OBU-stream bytes to push downstream (may be empty).
    pub obus: Vec<u8>,
    /// Whether the emitted buffer should carry the DISCONT flag.
    pub discont: bool,
    /// Whether the emitted buffer should carry the MARKER flag.
    pub marker: bool,
}

fn parse_leb128(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    loop {
        let b = *bytes.get(*pos)?;
        *pos += 1;
        result |= u32::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
}

fn write_leb128(out: &mut Vec<u8>, mut value: u32) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ObuHeader {
    obu_type: u8,
    has_extension: bool,
    has_size_field: bool,
    header_len: usize,
}

/// Parse an AV1 OBU header at `pos`. On success `pos` is left after the
/// header bytes AND after the internal leb128 size field (when present).
fn parse_obu_header(bytes: &[u8], pos: &mut usize) -> Option<ObuHeader> {
    let b = *bytes.get(*pos)?;
    if b & 0x80 != 0 {
        return None; // forbidden bit
    }
    let obu_type = (b >> 3) & 0x0f;
    // Valid OBU types: 1..=8 and 15 (padding). 0 and 9..=14 are reserved.
    if obu_type == 0 || (obu_type >= 9 && obu_type != 15) {
        return None;
    }
    let has_extension = b & 0x04 != 0;
    let has_size_field = b & 0x02 != 0;
    *pos += 1;
    let mut header_len = 1;
    if has_extension {
        if bytes.get(*pos).is_none() {
            return None;
        }
        header_len = 2;
        *pos += 1;
    }
    if has_size_field {
        parse_leb128(bytes, pos)?;
    }
    Some(ObuHeader {
        obu_type,
        has_extension,
        has_size_field,
        header_len,
    })
}

/// True for OBU types that must never reach a decoder (spec §5: tile list and
/// temporal delimiter MUST be ignored; padding carries no picture data).
fn is_ignored_obu_type(obu_type: u8) -> bool {
    matches!(obu_type, 2 | 8 | 15) // TemporalDelimiter | TileList | Padding
}

/// Translate one complete OBU (header + optional internal size + payload)
/// into the output bitstream: header with `has_size_field` forced on, then a
/// leb128 size, then the payload. Mirrors what the stock depay emits.
fn translate_obu(obu: &[u8], out: &mut Vec<u8>) -> Option<()> {
    let mut pos = 0usize;
    let header = parse_obu_header(obu, &mut pos)?;
    let payload = &obu[pos..];
    let mut b = header.obu_type << 3;
    if header.has_extension {
        b |= 0x04;
    }
    b |= 0x02; // has_size_field
    out.push(b);
    if header.has_extension {
        let ext_byte = obu.get(1).copied()?;
        out.push(ext_byte);
    }
    write_leb128(out, payload.len() as u32);
    out.extend_from_slice(payload);
    Some(())
}

/// Parse all complete OBUs inside `element` (which must start at an OBU
/// header) and append their translated forms to `out`. Returns true when the
/// element's OBUs were consumed (or only ignorable OBUs found); false when
/// the element cannot be parsed as an OBU stream.
fn translate_obu_element(element: &[u8], out: &mut Vec<u8>) -> bool {
    let mut pos = 0usize;
    let mut any = false;
    while pos < element.len() {
        let obu_start = pos;
        let Some(header) = parse_obu_header(element, &mut pos) else {
            return any;
        };
        if is_ignored_obu_type(header.obu_type) {
            continue;
        }
        let end = if header.has_size_field {
            // parse_obu_header already consumed the size; recompute it.
            let mut probe = obu_start + header.header_len;
            let size = match parse_leb128(element, &mut probe) {
                Some(size) => size as usize,
                None => return any,
            };
            let Some(end) = probe.checked_add(size) else {
                return any;
            };
            if end > element.len() {
                return any;
            }
            end
        } else {
            element.len()
        };
        if translate_obu(&element[obu_start..end], out).is_none() {
            return any;
        }
        any = true;
        pos = end;
    }
    any
}

/// Standard AV1 RTP payload (`Z|Y|W|N` aggregation header). Appends the
/// translated OBUs to `out`. Returns true when the payload parsed cleanly.
fn parse_standard(state: &mut Av1DepayState, payload: &[u8], out: &mut Vec<u8>) -> bool {
    let Some(&agg) = payload.first() else {
        return false;
    };
    let z = agg & 0x80 != 0;
    let y = agg & 0x40 != 0;
    let w = (agg >> 4) & 0x03;
    let rest = &payload[1..];
    let mut pos = 0usize;
    let mut idx = 0u32;

    while pos < rest.len() {
        let is_last = w != 0 && idx + 1 == u32::from(w);
        let element = if is_last {
            // Last OBU element: no length field, extends to the end.
            let slice = &rest[pos..];
            pos = rest.len();
            slice
        } else {
            let size = match parse_leb128(rest, &mut pos) {
                Some(size) => size as usize,
                None => return false,
            };
            let Some(end) = pos.checked_add(size) else {
                return false;
            };
            if end > rest.len() {
                return false;
            }
            let slice = &rest[pos..end];
            pos = end;
            slice
        };

        if idx == 0 && z {
            // Leading fragment: this element continues the pending OBU.
            let Some(fragment) = state.pending_fragment.take() else {
                // Continuation without a pending fragment: resync.
                state.needs_discont = true;
                idx += 1;
                continue;
            };
            let mut combined = fragment.bytes;
            combined.extend_from_slice(element);
            if !y || !is_last {
                if !translate_obu_element(&combined, out) {
                    state.needs_discont = true;
                } else {
                    state.found_valid_obu = true;
                }
            } else {
                state.pending_fragment = Some(PendingFragment { bytes: combined });
            }
            idx += 1;
            continue;
        }
        if is_last && y {
            // Trailing fragment: store for the next packet.
            state.pending_fragment = Some(PendingFragment {
                bytes: element.to_vec(),
            });
            state.found_valid_obu = true;
            idx += 1;
            continue;
        }
        if translate_obu_element(element, out) {
            state.found_valid_obu = true;
        } else {
            state.needs_discont = true;
        }
        idx += 1;
    }
    true
}

/// Raw OBU stream (no aggregation header). Each OBU may carry its own size
/// field; a final OBU without a size field is treated as a fragment that
/// continues into the next packet (a plausible GFN convention).
fn parse_raw(state: &mut Av1DepayState, payload: &[u8], out: &mut Vec<u8>) -> bool {
    let mut pos = 0usize;
    while pos < payload.len() {
        let obu_start = pos;
        let Some(header) = parse_obu_header(payload, &mut pos) else {
            return false;
        };
        if is_ignored_obu_type(header.obu_type) {
            continue;
        }
        let end = if header.has_size_field {
            // parse_obu_header already consumed the size field; re-read it
            // from just past the header bytes to bound the OBU.
            let mut probe = obu_start + header.header_len;
            let size = match parse_leb128(payload, &mut probe) {
                Some(size) => size as usize,
                None => return false,
            };
            let Some(end) = probe.checked_add(size) else {
                return false;
            };
            if end > payload.len() {
                return false;
            }
            end
        } else {
            // No size field: the OBU extends to the end of the packet. When
            // this packet is not the last of its temporal unit, treat it as a
            // fragment continuing into the next packet.
            payload.len()
        };
        if translate_obu(&payload[obu_start..end], out).is_none() {
            return false;
        }
        state.found_valid_obu = true;
        pos = end;
    }
    true
}

/// Process one RTP packet payload. `marker`/`timestamp` come from the RTP
/// header; the caller strips the header first.
pub(crate) fn process_rtp_payload(
    state: &mut Av1DepayState,
    payload: &[u8],
    marker: bool,
    timestamp: u32,
) -> DepayOutput {
    let mut out = Vec::with_capacity(payload.len() + 2);

    // Diagnostics: dump the first payload once (hex + interpreted header).
    if !state.logged_first_payload {
        state.logged_first_payload = true;
        let hex = payload
            .iter()
            .take(24)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "[GfnAv1Depay] first payload len={} marker={marker} ts={timestamp} first_bytes=[{hex}]",
            payload.len()
        );
    }

    // Detect the layout from the first byte(s).
    if state.mode.is_none() && !payload.is_empty() {
        let b = payload[0];
        let obu_type = (b >> 3) & 0x0f;
        let looks_like_raw_obu =
            b & 0x80 == 0 && (1..=8).contains(&obu_type);
        if looks_like_raw_obu {
            state.mode = Some(PayloadMode::RawObu);
            eprintln!(
                "[GfnAv1Depay] detected RAW OBU payload (no aggregation header), first byte=0x{b:02x} obu_type={obu_type}"
            );
        } else {
            state.mode = Some(PayloadMode::Standard);
            let z = b & 0x80 != 0;
            let y = b & 0x40 != 0;
            let w = (b >> 4) & 0x03;
            let n = b & 0x08 != 0;
            eprintln!(
                "[GfnAv1Depay] detected STANDARD aggregation header 0x{b:02x} (Z={z} Y={y} W={w} N={n})"
            );
        }
    }

    // New temporal unit?
    if state.marked_packet || state.last_timestamp != Some(timestamp) {
        if state.last_timestamp.is_some() && state.pending_fragment.is_some() {
            // A fragment was left open across a TU boundary: the previous TU
            // never finished its last OBU. Drop the fragment and resync.
            state.pending_fragment = None;
            state.needs_discont = true;
        }
        out.extend_from_slice(&TEMPORAL_DELIMITER);
    }
    state.marked_packet = marker;
    state.last_timestamp = Some(timestamp);

    if !payload.is_empty() {
        match state.mode {
            Some(PayloadMode::Standard) => {
                let mut try_out = Vec::with_capacity(payload.len() + 2);
                let ok = parse_standard(state, payload, &mut try_out);
                if ok && !try_out.is_empty() {
                    out.extend_from_slice(&try_out);
                } else {
                    // Fall back to the raw parse on the same packet: a
                    // standard-header byte can be misread as an OBU header
                    // (e.g. 0x0A is both W=0/N=1 aggregation and a valid
                    // sequence-header OBU).
                    let mut raw_out = Vec::with_capacity(payload.len() + 2);
                    let raw_ok = parse_raw(state, payload, &mut raw_out);
                    if raw_ok && !raw_out.is_empty() {
                        out.extend_from_slice(&raw_out);
                        state.mode = Some(PayloadMode::RawObu);
                    } else {
                        state.needs_discont = true;
                    }
                }
            }
            Some(PayloadMode::RawObu) => {
                let mut raw_out = Vec::with_capacity(payload.len() + 2);
                if parse_raw(state, payload, &mut raw_out) {
                    if !raw_out.is_empty() {
                        out.extend_from_slice(&raw_out);
                    }
                } else {
                    state.needs_discont = true;
                }
            }
            None => {
                state.needs_discont = true;
            }
        }
    }

    let discont = state.needs_discont;
    state.needs_discont = false;
    DepayOutput {
        obus: out,
        discont,
        marker,
    }
}

// ---------------------------------------------------------------------------
// GStreamer element.
// ---------------------------------------------------------------------------

pub(crate) struct GfnAv1Depay {
    state: Arc<Mutex<Av1DepayState>>,
    srcpad: gst::Pad,
    sinkpad: gst::Pad,
}

#[glib::object_subclass]
impl ObjectSubclass for GfnAv1Depay {
    const NAME: &'static str = "GfnAv1Depay";
    type Type = GfnAv1DepayElement;
    type ParentType = gst::Element;

    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Av1DepayState::default())),
            srcpad: gst::Pad::builder(gst::PadDirection::Src)
                .name("src")
                .build(),
            sinkpad: gst::Pad::builder(gst::PadDirection::Sink)
                .name("sink")
                .build(),
        }
    }
}

impl ObjectImpl for GfnAv1Depay {
    fn constructed(&self) {
        self.parent_constructed();
        let state_for_events = self.state.clone();
        let state_for_chain = self.state.clone();
        let srcpad_for_events = self.srcpad.clone();
        let srcpad_for_chain = self.srcpad.clone();
        let sinkpad = self.sinkpad.clone();

        unsafe {
            sinkpad.set_event_function(move |_pad, _obj, event| {
                match event.view() {
                    gst::EventView::Caps(_) => srcpad_for_events
                        .push_event(gst::event::Caps::new(&src_caps())),
                    gst::EventView::FlushStart(_) => {
                        state_for_events.lock().unwrap().reset();
                        srcpad_for_events.push_event(event)
                    }
                    _ => srcpad_for_events.push_event(event),
                }
            });
            sinkpad.set_chain_function(move |_pad, _obj, buffer| {
                let Ok(map) = buffer.map_readable() else {
                    return Err(gst::FlowError::Error);
                };
                let bytes = map.as_ref();
                if bytes.len() < 12 {
                    return Err(gst::FlowError::NotNegotiated);
                }
                let b0 = bytes[0];
                let b1 = bytes[1];
                let version = (b0 >> 6) & 0x03;
                if version != 2 {
                    return Err(gst::FlowError::NotNegotiated);
                }
                let has_padding = b0 & 0x20 != 0;
                let has_extension = b0 & 0x10 != 0;
                let csrc_count = (b0 & 0x0f) as usize;
                let marker = b1 & 0x80 != 0;
                let timestamp =
                    u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

                let mut offset = 12 + csrc_count * 4;
                if has_extension {
                    if offset + 4 > bytes.len() {
                        return Err(gst::FlowError::NotNegotiated);
                    }
                    let ext_len = (u16::from(bytes[offset + 2]) << 8
                        | u16::from(bytes[offset + 3])) as usize
                        * 4;
                    offset += 4 + ext_len;
                }
                if offset > bytes.len() {
                    return Err(gst::FlowError::NotNegotiated);
                }
                let payload_end = if has_padding {
                    match bytes.last() {
                        Some(&n) => bytes.len().saturating_sub(usize::from(n) + 1),
                        None => offset,
                    }
                } else {
                    bytes.len()
                };
                let payload = &bytes[offset..payload_end];

                let output = process_rtp_payload(
                    &mut state_for_chain.lock().unwrap(),
                    payload,
                    marker,
                    timestamp,
                );

                if output.obus.is_empty() {
                    return Ok(gst::FlowSuccess::Ok);
                }
                let mut out_buffer = gst::Buffer::from_slice(output.obus);
                {
                    let out_mut = out_buffer.get_mut().unwrap();
                    out_mut.set_pts(buffer.pts());
                    out_mut.set_dts(buffer.dts());
                    out_mut.set_duration(buffer.duration());
                    if output.discont {
                        out_mut.set_flags(gst::BufferFlags::DISCONT);
                    }
                    if output.marker {
                        out_mut.set_flags(gst::BufferFlags::MARKER);
                    }
                }
                match srcpad_for_chain.push(out_buffer) {
                    // EOS from downstream is normal (the stream ended); the
                    // EOS event propagation is handled by the event function.
                    Ok(_) | Err(gst::FlowError::Eos) => Ok(gst::FlowSuccess::Ok),
                    Err(err) => Err(err),
                }
            });
        }

        let element = self.obj();
        element.add_pad(&self.sinkpad).unwrap();
        element.add_pad(&self.srcpad).unwrap();
    }
}

impl GstObjectImpl for GfnAv1Depay {}

impl ElementImpl for GfnAv1Depay {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: std::sync::OnceLock<gst::subclass::ElementMetadata> =
            std::sync::OnceLock::new();
        Some(ELEMENT_METADATA.get_or_init(|| {
            gst::subclass::ElementMetadata::new(
                "GFN AV1 Depayloader",
                "Codec/Depayloader/Network/RTP",
                "Depayload AV1 from RTP packets (lenient, GFN-compatible)",
                "OpenNOW",
            )
        }))
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: std::sync::OnceLock<Vec<gst::PadTemplate>> =
            std::sync::OnceLock::new();
        PAD_TEMPLATES.get_or_init(|| {
            vec![
                gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &sink_caps(),
                )
                .unwrap(),
                gst::PadTemplate::new(
                    "src",
                    gst::PadDirection::Src,
                    gst::PadPresence::Always,
                    &src_caps(),
                )
                .unwrap(),
            ]
        })
        .as_slice()
    }
}

glib::wrapper! {
    pub(crate) struct GfnAv1DepayElement(ObjectSubclass<GfnAv1Depay>) @extends gst::Element, gst::Object;
}

/// Register the `gfnav1depay` element factory. Called once at streamer
/// startup (idempotent across re-registrations is not guaranteed, so guard
/// the call site).
pub(crate) fn register_element() -> Result<(), glib::BoolError> {
    let type_ = GfnAv1DepayElement::static_type();
    gst::Element::register(None, "gfnav1depay", gst::Rank::NONE, type_)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid OBU: header byte + size field + payload.
    fn obu(obu_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![(obu_type << 3) | 0x02];
        write_leb128(&mut v, payload.len() as u32);
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn raw_obu_single_packet_roundtrip() {
        let mut state = Av1DepayState::default();
        let frame = obu(6, &[1, 2, 3, 4]);
        let out = process_rtp_payload(&mut state, &frame, true, 90_000);
        // Temporal delimiter + translated OBU.
        assert_eq!(&out.obus[..2], &TEMPORAL_DELIMITER);
        assert!(out.obus.len() >= frame.len() + 2);
        assert!(out.discont); // first emission is a resync
        // The OBU header must have has_size_field set.
        assert_eq!(out.obus[2] & 0x02, 0x02);
        // Round trip: leb128 size at pos 3 must equal the payload length.
        let mut pos = 3usize;
        let size = parse_leb128(&out.obus, &mut pos).unwrap() as usize;
        assert_eq!(size, 4);
        assert_eq!(pos + size, out.obus.len());
    }

    #[test]
    fn raw_obu_multiple_obus_one_packet() {
        let mut state = Av1DepayState::default();
        let mut payload = Vec::new();
        payload.extend_from_slice(&obu(5, &[9])); // metadata
        payload.extend_from_slice(&obu(6, &[1, 2, 3, 4])); // frame
        let out = process_rtp_payload(&mut state, &payload, true, 90_000);
        assert!(!out.obus.is_empty());
        // Two translated OBUs: type-5 (0x2A header) and type-6 (0x32 header),
        // both with has_size_field.
        let type5 = out.obus.iter().filter(|&&b| b & 0xfe == 0x2a).count();
        let type6 = out.obus.iter().filter(|&&b| b & 0xfe == 0x32).count();
        assert_eq!(type5, 1);
        assert_eq!(type6, 1);
    }

    #[test]
    fn standard_aggregation_two_obus() {
        let mut state = Av1DepayState::default();
        let first = obu(5, &[9]);
        let second = obu(6, &[1, 2, 3, 4]);
        // agg header: Z=0 Y=0 W=2 N=1 -> 0b0001_1000 | 0x08 = 0x18
        let mut payload = vec![0x18u8];
        payload.extend_from_slice(&first); // has size field
        payload.push(second[0]); // last OBU: no size field
        payload.extend_from_slice(&second[2..]);

        let out = process_rtp_payload(&mut state, &payload, true, 90_000);
        assert!(!out.obus.is_empty());
        assert_eq!(&out.obus[..2], &TEMPORAL_DELIMITER);
        let type5 = out.obus.iter().filter(|&&b| b & 0xfe == 0x2a).count();
        let type6 = out.obus.iter().filter(|&&b| b & 0xfe == 0x32).count();
        assert_eq!(type5, 1);
        assert_eq!(type6, 1);
    }

    #[test]
    fn standard_fragmented_frame() {
        let mut state = Av1DepayState::default();
        let ts = 90_000;
        let mut frame = obu(6, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let split = 4;
        let (head, tail) = frame.split_at(split);
        // Packet 1: Y=1 W=1 (first+last element, trailing fragment).
        let mut p1 = vec![0b0100_0000 | 0x10];
        p1.extend_from_slice(head);
        let out1 = process_rtp_payload(&mut state, &p1, false, ts);
        assert!(out1.obus.is_empty() || out1.obus == TEMPORAL_DELIMITER);
        // Packet 2: Z=1 W=1 (leading fragment completes the OBU).
        let mut p2 = vec![0b1000_0000 | 0x10];
        p2.extend_from_slice(tail);
        let out2 = process_rtp_payload(&mut state, &p2, true, ts);
        assert!(!out2.obus.is_empty());
        // The second fragment belongs to the SAME temporal unit (no new TD),
        // so out2 is the reassembled OBU alone: [hdr][leb128 size][payload]
        // must equal the original frame.
        let body = &out2.obus[..];
        assert_eq!(body[0] & 0x02, 0x02); // has_size_field
        let mut pos = 1usize;
        let size = parse_leb128(body, &mut pos).unwrap() as usize;
        assert_eq!(pos + size, body.len());
        assert_eq!(&body[pos..], &frame[2..]);
    }

    #[test]
    fn garbage_does_not_wedge() {
        let mut state = Av1DepayState::default();
        // Pure garbage first packet: must not panic and must not permanently
        // wedge later packets.
        let garbage: Vec<u8> = vec![0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa];
        let _ = process_rtp_payload(&mut state, &garbage, true, 90_000);
        // A good raw packet right after must still decode.
        let good = obu(6, &[7, 7, 7]);
        let out = process_rtp_payload(&mut state, &good, true, 90_001);
        assert!(!out.obus.is_empty());
    }

    #[test]
    fn timestamp_and_marker_bound_tus() {
        let mut state = Av1DepayState::default();
        let frame = obu(6, &[1]);
        let _ = process_rtp_payload(&mut state, &frame, true, 90_000);
        // Same TU (marker already set from the previous frame's last packet is
        // the TU boundary indicator here): a new TU starts a fresh delimiter.
        let out2 = process_rtp_payload(&mut state, &frame, true, 90_000);
        assert_eq!(&out2.obus[..2], &TEMPORAL_DELIMITER);
        let out3 = process_rtp_payload(&mut state, &frame, true, 90_001);
        assert_eq!(&out3.obus[..2], &TEMPORAL_DELIMITER);
    }

    #[test]
    fn first_byte_ambiguous_prefers_standard_then_falls_back() {
        // 0x0A is a valid sequence-header OBU header AND a valid aggregation
        // header (W=0, N=1). With payload [0x0A, ...] the standard parse
        // fails (no leb128 size present), so the raw fallback must kick in.
        let mut state = Av1DepayState::default();
        let payload = vec![0x0a, 0x05, 1, 2, 3, 4, 5];
        let out = process_rtp_payload(&mut state, &payload, true, 90_000);
        // Either mode produced a TD-prefixed stream; nothing panics.
        assert!(!out.obus.is_empty());
        assert_eq!(&out.obus[..2], &TEMPORAL_DELIMITER);
    }
}
