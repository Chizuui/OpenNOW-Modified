use super::*;
use str0m::channel::Reliability;

use super::budget_tests::connect_bundle_pair;
use crate::nvst_budget::{NORMAL_BUDGET, TRANSPORT_AGGREGATE_CAPACITY};

fn receive_messages(local: &mut Rtc, remote: &mut Rtc, now: Instant) -> Vec<(String, Vec<u8>)> {
    let mut messages = Vec::new();
    for _ in 0..4 {
        for collect in [false, true] {
            let (source, destination) = if collect {
                (&mut *remote, &mut *local)
            } else {
                (&mut *local, &mut *remote)
            };
            loop {
                match source.poll_output().unwrap() {
                    Output::Timeout(_) => break,
                    Output::Transmit(packet) => destination
                        .handle_input(Input::Receive(
                            now,
                            Receive {
                                proto: packet.proto,
                                source: packet.source,
                                destination: packet.destination,
                                contents: packet.contents.as_ref().try_into().unwrap(),
                            },
                        ))
                        .unwrap(),
                    Output::Event(Event::ChannelData(data)) if collect => {
                        assert!(data.binary);
                        let channel = source.channel(data.id).unwrap();
                        messages.push((channel.config().unwrap().label.clone(), data.data));
                    }
                    _ => {}
                }
            }
        }
    }
    messages
}

#[test]
fn mjolnir_nack_uses_partial_control_and_preserves_wrapping_rtp_sequences() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let partial = local.channel(channels.control_partial).unwrap();
    let config = partial.config().unwrap();
    assert!(!config.ordered);
    assert_eq!(
        config.reliability,
        Reliability::MaxPacketLifetime { lifetime: 300 }
    );
    local.direct_api().close_data_channel(channels.rtcp);
    receive_messages(&mut local, &mut remote, now);
    assert!(local.channel(channels.rtcp).is_none());

    let feedback = NvstFeedbackState::default();
    feedback.request_nack(65534, 65537, now);
    send_pending_nack(&feedback, now, &mut local, channels, true, 1, 0x12345678);
    let messages = receive_messages(&mut local, &mut remote, now);
    assert_eq!(
        messages,
        vec![(
            "control_channel_partially_reliable".to_owned(),
            vec![0x17, 3, 13, 0, 2, 0, 1, 0xfe, 0xff, 7, 0, 0, 0, 0, 0, 0, 0],
        )]
    );
    assert_eq!(feedback.take_nack(now), None);
    assert!(feedback.resolve_nack(65536));
    assert!(!feedback.keyframe_request_pending());
}

#[test]
fn non_mjolnir_nack_remains_rtcp_generic_on_the_rtcp_channel() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    local
        .direct_api()
        .close_data_channel(channels.control_partial);
    receive_messages(&mut local, &mut remote, now);
    assert!(local.channel(channels.control_partial).is_none());
    let feedback = NvstFeedbackState::default();
    feedback.request_nack(65534, 65537, now);
    send_pending_nack(&feedback, now, &mut local, channels, false, 1, 2);
    assert_eq!(
        receive_messages(&mut local, &mut remote, now),
        vec![(
            "rtcp_on_sctp_private".to_owned(),
            vec![0x81, 205, 0, 3, 0, 0, 0, 1, 0, 0, 0, 2, 0xff, 0xfe, 0, 7],
        )]
    );
    assert!(feedback.resolve_nack(65536));
}

#[test]
fn a_closed_nack_channel_does_not_spend_an_attempt_or_fall_back() {
    for mjolnir in [true, false] {
        let (mut local, mut remote, channels, _) = connect_bundle_pair();
        let now = Instant::now() + Duration::from_secs(1);
        receive_messages(&mut local, &mut remote, now);
        local.direct_api().close_data_channel(if mjolnir {
            channels.control_partial
        } else {
            channels.rtcp
        });
        receive_messages(&mut local, &mut remote, now);
        let feedback = NvstFeedbackState::default();
        feedback.request_nack(42, 42, now);
        for _ in 0..MAX_NACK_ATTEMPTS + 1 {
            send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        }
        assert!(receive_messages(&mut local, &mut remote, now).is_empty());
        assert!(!feedback.resolve_nack(42));
    }
}

#[test]
fn rejected_nacks_preserve_the_shared_budget_and_retry_attempts() {
    for mjolnir in [true, false] {
        let (mut local, _remote, channels, _) = connect_bundle_pair();
        let now = Instant::now();
        let buffered = channels.buffered_total(&mut local);
        assert!(channels.send_control(&mut local, &vec![0; NORMAL_BUDGET - buffered]));
        let feedback = NvstFeedbackState::default();
        feedback.request_nack(42, 42, now);
        for _ in 0..MAX_NACK_ATTEMPTS + 1 {
            send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        }
        let buffered = channels.buffered_total(&mut local);
        assert_eq!(buffered, NORMAL_BUDGET);
        assert!(buffered < TRANSPORT_AGGREGATE_CAPACITY);
        assert!(!feedback.resolve_nack(42));
        feedback.request_nack(43, 43, now);
        send_pending_nack(&feedback, now, &mut local, channels, mjolnir, 1, 2);
        assert_eq!(feedback.take_nack(now), Some((43, 43)));
    }
}

#[test]
fn mjolnir_nacks_keep_the_existing_batch_retry_and_expiry_limits() {
    let (mut local, mut remote, channels, _) = connect_bundle_pair();
    let now = Instant::now() + Duration::from_secs(1);
    receive_messages(&mut local, &mut remote, now);
    let feedback = NvstFeedbackState::default();
    feedback.request_nack(u64::MAX - 64, u64::MAX, now);
    for _ in 0..2 {
        send_pending_nack(&feedback, now, &mut local, channels, true, 1, 2);
    }
    let messages = receive_messages(&mut local, &mut remote, now);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].1,
        nack_v2(0, &(65471..65535).collect::<Vec<_>>())
            .unwrap()
            .encoded()
    );
    assert_eq!(messages[1].1, nack_v2(0, &[65535]).unwrap().encoded());
    assert_eq!(feedback.take_nack(now), None);
    for attempt in 1..MAX_NACK_ATTEMPTS {
        let retry_at = now + NACK_RETRY_INTERVAL * u32::from(attempt);
        send_pending_nack(
            &feedback,
            retry_at - Duration::from_nanos(1),
            &mut local,
            channels,
            true,
            1,
            2,
        );
        assert!(receive_messages(&mut local, &mut remote, retry_at).is_empty());
        for _ in 0..2 {
            send_pending_nack(&feedback, retry_at, &mut local, channels, true, 1, 2);
        }
        assert_eq!(receive_messages(&mut local, &mut remote, retry_at).len(), 2);
    }
    send_pending_nack(
        &feedback,
        now + NACK_RETRY_INTERVAL * 3,
        &mut local,
        channels,
        true,
        1,
        2,
    );
    assert!(receive_messages(&mut local, &mut remote, now + NACK_RETRY_INTERVAL * 3).is_empty());
    assert_eq!(feedback.take_nack(now + NACK_TRACKING_TIMEOUT), None);
    assert!(!feedback.resolve_nack(u64::MAX));
    assert!(!feedback.keyframe_request_pending());
}
