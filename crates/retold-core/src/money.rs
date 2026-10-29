//! Money, in whole micro-dollars so sums never drift.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign};
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// US dollars, stored as millionths of a dollar.
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
pub struct Usd(i64);

impl Usd {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_micros(micros: i64) -> Self {
        Self(micros)
    }

    #[must_use]
    pub const fn micros(self) -> i64 {
        self.0
    }

    /// Rounds to the nearest micro-dollar; `None` for NaN, infinity or absurd values.
    #[must_use]
    pub fn from_dollars(d: f64) -> Option<Self> {
        let micros = (d * 1_000_000.0).round();
        (micros.is_finite() && micros.abs() < 1e15).then_some(Self(crate::num::round_i64(micros)))
    }

    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Add for Usd {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl AddAssign for Usd {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sum for Usd {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

impl fmt::Display for Usd {
    /// `$0.05`; amounts under a cent keep more digits (`$0.0042`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        if abs > 0 && abs < 10_000 {
            let s = format!("{abs:06}");
            write!(f, "{sign}$0.{}", s.trim_end_matches('0'))
        } else {
            let cents = (abs + 5_000) / 10_000;
            write!(f, "{sign}${}.{:02}", cents / 100, cents % 100)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not an amount in dollars (expected something like `1.00` or `$0.50`)")]
pub struct UsdParseError(String);

impl FromStr for Usd {
    type Err = UsdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || UsdParseError(s.to_owned());
        let n: f64 = s
            .trim()
            .trim_start_matches('$')
            .parse()
            .map_err(|_| err())?;
        if n < 0.0 {
            return Err(err());
        }
        Self::from_dollars(n).ok_or_else(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display() {
        assert_eq!(Usd::from_micros(50_000).to_string(), "$0.05");
        assert_eq!(Usd::from_micros(1_234_567).to_string(), "$1.23");
        assert_eq!(Usd::from_micros(4_200).to_string(), "$0.0042");
        assert_eq!(Usd::ZERO.to_string(), "$0.00");
        assert_eq!(Usd::from_micros(999_000).to_string(), "$1.00");
    }

    #[test]
    fn parse() {
        assert_eq!("$1.00".parse::<Usd>().unwrap(), Usd::from_micros(1_000_000));
        assert_eq!("0.5".parse::<Usd>().unwrap(), Usd::from_micros(500_000));
        assert!("-1".parse::<Usd>().is_err());
        assert!("lots".parse::<Usd>().is_err());
    }

    #[test]
    fn sums_exactly() {
        let total: Usd = std::iter::repeat_n(Usd::from_dollars(0.1).unwrap(), 10).sum();
        assert_eq!(total, Usd::from_micros(1_000_000));
    }
}
