//! GStreamer/libnice ICE observability: candidate + STUN/state logging for
//! NAT debugging, mirroring the official client's transport logging
//! ("Advertising ICE candidate", "Incoming candidate ... typ srflx", selected
//! pair). Everything is routed through the existing `Event::Log` channel so it
//! shows up in the app log export next to the other `[NativeStreamer]` lines.

use std::sync::mpsc::Sender;

use gstreamer as gst;
use gst::glib;
use gst::glib::prelude::*;

use crate::gstreamer_backend::send_log;
use crate::gstreamer_pipeline::glib_value_to_u32;
use crate::ice_observability::{
    candidate_type_name, component_state_name, lookup_candidate_address, register_ice_candidate,
    transport_name,
};
use crate::protocol::Event;

/// Attach ICE observability to a freshly built webrtcbin:
/// - every local candidate gathered (`on-ice-candidate`) is logged and
///   registered by foundation,
/// - libnice agent signals (selected pair, component state, STUN binding
///   received) are mirrored into the app log.
///
/// Fully defensive: if the ICE backend is not the bundled libnice one (no
/// `ice-agent`/`agent` properties), only the candidate logging stays active and
/// a single debug line explains why the agent signals are unavailable.
pub(crate) fn wire_ice_observability(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) {
    if event_sender.is_none() {
        return;
    }

    // Local candidates: same signal the pipeline already uses for
    // Event::LocalIce; here we only add the observability half.
    webrtc.connect("on-ice-candidate", false, {
        let event_sender = event_sender.clone();
        move |values| {
            let candidate = values
                .get(2)
                .and_then(|value| value.get::<String>().ok())
                .unwrap_or_default();
            if let Some(info) = register_ice_candidate(&candidate) {
                send_log(
                    &event_sender,
                    "info",
                    format!("[ICE] Local candidate gathered: {}", info.describe()),
                );
            }
            None
        }
    });

    wire_agent_observability(webrtc, event_sender);
}

/// Log a remote (server) candidate at `add-ice-candidate` time — the analog of
/// the official client's "Incoming candidate: ... typ srflx".
pub(crate) fn log_remote_ice_candidate(
    candidate_sdp: &str,
    event_sender: &Option<Sender<Event>>,
) {
    if let Some(info) = register_ice_candidate(candidate_sdp) {
        send_log(
            event_sender,
            "info",
            format!("[ICE] Remote candidate added: {}", info.describe()),
        );
    }
}

/// Mirror libnice agent signals into the app log. The `agent` property is the
/// raw `NiceAgent` reachable through webrtcbin's `ice-agent` object property
/// (the bundled gstwebrtcnice backend). All lookups are guarded so a non-nice
/// ICE backend simply disables this half.
fn wire_agent_observability(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    if webrtc.find_property("ice-agent").is_none() {
        send_log(
            &event_sender,
            "debug",
            "[ICE] ice-agent property unavailable; libnice agent signals not wired."
                .to_owned(),
        );
        return;
    }
    let Ok(ice_agent) = webrtc.property_value("ice-agent").get::<glib::Object>() else {
        send_log(
            &event_sender,
            "debug",
            "[ICE] ice-agent object not readable; libnice agent signals not wired.".to_owned(),
        );
        return;
    };
    if ice_agent.find_property("agent").is_none() {
        send_log(
            &event_sender,
            "debug",
            "[ICE] ICE backend has no exposed NiceAgent (not gstwebrtcnice); libnice agent signals not wired."
                .to_owned(),
        );
        return;
    }
    let Ok(agent) = ice_agent.property_value("agent").get::<glib::Object>() else {
        send_log(
            &event_sender,
            "debug",
            "[ICE] NiceAgent not readable from ICE backend; libnice agent signals not wired."
                .to_owned(),
        );
        return;
    };

    // Selected candidate pair: the NAT-relevant fact — which local candidate
    // type reached which remote candidate type (host vs srflx vs relay).
    agent.connect("new-selected-pair-full", false, {
        let event_sender = event_sender.clone();
        move |values| {
            let stream_id = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
            let component_id = values.get(2).and_then(glib_value_to_u32).unwrap_or(0);
            let local = values
                .get(3)
                .and_then(|value| value.get::<glib::Object>().ok());
            let remote = values
                .get(4)
                .and_then(|value| value.get::<glib::Object>().ok());
            let local_desc = describe_candidate_object(&local);
            let remote_desc = describe_candidate_object(&remote);
            send_log(
                &event_sender,
                "info",
                format!(
                    "[ICE] Selected pair (stream={stream_id} component={component_id}): \
                     local {local_desc} <-> remote {remote_desc}"
                ),
            );
            None
        }
    });

    // Component state machine: CONNECTED/READY after hole punching, FAILED on
    // consent loss — the exact signal libnice emits when RFC 7675 consent
    // expires (see the NAT stability analysis).
    agent.connect("component-state-changed", false, {
        let event_sender = event_sender.clone();
        move |values| {
            let stream_id = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
            let component_id = values.get(2).and_then(glib_value_to_u32).unwrap_or(0);
            let state = values.get(3).and_then(glib_value_to_u32).unwrap_or(0);
            send_log(
                &event_sender,
                "info",
                format!(
                    "[ICE] Component {stream_id}/{component_id} state: {}",
                    component_state_name(state)
                ),
            );
            None
        }
    });

    // The peer's first STUN binding request reaching us — NAT hole punch
    // confirmed from the remote side.
    agent.connect("initial-binding-request-received", false, {
        let event_sender = event_sender.clone();
        move |values| {
            let stream_id = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
            send_log(
                &event_sender,
                "info",
                format!("[ICE] Peer STUN binding request received (stream={stream_id}) — NAT hole punch confirmed."),
            );
            None
        }
    });

    // Local candidate gathering completed for a stream.
    agent.connect("candidate-gathering-done", false, move |values| {
        let stream_id = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
        send_log(
            &event_sender,
            "info",
            format!("[ICE] Candidate gathering done (stream={stream_id})."),
        );
        None
    });
}

/// Best-effort description of a `NiceCandidate` GObject: type, transport,
/// priority, foundation, plus the SDP-parsed address from the registry (the
/// address property is a libnice boxed type we do not link against).
fn describe_candidate_object(candidate: &Option<glib::Object>) -> String {
    let Some(candidate) = candidate else {
        return "n/a".to_owned();
    };
    let cand_type = if candidate.find_property("type").is_some() {
        candidate.property::<i32>("type")
    } else {
        0
    };
    let transport = if candidate.find_property("transport").is_some() {
        candidate.property::<i32>("transport")
    } else {
        0
    };
    let foundation = if candidate.find_property("foundation").is_some() {
        candidate.property::<String>("foundation")
    } else {
        String::new()
    };
    let priority = if candidate.find_property("priority").is_some() {
        candidate.property::<u32>("priority")
    } else {
        0
    };
    let address = lookup_candidate_address(&foundation)
        .unwrap_or_else(|| "? (peer-reflexive, no SDP)".to_owned());
    format!(
        "{} {} ({} prio={} f={})",
        candidate_type_name(cand_type),
        address,
        transport_name(transport),
        priority,
        foundation
    )
}
