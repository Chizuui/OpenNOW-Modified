//! ICE candidate parsing + observability helpers for NAT debugging.
//!
//! Mirrors the official client's transport observability ("Advertising ICE
//! candidate", "Incoming candidate ... typ srflx", selected pair) at the
//! GStreamer/libnice layer. This module is intentionally free of any
//! GStreamer dependency so the parsing logic stays unit-testable with a plain
//! `cargo test` (the `gstreamer` feature is optional).
//!
//! Candidate addresses are registered by foundation in a process-wide registry
//! so the libnice `new-selected-pair-full` signal (which only carries
//! `NiceCandidate` objects) can be enriched with the human-readable address we
//! already parsed from the SDP candidate lines.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One parsed ICE candidate (SDP `candidate:` line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IceCandidateInfo {
    pub foundation: String,
    pub component_id: String,
    pub transport: String,
    pub priority: Option<u64>,
    pub address: String,
    pub cand_type: String,
    pub related_address: Option<String>,
}

impl IceCandidateInfo {
    /// Single-line human readable form for the app log.
    pub(crate) fn describe(&self) -> String {
        let base = format!(
            "{} {} ({} prio={} f={})",
            self.cand_type,
            self.address,
            self.transport,
            self.priority.unwrap_or(0),
            self.foundation
        );
        match &self.related_address {
            Some(related) => format!("{base} via {related}"),
            None => base,
        }
    }
}

/// Parse an SDP candidate line. Accepts both `candidate:...` and the
/// `a=candidate:...` form used by webrtcbin's `on-ice-candidate` signal.
pub(crate) fn parse_ice_candidate(line: &str) -> Option<IceCandidateInfo> {
    let trimmed = line.trim();
    let body = trimmed
        .strip_prefix("a=candidate:")
        .or_else(|| trimmed.strip_prefix("candidate:"))?;

    let tokens: Vec<&str> = body.split_whitespace().collect();
    if tokens.len() < 8 || tokens[6] != "typ" {
        return None;
    }

    let foundation = tokens[0].to_owned();
    let component_id = tokens[1].to_owned();
    let transport = tokens[2].to_owned();
    let priority = tokens[3].parse::<u64>().ok();
    let address = format!("{}:{}", tokens[4], tokens[5]);
    let cand_type = tokens[7].to_owned();

    // Optional trailing attributes; only `raddr <ip> rport <port>` matters for
    // NAT diagnostics (the mapped address behind the NAT).
    let mut related_address = None;
    for (index, token) in tokens.iter().enumerate() {
        if *token == "raddr" {
            if let (Some(ip), Some(rport)) = (tokens.get(index + 1), tokens.get(index + 3)) {
                related_address = Some(format!("{ip}:{rport}"));
            }
        }
    }

    Some(IceCandidateInfo {
        foundation,
        component_id,
        transport,
        priority,
        address,
        cand_type,
        related_address,
    })
}

/// NiceCandidateType enum value -> name (nice/candidate.h).
pub(crate) fn candidate_type_name(value: i32) -> String {
    match value {
        1 => "host".to_owned(),
        2 => "srflx".to_owned(),
        3 => "prflx".to_owned(),
        4 => "relay".to_owned(),
        other => format!("unknown({other})"),
    }
}

/// NiceCandidateTransport enum value -> name (nice/candidate.h).
pub(crate) fn transport_name(value: i32) -> String {
    match value {
        1 => "UDP".to_owned(),
        2 => "TCP".to_owned(),
        other => format!("unknown({other})"),
    }
}

/// NiceComponentState enum value -> name (nice/agent.h).
pub(crate) fn component_state_name(value: u32) -> String {
    match value {
        0 => "disconnected".to_owned(),
        1 => "gathering".to_owned(),
        2 => "connecting".to_owned(),
        3 => "connected".to_owned(),
        4 => "ready".to_owned(),
        5 => "failed".to_owned(),
        other => format!("unknown({other})"),
    }
}

fn candidate_registry() -> &'static Mutex<HashMap<String, String>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Parse and register a candidate by foundation (address kept for enrichment
/// of the libnice selected-pair signal).
pub(crate) fn register_ice_candidate(line: &str) -> Option<IceCandidateInfo> {
    let info = parse_ice_candidate(line)?;
    if let Ok(mut registry) = candidate_registry().lock() {
        registry.insert(info.foundation.clone(), info.address.clone());
    }
    Some(info)
}

/// Look up a candidate address by foundation (from any locally seen SDP
/// candidate line). Returns `None` when the pair was never surfaced via SDP
/// (e.g. peer-reflexive candidates discovered mid-check).
pub(crate) fn lookup_candidate_address(foundation: &str) -> Option<String> {
    candidate_registry().lock().ok()?.get(foundation).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_candidate() {
        let info = parse_ice_candidate(
            "candidate:1 1 UDP 2122260223 192.168.1.5 52134 typ host generation 0",
        )
        .expect("host candidate should parse");

        assert_eq!(info.cand_type, "host");
        assert_eq!(info.address, "192.168.1.5:52134");
        assert_eq!(info.transport, "UDP");
        assert_eq!(info.priority, Some(2_122_260_223));
        assert_eq!(info.foundation, "1");
        assert_eq!(info.component_id, "1");
        assert_eq!(info.related_address, None);
    }

    #[test]
    fn parses_srflx_candidate_with_mapped_address() {
        let info = parse_ice_candidate(
            "a=candidate:2 1 UDP 1686052607 183.78.14.231 19567 typ srflx \
             raddr 192.168.1.5 rport 52134 generation 0 network-cost 999",
        )
        .expect("srflx candidate should parse");

        assert_eq!(info.cand_type, "srflx");
        assert_eq!(info.address, "183.78.14.231:19567");
        assert_eq!(info.related_address, Some("192.168.1.5:52134".to_owned()));
        assert_eq!(info.priority, Some(1_686_052_607));
    }

    #[test]
    fn parses_relay_candidate() {
        let info = parse_ice_candidate(
            "candidate:3 1 UDP 1677729535 10.0.0.1 39959 typ relay raddr 0.0.0.0 rport 0",
        )
        .expect("relay candidate should parse");

        assert_eq!(info.cand_type, "relay");
        assert_eq!(info.address, "10.0.0.1:39959");
        assert_eq!(info.related_address, Some("0.0.0.0:0".to_owned()));
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse_ice_candidate("").is_none());
        assert!(parse_ice_candidate("not a candidate").is_none());
        assert!(parse_ice_candidate("candidate:1 1 UDP 123").is_none());
        assert!(parse_ice_candidate("m=application 9 UDP/DTLS/SCTP webrtc-datachannel").is_none());
    }

    #[test]
    fn registry_round_trips_by_foundation() {
        register_ice_candidate("candidate:7 1 UDP 1234 203.0.113.9 9000 typ srflx");
        assert_eq!(
            lookup_candidate_address("7").as_deref(),
            Some("203.0.113.9:9000")
        );
        assert_eq!(lookup_candidate_address("nope"), None);
    }

    #[test]
    fn maps_libnice_enum_values() {
        assert_eq!(candidate_type_name(1), "host");
        assert_eq!(candidate_type_name(2), "srflx");
        assert_eq!(candidate_type_name(4), "relay");
        assert_eq!(candidate_type_name(99), "unknown(99)");

        assert_eq!(transport_name(1), "UDP");
        assert_eq!(transport_name(2), "TCP");

        assert_eq!(component_state_name(0), "disconnected");
        assert_eq!(component_state_name(3), "connected");
        assert_eq!(component_state_name(4), "ready");
        assert_eq!(component_state_name(5), "failed");
    }

    #[test]
    fn describe_includes_mapped_address_for_srflx() {
        let info = parse_ice_candidate(
            "a=candidate:2 1 UDP 1686052607 183.78.14.231 19567 typ srflx \
             raddr 192.168.1.5 rport 52134",
        )
        .unwrap();
        let description = info.describe();
        assert!(description.contains("srflx 183.78.14.231:19567"));
        assert!(description.contains("via 192.168.1.5:52134"));
    }
}
