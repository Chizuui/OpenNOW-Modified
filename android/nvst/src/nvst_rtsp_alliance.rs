//! Partner compatibility selected by Kotlin's CloudMatch provider owner.
//! NVIDIA keeps the existing negotiation; these rules follow dev 5b89338a3.
use super::{NvstRtspError, SessionContext, sdp_attribute};

pub(crate) fn enabled(context: &SessionContext) -> bool {
    context
        .session
        .extra
        .get("alliancePartner")
        .and_then(|v| v.as_bool())
        == Some(true)
}

pub(super) fn runtime_key(sdp: &str) -> Option<(String, u32)> {
    let key = sdp_attribute(sdp, "runtime.encryptionKey")?;
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let id = sdp_attribute(sdp, "runtime.encryptionKeyId")?
        .parse::<i64>()
        .ok()?;
    Some((key.to_ascii_uppercase(), id as u32))
}

pub(super) fn announce(body: String, unified_socket: bool) -> String {
    // Android's older template repeats this attribute with a final hard-coded 0.
    // Last-value readers would send video to a separate socket while the local
    // receiver waits on the unified bundle. Emit the negotiated value once.
    let mut lines: Vec<_> = body
        .lines()
        .filter(|line| !line.starts_with("a=x-nv-general.enableUnifiedSocket:"))
        .map(ToOwned::to_owned)
        .collect();
    let index = lines
        .iter()
        .position(|line| line == "a=x-nv-general.nativeRtcOnBundlePort:1")
        .map_or(lines.len(), |i| i + 1);
    lines.insert(
        index,
        format!(
            "a=x-nv-general.enableUnifiedSocket:{}",
            u8::from(unified_socket)
        ),
    );
    lines.join("\r\n") + "\r\n"
}

pub(crate) fn wait_authenticated_transport(
    mut ready: impl FnMut() -> Result<bool, NvstRtspError>,
    timeout: std::time::Duration,
) -> Result<(), NvstRtspError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if ready()? {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(NvstRtspError::new(
                "nvst-transport-ready-timeout",
                "NVST transport did not authenticate before PLAY; cloud session retained",
            ));
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(10)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_server_key_is_preserved_and_malformed_keys_are_not_used() {
        let key = "ab".repeat(32);
        assert_eq!(
            runtime_key(&format!(
                "a=x-nv-runtime.encryptionKey:{key}\na=x-nv-runtime.encryptionKeyId:-1"
            )),
            Some((key.to_uppercase(), u32::MAX))
        );
        for key in ["ab".repeat(31), "zz".repeat(32)] {
            assert!(
                runtime_key(&format!(
                    "a=x-nv-runtime.encryptionKey:{key}\na=x-nv-runtime.encryptionKeyId:7"
                ))
                .is_none()
            );
        }
        assert!(runtime_key("").is_none());
    }

    #[test]
    fn final_announce_layout_agrees_with_the_receiver() {
        let template = "a=x-nv-general.nativeRtcOnBundlePort:1\r\na=x-nv-general.enableUnifiedSocket:1\r\na=x-nv-general.enableUnifiedSocket:0\r\na=other:kept\r\n";
        for unified in [false, true] {
            let body = announce(template.into(), unified);
            assert_eq!(
                sdp_attribute(&body, "general.enableUnifiedSocket"),
                Some(u8::from(unified).to_string())
            );
            assert_eq!(
                body.matches("a=x-nv-general.enableUnifiedSocket:").count(),
                1
            );
            assert!(body.contains("a=other:kept\r\n"));
        }
    }

    #[test]
    fn play_wait_is_bounded_and_propagates_cancellation_or_receiver_failure() {
        let mut polls = 0;
        wait_authenticated_transport(
            || {
                polls += 1;
                Ok(polls == 2)
            },
            std::time::Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(polls, 2);
        assert_eq!(
            wait_authenticated_transport(|| Ok(false), std::time::Duration::ZERO)
                .unwrap_err()
                .code,
            "nvst-transport-ready-timeout"
        );
        assert_eq!(
            wait_authenticated_transport(
                || Err(NvstRtspError::new("nvst-cancelled", "cancelled")),
                std::time::Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "nvst-cancelled"
        );
    }
}
