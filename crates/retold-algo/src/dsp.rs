//! Deterministic digital signal processing algorithms for broadcast audio mastering.
//!
//! Provides:
//! - Pure-Rust ITU-R BS.1770-4 / EBU R128 integrated loudness (LUFS) measurement.
//! - 4x oversampled true peak estimation (dBFS).
//! - Windowed RMS energy envelope extraction for dynamic background music ducking.
//! - Zero-crossing silence trimming for clean voice clip transitions.

use retold_core::num;
use serde::{Deserialize, Serialize};

/// Point along the dynamic audio energy envelope.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RmsPoint {
    /// Timestamp in seconds from audio onset.
    pub time_seconds: f64,
    /// Linear RMS amplitude [0.0, 1.0].
    pub linear_rms: f64,
    /// Decibels relative to full scale (dBFS).
    pub dbfs: f64,
}

/// Result of trimming lead-in and lead-out silence at sample zero crossings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SilenceTrimResult {
    /// Start sample index in the original audio buffer.
    pub start_sample: usize,
    /// End sample index (exclusive) in the original audio buffer.
    pub end_sample: usize,
    /// Duration of trimmed leading silence in seconds.
    pub lead_in_seconds: f64,
    /// Duration of trimmed trailing silence in seconds.
    pub lead_out_seconds: f64,
    /// Clean audio slice starting and ending precisely at zero crossings.
    pub samples: Vec<f32>,
}

/// Comprehensive audio broadcast loudness report.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoudnessReport {
    /// Integrated loudness in LUFS (LKFS) compliant with ITU-R BS.1770-4.
    pub integrated_lufs: f64,
    /// Estimated True Peak in dBFS using 4x inter-sample oversampling.
    pub true_peak_dbfs: f64,
    /// Maximum short-term window (3-second) loudness in LUFS.
    pub max_short_term_lufs: f64,
}

#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn pre_filter(sample_rate: f64) -> Self {
        if (sample_rate - 48_000.0).abs() < 1.0 {
            Self {
                b0: 1.535_124_859_586_97,
                b1: -2.691_696_189_406_38,
                b2: 1.198_392_810_852_85,
                a1: -1.690_659_293_182_41,
                a2: 0.732_480_774_215_85,
            }
        } else {
            let f0 = 1_681.974_450_955_533;
            let gain = 3.999_843_806_97;
            let v_h = 10.0_f64.powf(gain / 20.0);
            let k = (std::f64::consts::PI * f0 / sample_rate).tan();
            let a0 = 1.0 + (std::f64::consts::SQRT_2 * k) + (k * k);
            Self {
                b0: (v_h + ((2.0 * v_h).sqrt() * k) + (k * k)) / a0,
                b1: (2.0 * ((k * k) - v_h)) / a0,
                b2: (v_h - ((2.0 * v_h).sqrt() * k) + (k * k)) / a0,
                a1: (2.0 * ((k * k) - 1.0)) / a0,
                a2: (1.0 - (std::f64::consts::SQRT_2 * k) + (k * k)) / a0,
            }
        }
    }

    fn rlb_filter(sample_rate: f64) -> Self {
        if (sample_rate - 48_000.0).abs() < 1.0 {
            Self {
                b0: 1.0,
                b1: -2.0,
                b2: 1.0,
                a1: -1.990_047_454_833_98,
                a2: 0.990_072_250_366_21,
            }
        } else {
            let f0 = 38.135_470_876_024_44;
            let k = (std::f64::consts::PI * f0 / sample_rate).tan();
            let a0 = 1.0 + (std::f64::consts::SQRT_2 * k) + (k * k);
            Self {
                b0: 1.0 / a0,
                b1: -2.0 / a0,
                b2: 1.0 / a0,
                a1: (2.0 * ((k * k) - 1.0)) / a0,
                a2: (1.0 - (std::f64::consts::SQRT_2 * k) + (k * k)) / a0,
            }
        }
    }

    fn filter_samples(&self, samples: &[f32]) -> Vec<f64> {
        let mut out = Vec::with_capacity(samples.len());
        let mut w1 = 0.0_f64;
        let mut w2 = 0.0_f64;

        for &s in samples {
            let x = f64::from(s);
            let w0 = x - (self.a1 * w1) - (self.a2 * w2);
            let y = (self.b0 * w0) + (self.b1 * w1) + (self.b2 * w2);
            w2 = w1;
            w1 = w0;
            out.push(y);
        }
        out
    }
}

/// Measure integrated loudness, true peak, and short-term maximum according to ITU-R BS.1770-4.
#[must_use]
pub fn measure_loudness(samples: &[f32], sample_rate: u32) -> LoudnessReport {
    if samples.is_empty() || sample_rate == 0 {
        return LoudnessReport {
            integrated_lufs: -70.0,
            true_peak_dbfs: -100.0,
            max_short_term_lufs: -70.0,
        };
    }

    let sr = f64::from(sample_rate);
    let stage1 = Biquad::pre_filter(sr);
    let stage2 = Biquad::rlb_filter(sr);

    let filtered1 = stage1.filter_samples(samples);
    let filtered_f32: Vec<f32> = filtered1.iter().map(|&v| num::narrow_f32(v)).collect();
    let k_weighted = stage2.filter_samples(&filtered_f32);

    let block_size = num::round_usize(0.400 * sr);
    let step_size = num::round_usize(0.100 * sr);
    let st_block_size = num::round_usize(3.0 * sr);

    let mut block_powers = Vec::new();
    let mut max_short_term = -70.0_f64;

    let mut idx = 0;
    while idx + block_size <= k_weighted.len() {
        let slice = &k_weighted[idx..idx + block_size];
        let sum_sq: f64 = slice.iter().map(|&x| x * x).sum();
        let mean_sq = sum_sq / num::count_f64(block_size);
        block_powers.push(mean_sq);
        idx += step_size;
    }

    // Short-term maximum (3.0 second windows)
    let mut st_idx = 0;
    while st_idx + st_block_size <= k_weighted.len() {
        let slice = &k_weighted[st_idx..st_idx + st_block_size];
        let sum_sq: f64 = slice.iter().map(|&x| x * x).sum();
        let mean_sq = sum_sq / num::count_f64(st_block_size);
        let lufs = -0.691 + (10.0 * (mean_sq + 1e-12).log10());
        if lufs > max_short_term {
            max_short_term = lufs;
        }
        st_idx += step_size;
    }

    // Gating pass 1: Absolute threshold (-70 LKFS)
    let abs_threshold_pwr = 10.0_f64.powf((-70.0 + 0.691) / 10.0);
    let gated_abs: Vec<f64> = block_powers
        .iter()
        .copied()
        .filter(|&p| p > abs_threshold_pwr)
        .collect();

    let mean_abs: f64 = gated_abs.iter().sum::<f64>() / num::count_f64(gated_abs.len()).max(1.0);
    let relative_threshold_pwr = mean_abs * 10.0_f64.powf(-10.0 / 10.0);
    let gated_rel: Vec<f64> = gated_abs
        .iter()
        .copied()
        .filter(|&p| p > relative_threshold_pwr)
        .collect();
    let integrated = if gated_rel.is_empty() {
        -70.0
    } else {
        let mean_rel: f64 = gated_rel.iter().sum::<f64>() / num::count_f64(gated_rel.len());
        -0.691 + (10.0 * (mean_rel + 1e-12).log10())
    };

    let true_peak = estimate_true_peak(samples);

    LoudnessReport {
        integrated_lufs: integrated,
        true_peak_dbfs: true_peak,
        max_short_term_lufs: max_short_term,
    }
}

/// Estimate True Peak in dBFS using 4x inter-sample sinc interpolation.
#[must_use]
pub fn estimate_true_peak(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return -100.0;
    }

    let mut peak = 0.0_f64;
    // 4x Polyphase FIR coefficients approximating a windowed sinc
    // Filter taps for phases [0, 0.25, 0.5, 0.75]
    let phases: [[f64; 4]; 4] = [
        [0.0, 1.0, 0.0, 0.0],
        [-0.0625, 0.8125, 0.3125, -0.0625],
        [-0.125, 0.625, 0.625, -0.125],
        [-0.0625, 0.3125, 0.8125, -0.0625],
    ];

    for i in 1..(samples.len().saturating_sub(2)) {
        let s0 = f64::from(samples[i - 1]);
        let s1 = f64::from(samples[i]);
        let s2 = f64::from(samples[i + 1]);
        let s3 = f64::from(samples[i + 2]);

        for phase in &phases {
            let interp = (s0 * phase[0]) + (s1 * phase[1]) + (s2 * phase[2]) + (s3 * phase[3]);
            let abs_val = interp.abs();
            if abs_val > peak {
                peak = abs_val;
            }
        }
    }

    if peak <= 1e-9 {
        -100.0
    } else {
        20.0 * peak.log10()
    }
}

/// Extract windowed RMS envelope to guide dynamic ducking curves.
#[must_use]
pub fn extract_rms_envelope(
    samples: &[f32],
    sample_rate: u32,
    window_ms: u32,
    hop_ms: u32,
) -> Vec<RmsPoint> {
    if samples.is_empty() || sample_rate == 0 {
        return Vec::new();
    }

    let sr = f64::from(sample_rate);
    let win_size = num::round_usize((f64::from(window_ms) / 1000.0) * sr);
    let hop_size = num::trunc_usize(((f64::from(hop_ms) / 1000.0) * sr).round().max(1.0));

    let mut points = Vec::new();
    let mut start = 0;

    while start < samples.len() {
        let end = (start + win_size).min(samples.len());
        let slice = &samples[start..end];
        let sum_sq: f64 = slice.iter().map(|&s| f64::from(s).powi(2)).sum();
        let rms = (sum_sq / num::count_f64(slice.len())).sqrt();
        let dbfs = if rms <= 1e-9 {
            -100.0
        } else {
            20.0 * rms.log10()
        };

        let time_sec = (num::count_f64(start) + (num::count_f64(slice.len()) / 2.0)) / sr;
        points.push(RmsPoint {
            time_seconds: time_sec,
            linear_rms: rms,
            dbfs,
        });

        start += hop_size;
    }

    points
}

/// Trim lead-in and lead-out silence snapping cleanly to the nearest zero crossings.
#[must_use]
pub fn trim_silence(samples: &[f32], sample_rate: u32, threshold_dbfs: f64) -> SilenceTrimResult {
    if samples.is_empty() || sample_rate == 0 {
        return SilenceTrimResult {
            start_sample: 0,
            end_sample: 0,
            lead_in_seconds: 0.0,
            lead_out_seconds: 0.0,
            samples: Vec::new(),
        };
    }

    let threshold_linear = num::narrow_f32(10.0_f64.powf(threshold_dbfs / 20.0));
    let sr = f64::from(sample_rate);

    // 1. Locate first audible sample
    let first_audible = samples.iter().position(|&s| s.abs() >= threshold_linear);
    let Some(first_idx) = first_audible else {
        return SilenceTrimResult {
            start_sample: 0,
            end_sample: 0,
            lead_in_seconds: num::count_f64(samples.len()) / sr,
            lead_out_seconds: 0.0,
            samples: Vec::new(),
        };
    };

    // Scan backwards from first_idx to nearest zero crossing
    let mut start_zero = first_idx;
    for i in (1..=first_idx).rev() {
        if (samples[i - 1] <= 0.0 && samples[i] >= 0.0)
            || (samples[i - 1] >= 0.0 && samples[i] <= 0.0)
        {
            start_zero = i;
            break;
        }
    }

    // 2. Locate last audible sample
    let last_audible = samples
        .iter()
        .rposition(|&s| s.abs() >= threshold_linear)
        .unwrap_or(first_idx);
    let mut end_zero = (last_audible + 1).min(samples.len());
    for i in last_audible..(samples.len().saturating_sub(1)) {
        if (samples[i] <= 0.0 && samples[i + 1] >= 0.0)
            || (samples[i] >= 0.0 && samples[i + 1] <= 0.0)
        {
            end_zero = i + 1;
            break;
        }
    }

    let lead_in_sec = num::count_f64(start_zero) / sr;
    let total_samples = samples.len();
    let lead_out_samples = total_samples.saturating_sub(end_zero);
    let lead_out_sec = num::count_f64(lead_out_samples) / sr;
    let trimmed = samples[start_zero..end_zero].to_vec();

    SilenceTrimResult {
        start_sample: start_zero,
        end_sample: end_zero,
        lead_in_seconds: lead_in_sec,
        lead_out_seconds: lead_out_sec,
        samples: trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_silence_at_zero_crossings() {
        let mut audio = vec![0.0_f32; 1000]; // 1000 samples silence
        // Sine wave burst with zero crossing start
        for i in 0..500 {
            let phase = f64::from(i) * 2.0 * std::f64::consts::PI / 50.0;
            audio.push(num::narrow_f32(0.5 * phase.sin()));
        }
        audio.extend(vec![0.0_f32; 1000]); // 1000 samples trailing silence

        let trimmed = trim_silence(&audio, 48000, -30.0);
        assert!(trimmed.lead_in_seconds > 0.0);
        assert!(trimmed.lead_out_seconds > 0.0);
        assert_ne!(trimmed.samples.len(), 0);
        // First sample of trimmed must be very close to zero
        assert!(trimmed.samples[0].abs() < 0.1);
    }

    #[test]
    fn extracts_rms_envelope_with_peaks() {
        let mut audio = vec![0.0_f32; 2400]; // 50ms at 48kHz
        audio.extend(std::iter::repeat_n(0.5_f32, 2400)); // 0.5 flat DC burst
        let env = extract_rms_envelope(&audio, 48000, 50, 25);
        assert_ne!(env.len(), 0);
        let peak_db = env.iter().map(|p| p.dbfs).fold(-100.0_f64, f64::max);
        assert!((peak_db - (20.0 * 0.5_f64.log10())).abs() < 1.0);
    }

    #[test]
    fn measures_silence_as_low_loudness() {
        let silence = vec![0.0_f32; 48000];
        let rep = measure_loudness(&silence, 48000);
        assert_eq!(rep.integrated_lufs, -70.0);
        assert!(rep.true_peak_dbfs <= -99.0);
    }

    #[test]
    fn measures_sine_wave_loudness_and_true_peak() {
        let sr = 48000_u32;
        let mut sine = Vec::with_capacity(sr as usize * 2);
        // 1 kHz sine at 0.5 amplitude (-6 dBFS peak) for 2 seconds
        for i in 0..(sr * 2) {
            let t = f64::from(i) / f64::from(sr);
            let val = 0.5 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            sine.push(num::narrow_f32(val));
        }

        let rep = measure_loudness(&sine, sr);
        // A 1 kHz sine at 0.5 amp peak is approx -9 dBFS RMS / LUFS
        assert!(rep.integrated_lufs > -15.0 && rep.integrated_lufs < -5.0);
        assert!((rep.true_peak_dbfs - (-6.0)).abs() < 0.5);
    }
}
