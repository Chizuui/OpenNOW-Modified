//! Per-session AES/GHASH setup, with independent counter and hash state for each packet.
use aes::cipher::{BlockEncrypt, InnerIvInit, StreamCipher};
use aes::{Aes128, Aes256};
use ctr::{Ctr32BE, CtrCore};
use ghash::{GHash, universal_hash::UniversalHash};

enum ExpandedAes {
    Aes128(Aes128),
    Aes256(Aes256),
}

pub(super) struct GcmKey {
    aes: ExpandedAes,
    hash: GHash,
}

// Keep per-packet counters on the stack rather than allocating a Box for every RTP packet.
pub(super) enum GcmCtr {
    Aes128(Ctr32BE<Aes128>),
    Aes256(Ctr32BE<Aes256>),
}

impl GcmCtr {
    pub(super) fn apply_keystream(&mut self, buffer: &mut [u8]) {
        match self {
            Self::Aes128(cipher) => cipher.apply_keystream(buffer),
            Self::Aes256(cipher) => cipher.apply_keystream(buffer),
        }
    }
}

impl GcmKey {
    pub(super) fn new(key: &[u8]) -> Self {
        let mut hash_key = [0_u8; 16];
        let aes = match key.len() {
            16 => {
                let aes =
                    <Aes128 as aes::cipher::KeyInit>::new_from_slice(key).expect("AES128 key");
                aes.encrypt_block((&mut hash_key).into());
                ExpandedAes::Aes128(aes)
            }
            32 => {
                let aes =
                    <Aes256 as aes::cipher::KeyInit>::new_from_slice(key).expect("AES256 key");
                aes.encrypt_block((&mut hash_key).into());
                ExpandedAes::Aes256(aes)
            }
            _ => unreachable!("AES-GCM key is 16 or 32 bytes"),
        };
        Self {
            aes,
            hash: <GHash as ghash::universal_hash::KeyInit>::new((&hash_key).into()),
        }
    }

    pub(super) fn tag_and_ctr(
        &self,
        iv: &[u8; 12],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> ([u8; 16], GcmCtr) {
        let mut j0 = [0_u8; 16];
        j0[..12].copy_from_slice(iv);
        j0[15] = 1;
        // Only immutable key setup is reused. The packet IV, running counter, and GHASH
        // accumulator are fresh so packets cannot contaminate one another's authentication.
        let mut ctr =
            match &self.aes {
                ExpandedAes::Aes128(aes) => GcmCtr::Aes128(Ctr32BE::from_core(
                    CtrCore::inner_iv_init(aes.clone(), (&j0).into()),
                )),
                ExpandedAes::Aes256(aes) => GcmCtr::Aes256(Ctr32BE::from_core(
                    CtrCore::inner_iv_init(aes.clone(), (&j0).into()),
                )),
            };
        let mut tag_mask = [0_u8; 16];
        ctr.apply_keystream(&mut tag_mask);
        let mut hasher = self.hash.clone();
        hasher.update_padded(aad);
        hasher.update_padded(ciphertext);
        let mut lengths = ghash::Block::default();
        lengths[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
        lengths[8..].copy_from_slice(&((ciphertext.len() as u64) * 8).to_be_bytes());
        hasher.update(&[lengths]);
        let mut tag = hasher.finalize();
        for (byte, mask) in tag.iter_mut().zip(tag_mask) {
            *byte ^= mask;
        }
        (tag.into(), ctr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_hash_and_counter_remain_independent_between_packets() {
        let key = GcmKey::new(&[0; 16]);
        // NIST AES128-GCM: zero key/IV, one all-zero plaintext block, no AAD.
        let ciphertext = [
            0x03, 0x88, 0xda, 0xce, 0x60, 0xb6, 0xa3, 0x92, 0xf3, 0x28, 0xc2, 0xb9, 0x71, 0xb2,
            0xfe, 0x78,
        ];
        let expected_tag = [
            0xab, 0x6e, 0x47, 0xd4, 0x2c, 0xec, 0x13, 0xbd, 0xf5, 0x3a, 0x67, 0xb2, 0x12, 0x57,
            0xbd, 0xdf,
        ];
        for _ in 0..10 {
            let (tag, mut ctr) = key.tag_and_ctr(&[0; 12], &[], &ciphertext);
            assert_eq!(tag, expected_tag);
            let mut plaintext = ciphertext;
            ctr.apply_keystream(&mut plaintext);
            assert_eq!(plaintext, [0; 16]);
            // Exercise different hash lengths, AAD and IV between repetitions of the vector.
            let _ = key.tag_and_ctr(&[1; 12], &[2; 13], &[3; 129]);
        }
    }

    #[test]
    #[ignore = "isolated host microbenchmark; not Android stream performance"]
    fn cached_gcm_setup_benchmark() {
        use std::{hint::black_box, time::Instant};
        let raw_key = [7; 32];
        let cached = GcmKey::new(&raw_key);
        let data = [42; 1280];
        let iv = [1; 12];
        for round in 0..3 {
            let start = Instant::now();
            for _ in 0..50_000 {
                black_box(GcmKey::new(black_box(&raw_key)).tag_and_ctr(
                    &iv,
                    &[0; 12],
                    black_box(&data),
                ));
            }
            let fresh = start.elapsed();
            let start = Instant::now();
            for _ in 0..50_000 {
                black_box(cached.tag_and_ctr(&iv, &[0; 12], black_box(&data)));
            }
            let reused = start.elapsed();
            println!(
                "round={round} fresh={fresh:?} cached={reused:?} cached/fresh={:.3}",
                reused.as_secs_f64() / fresh.as_secs_f64()
            );
        }
    }
}
