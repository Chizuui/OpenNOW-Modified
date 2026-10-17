//! Bounded retransmission ledger. Each missing packet keeps its own retry budget,
//! even when later packets expand a loss burst or split an existing missing run.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub(super) const INITIAL_WAIT: Duration = Duration::from_millis(1);
pub(super) const RETRY_INTERVAL: Duration = Duration::from_millis(4);
pub(super) const TRACKING_TIMEOUT: Duration = Duration::from_millis(52);
pub(super) const MAX_ATTEMPTS: u8 = 3;
const MAX_PACKETS: usize = 2_048;
pub(super) const MAX_BATCH_PACKETS: usize = 64;

#[derive(Debug)]
struct MissingPacket {
    requested_at: Instant,
    last_sent_at: Option<Instant>,
    attempts: u8,
}

impl MissingPacket {
    fn due(&self, now: Instant) -> bool {
        self.attempts < MAX_ATTEMPTS
            && now.saturating_duration_since(self.requested_at) >= INITIAL_WAIT
            && self
                .last_sent_at
                .is_none_or(|last| now.saturating_duration_since(last) >= RETRY_INTERVAL)
    }
}

#[derive(Debug, Default)]
pub(super) struct NackQueue {
    packets: BTreeMap<u64, MissingPacket>,
}

impl NackQueue {
    pub(super) fn request(&mut self, first: u64, last: u64, now: Instant) {
        if first > last {
            return;
        }
        // A malformed or very large gap must not cause an unbounded insertion
        // loop. The receive window will escalate unrecoverable gaps to IDR.
        let bounded_last = last.min(first.saturating_add(MAX_PACKETS as u64 - 1));
        for index in first..=bounded_last {
            if self.packets.contains_key(&index) {
                continue;
            }
            if self.packets.len() == MAX_PACKETS {
                break;
            }
            self.packets.insert(
                index,
                MissingPacket {
                    requested_at: now,
                    last_sent_at: None,
                    attempts: 0,
                },
            );
        }
    }

    /// Resolving a reordered packet before any successful request isn't a
    /// retransmission recovery. Healthy packets require no allocation here.
    pub(super) fn resolve(&mut self, index: u64) -> bool {
        self.packets
            .remove(&index)
            .is_some_and(|packet| packet.attempts > 0)
    }

    #[cfg(test)]
    pub(super) fn take(&mut self, now: Instant) -> Option<(u64, u64)> {
        self.take_up_to(now, MAX_BATCH_PACKETS)
    }

    pub(super) fn take_up_to(&mut self, now: Instant, packet_limit: usize) -> Option<(u64, u64)> {
        let packet_limit = packet_limit.min(MAX_BATCH_PACKETS);
        if packet_limit == 0 {
            return None;
        }
        self.packets.retain(|_, packet| {
            now.saturating_duration_since(packet.requested_at) < TRACKING_TIMEOUT
        });
        let first = *self
            .packets
            .iter()
            .filter(|(_, packet)| packet.due(now))
            .min_by_key(|(_, packet)| packet.last_sent_at)?
            .0;
        // First attempts outrank retries, then the oldest attempt wins. A large
        // burst must not exhaust the front run before its tail gets a request.
        let mut last = first;
        // Only consecutive, due, still-missing packets may share a range; an
        // already received or differently paced packet must never be re-added.
        while last - first + 1 < packet_limit as u64 {
            let Some(next) = last.checked_add(1) else {
                break;
            };
            if !self
                .packets
                .get(&next)
                .is_some_and(|packet| packet.due(now))
            {
                break;
            }
            last = next;
        }
        for (_, packet) in self.packets.range_mut(first..=last) {
            packet.last_sent_at = Some(now);
            packet.attempts += 1;
        }
        Some((first, last))
    }

    pub(super) fn send_failed(&mut self, first: u64, last: u64) {
        if first > last {
            return;
        }
        for (_, packet) in self.packets.range_mut(first..=last) {
            packet.attempts = packet.attempts.saturating_sub(1);
            // Keep last_sent_at: a refused channel write must not spin or evade
            // the four-millisecond retry spacing and original expiry deadline.
        }
    }

    pub(super) fn clear(&mut self) {
        self.packets.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reordering_guard_resolves_without_request_or_recovery_count() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(7, 7, now);
        assert_eq!(
            queue.take(now + INITIAL_WAIT - Duration::from_nanos(1)),
            None
        );
        assert!(!queue.resolve(7));
        assert_eq!(queue.take(now + INITIAL_WAIT), None);
    }

    #[test]
    fn growing_loss_preserves_existing_retry_budget_and_expiry() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(7, 7, now);
        let sent = now + INITIAL_WAIT;
        for attempt in 0..MAX_ATTEMPTS {
            let at = sent + RETRY_INTERVAL * u32::from(attempt);
            queue.request(7, 8 + u64::from(attempt), at);
            let mut batches = Vec::new();
            while let Some(range) = queue.take(at) {
                batches.push(range);
            }
            assert!(batches.iter().any(|&(first, last)| first <= 7 && last >= 7));
        }
        queue.request(7, 100, now + Duration::from_millis(20));
        let old = queue.packets.get(&7).unwrap();
        assert_eq!(old.requested_at, now);
        assert_eq!(old.attempts, MAX_ATTEMPTS);
        while let Some((first, _)) = queue.take(now + TRACKING_TIMEOUT) {
            assert!(first > 7);
        }
        assert!(!queue.packets.contains_key(&7));
    }

    #[test]
    fn failed_sends_keep_spacing_but_do_not_count_as_recovery() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(7, 8, now);
        let at = now + INITIAL_WAIT;
        assert_eq!(queue.take(at), Some((7, 8)));
        queue.send_failed(7, 8);
        assert_eq!(
            queue.take(at + RETRY_INTERVAL - Duration::from_nanos(1)),
            None
        );
        assert!(!queue.resolve(7));
        assert_eq!(queue.take(at + RETRY_INTERVAL), Some((8, 8)));
        assert!(queue.resolve(8));
    }

    #[test]
    fn disjoint_due_runs_are_batched_without_requesting_received_packets() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(10, 100, now);
        assert!(!queue.resolve(12));
        let at = now + INITIAL_WAIT;
        assert_eq!(queue.take(at), Some((10, 11)));
        assert_eq!(queue.take(at), Some((13, 76)));
        assert_eq!(queue.take(at), Some((77, 100)));
        assert_eq!(queue.take(at), None);
        assert!(queue.resolve(14));
        assert_eq!(queue.take(at + RETRY_INTERVAL), Some((10, 11)));
        assert_eq!(queue.take(at + RETRY_INTERVAL), Some((13, 13)));
        assert_eq!(queue.take(at + RETRY_INTERVAL), Some((15, 78)));
    }

    #[test]
    fn large_loss_burst_gets_first_requests_before_front_run_retries() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(0, 511, now);
        for poll in 0..2 {
            let at = now + INITIAL_WAIT + RETRY_INTERVAL * poll;
            for batch in 0..4 {
                let first = u64::from(poll * 4 + batch) * MAX_BATCH_PACKETS as u64;
                assert_eq!(
                    queue.take(at),
                    Some((first, first + MAX_BATCH_PACKETS as u64 - 1))
                );
            }
        }
        assert_eq!(
            queue.take(now + INITIAL_WAIT + RETRY_INTERVAL),
            Some((0, 63))
        );
    }

    #[test]
    fn disjoint_runs_share_a_total_packet_budget() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(0, 100, now);
        queue.resolve(10);
        let at = now + INITIAL_WAIT;
        assert_eq!(queue.take_up_to(at, 64), Some((0, 9)));
        assert_eq!(queue.take_up_to(at, 54), Some((11, 64)));
        assert_eq!(queue.take_up_to(at, 0), None);
        assert_eq!(queue.take_up_to(at + RETRY_INTERVAL, 64), Some((65, 100)));
    }

    #[test]
    fn ledger_and_each_request_stay_bounded_even_at_sequence_limit() {
        let mut queue = NackQueue::default();
        let now = Instant::now();
        queue.request(0, u64::MAX, now);
        queue.request(3_000, u64::MAX, now);
        assert_eq!(queue.packets.len(), MAX_PACKETS);
        let at = now + INITIAL_WAIT;
        let mut count = 0;
        while let Some((first, last)) = queue.take(at) {
            assert!(last - first < MAX_BATCH_PACKETS as u64);
            count += last - first + 1;
        }
        assert_eq!(count, MAX_PACKETS as u64);
        queue.clear();
        queue.request(u64::MAX - 1, u64::MAX, now);
        assert_eq!(queue.take(at), Some((u64::MAX - 1, u64::MAX)));
    }
}
