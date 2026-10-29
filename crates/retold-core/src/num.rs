//! Numeric conversions between counts, measurements and pixel or sample indices.
//!
//! Every lossy `as` cast in the workspace lives here, behind a name that states its rounding and
//! range rule, so call sites read as intent and the casting lints stay on everywhere else.

/// Largest integer an `f64` represents exactly (2^53).
const EXACT_F64_LIMIT: u64 = 1 << 53;

/// A count or index as `f64`. Exact for every value below 2^53 (9 quadrillion); larger values
/// round to the nearest representable `f64`.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "exact below 2^53, documented rounding above"
)]
pub const fn count_f64(count: usize) -> f64 {
    count as f64
}

/// A 64-bit count as `f64`, with the same rule as [`count_f64`].
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "exact below 2^53, documented rounding above"
)]
pub const fn u64_f64(count: u64) -> f64 {
    count as f64
}

/// A count or index as `f32`. Exact below 2^24 (16.7 million), which covers pixel coordinates.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "exact below 2^24, documented rounding above"
)]
pub const fn count_f32(count: usize) -> f32 {
    count as f32
}

/// A signed 64-bit value as `f64`, with the same rule as [`count_f64`].
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "exact below 2^53 in magnitude")]
pub const fn i64_f64(value: i64) -> f64 {
    value as f64
}

/// Whether [`u64_f64`] is exact for this value.
#[must_use]
pub const fn is_exact_in_f64(count: u64) -> bool {
    count <= EXACT_F64_LIMIT
}

/// The nearest `usize`: rounds half away from zero, clamps negatives and NaN to 0 and large
/// values to `usize::MAX`.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn round_usize(value: f64) -> usize {
    value.round() as usize
}

/// The nearest `u32`, with the clamping rule of [`round_usize`].
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn round_u32(value: f64) -> u32 {
    value.round() as u32
}

/// The nearest `u8`, with the clamping rule of [`round_usize`]; for 8-bit pixel channels.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn round_u8(value: f64) -> u8 {
    value.round() as u8
}

/// The nearest `u64`, with the clamping rule of [`round_usize`].
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn round_u64(value: f64) -> u64 {
    value.round() as u64
}

/// The nearest `i64`: rounds half away from zero, saturates at the `i64` range, NaN becomes 0.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn round_i64(value: f64) -> i64 {
    value.round() as i64
}

/// Rust's `as` conversion for floats, named: truncates toward zero, saturates at the target range,
/// NaN becomes 0.
macro_rules! truncating {
    ($($name:ident -> $target:ty),* $(,)?) => {$(
        #[doc = concat!("`value as ", stringify!($target), "`: truncates toward zero, saturates, NaN becomes 0.")]
        #[must_use]
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
        )]
        pub const fn $name(value: f64) -> $target {
            value as $target
        }
    )*};
}

truncating!(trunc_usize -> usize, trunc_u8 -> u8, trunc_u16 -> u16, trunc_u32 -> u32, trunc_u64 -> u64);

/// `value as i64`: truncates toward zero, saturates at the `i64` range, NaN becomes 0.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn trunc_i64(value: f64) -> i64 {
    value as i64
}

/// `value as i16`: truncates toward zero, saturates at the `i16` range, NaN becomes 0; for
/// 16-bit PCM samples.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-int `as` saturates and maps NaN to 0, which is the documented behaviour"
)]
pub const fn trunc_i16(value: f64) -> i16 {
    value as i16
}

/// The nearest `f32`; values beyond the `f32` range become infinite.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "audio and pixel math needs only f32 precision"
)]
pub const fn narrow_f32(value: f64) -> f32 {
    value as f32
}

/// An `i128` intermediate clamped into the `i64` range.
#[must_use]
pub fn clamp_i64(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_saturates_and_maps_nan_to_zero() {
        assert_eq!(round_usize(2.5), 3);
        assert_eq!(round_usize(-4.0), 0);
        assert_eq!(round_usize(f64::NAN), 0);
        assert_eq!(round_u32(1e20), u32::MAX);
        assert_eq!(round_i64(-1e30), i64::MIN);
        assert_eq!(trunc_usize(3.99), 3);
        assert_eq!(trunc_i64(-3.99), -3);
        assert_eq!(clamp_i64(i128::MAX), i64::MAX);
        assert_eq!(clamp_i64(i128::MIN), i64::MIN);
    }

    #[test]
    fn counts_are_exact_below_two_to_the_fifty_three() {
        assert!(is_exact_in_f64(EXACT_F64_LIMIT));
        assert!(!is_exact_in_f64(EXACT_F64_LIMIT + 1));
        assert!((count_f64(1_000_000) - 1e6).abs() < f64::EPSILON);
    }
}
