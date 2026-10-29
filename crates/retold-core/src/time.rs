//! Time units. Each unit is its own type, so `Frames` can never be passed where `Seconds`
//! is expected.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// FilmCraft's timeline resolution: ticks per second. Divisible by every common frame rate
/// (23.976 excepted), so whole frames convert to whole ticks.
pub const TICKS_PER_SECOND: i64 = 254_016_000_000;

/// Frames per second. Always at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "u32", into = "u32")]
pub struct Fps(u32);

impl Fps {
    /// Returns `None` for zero.
    #[must_use]
    pub const fn new(fps: u32) -> Option<Self> {
        if fps == 0 { None } else { Some(Self(fps)) }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Fps {
    type Error = &'static str;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Self::new(v).ok_or("fps must be at least 1")
    }
}

impl From<Fps> for u32 {
    fn from(f: Fps) -> Self {
        f.0
    }
}

/// A duration or position in seconds. Always finite.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "f64", into = "f64")]
pub struct Seconds(f64);

impl Seconds {
    pub const ZERO: Self = Self(0.0);

    /// Returns `None` for NaN or infinity.
    #[must_use]
    pub fn new(s: f64) -> Option<Self> {
        s.is_finite().then_some(Self(s))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The nearest whole frame.
    #[must_use]
    pub fn to_frames(self, fps: Fps) -> Frames {
        Frames(crate::num::round_u64(
            self.0.max(0.0) * f64::from(fps.get()),
        ))
    }

    /// The nearest tick.
    #[must_use]
    pub fn to_ticks(self) -> Ticks {
        Ticks(crate::num::round_i64(
            self.0 * crate::num::i64_f64(TICKS_PER_SECOND),
        ))
    }
}

impl TryFrom<f64> for Seconds {
    type Error = &'static str;
    fn try_from(v: f64) -> Result<Self, Self::Error> {
        Self::new(v).ok_or("seconds must be a finite number")
    }
}

impl From<Seconds> for f64 {
    fn from(s: Seconds) -> Self {
        s.0
    }
}

impl std::ops::Add for Seconds {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Seconds {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} s", self.0)
    }
}

/// A whole number of frames at some frame rate.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Frames(pub u64);

impl Frames {
    /// Exact when `TICKS_PER_SECOND` is divisible by the frame rate.
    #[must_use]
    pub fn to_ticks(self, fps: Fps) -> Ticks {
        let ticks = i128::from(self.0) * i128::from(TICKS_PER_SECOND) / i128::from(fps.get());
        Ticks(crate::num::clamp_i64(ticks))
    }

    #[must_use]
    pub fn to_seconds(self, fps: Fps) -> Seconds {
        Seconds(crate::num::u64_f64(self.0) / f64::from(fps.get()))
    }
}

/// FilmCraft timeline time: 1/254,016,000,000 of a second.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(transparent)]
pub struct Ticks(pub i64);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn common_frame_rates_give_whole_ticks() {
        for fps in [24, 25, 30, 48, 50, 60, 120] {
            assert_eq!(TICKS_PER_SECOND % i64::from(fps), 0, "{fps}");
        }
    }

    #[test]
    fn one_second_of_frames_is_one_second_of_ticks() {
        let fps = Fps::new(30).unwrap();
        assert_eq!(Frames(30).to_ticks(fps), Ticks(TICKS_PER_SECOND));
    }

    #[test]
    fn rejects_bad_values() {
        assert!(Fps::new(0).is_none());
        assert!(Seconds::new(f64::NAN).is_none());
        assert!(serde_json::from_str::<Fps>("0").is_err());
    }

    proptest! {
        #[test]
        fn frames_round_trip_through_seconds(n in 0u64..10_000_000, fps in 1u32..240) {
            let fps = Fps::new(fps).unwrap();
            prop_assert_eq!(Frames(n).to_seconds(fps).to_frames(fps), Frames(n));
        }
    }
}
