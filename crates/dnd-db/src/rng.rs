//! Working randomness of a campaign (ADR-0050). The game core stays I/O-free: it receives a
//! [`GameRng`] from here. Tests inject a seeded generator; the live game draws from the OS.

use dnd_game_core::{GameRng, SplitMix64};

/// The live game's randomness: every number comes from the platform CSPRNG, so the stream is
/// neither derived from public identifiers (a session id, the clock) nor replayed by a restart.
///
/// The constructor draws one number, so a missing entropy source fails at startup rather than
/// mid-turn. If the source fails later, the table must not stop: the stream continues from that
/// startup seed and the failure is logged. This degraded path is unpredictable to outsiders but
/// not cryptographically strong.
#[derive(Debug)]
pub struct OsRng {
    fallback: SplitMix64,
}

impl OsRng {
    /// Fails when the OS entropy source is unavailable at startup.
    pub fn new() -> Result<Self, getrandom::Error> {
        let mut probe = [0u8; 8];
        getrandom::fill(&mut probe)?;
        Ok(Self {
            fallback: SplitMix64::new(u64::from_le_bytes(probe)),
        })
    }
}

impl GameRng for OsRng {
    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        match getrandom::fill(&mut bytes) {
            Ok(()) => u64::from_le_bytes(bytes),
            Err(error) => {
                tracing::warn!(%error, "OS randomness unavailable; continuing from the startup seed");
                self.fallback.next_u64()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn two_generators_do_not_share_a_stream() {
        let mut a = OsRng::new().unwrap();
        let mut b = OsRng::new().unwrap();
        let first: Vec<u64> = (0..4).map(|_| a.next_u64()).collect();
        let second: Vec<u64> = (0..4).map(|_| b.next_u64()).collect();
        assert_ne!(first, second);
        assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
    }
}
