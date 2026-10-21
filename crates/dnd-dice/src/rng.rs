//! Randomness is injected (ADR-0001): the engine only asks for raw `u64`s.

use crate::error::DiceError;

/// Source of random numbers for dice. `dnd-game-core` adapts its own RNG to this trait.
pub trait DiceRng {
    fn next_u64(&mut self) -> u64;
}

impl<R: DiceRng + ?Sized> DiceRng for &mut R {
    fn next_u64(&mut self) -> u64 {
        (**self).next_u64()
    }
}

/// Rejection sampling gives up after this many draws. With a working RNG the chance of a single
/// rejection is below `sides / 2^64`, so this only trips on a broken source (e.g. a constant one).
const MAX_ATTEMPTS: u32 = 128;

/// Unbiased uniform value in `1..=sides`.
///
/// # Errors
/// [`DiceError::InvalidSides`] for `sides == 0`; [`DiceError::RngExhausted`] if the RNG keeps
/// returning values from the rejected zone.
pub fn uniform<R: DiceRng + ?Sized>(rng: &mut R, sides: u32) -> Result<u32, DiceError> {
    if sides == 0 {
        return Err(DiceError::InvalidSides { sides: 0 });
    }
    let n = u64::from(sides);
    // 2^64 mod n: values below it would make the low residues more likely.
    let threshold = n.wrapping_neg() % n;
    for _ in 0..MAX_ATTEMPTS {
        let x = rng.next_u64();
        if x >= threshold {
            // x % n < n <= u32::MAX, so the cast is lossless and `+ 1` cannot overflow.
            return Ok((x % n) as u32 + 1);
        }
    }
    Err(DiceError::RngExhausted)
}

/// Small deterministic PRNG (SplitMix64) for tests and seeded reproducible rolls.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl DiceRng for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code
    use super::*;

    struct Constant(u64);
    impl DiceRng for Constant {
        fn next_u64(&mut self) -> u64 {
            self.0
        }
    }

    #[test]
    fn uniform_stays_in_range() {
        let mut rng = SplitMix64::new(7);
        for sides in [1, 2, 3, 6, 20, 100, 1_000_000] {
            for _ in 0..200 {
                let v = uniform(&mut rng, sides).unwrap();
                assert!((1..=sides).contains(&v));
            }
        }
    }

    #[test]
    fn rejects_the_biased_zone() {
        // 2^64 mod 6 = 4: draws 0..4 must be rejected.
        assert_eq!(uniform(&mut Constant(3), 6), Err(DiceError::RngExhausted));
        assert_eq!(uniform(&mut Constant(4), 6), Ok(5));
        assert_eq!(uniform(&mut Constant(0), 1), Ok(1));
    }

    #[test]
    fn zero_sides_is_an_error() {
        assert_eq!(
            uniform(&mut SplitMix64::new(1), 0),
            Err(DiceError::InvalidSides { sides: 0 })
        );
    }
}
