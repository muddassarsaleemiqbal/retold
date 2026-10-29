//! Bounded peak envelopes of saved narration for waveform display; no provider is contacted.
use std::io::Cursor;

use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

use crate::ProviderError;

/// A waveform never needs more columns than a wide window has pixels.
pub const MAX_BUCKETS: usize = 2048;
/// Long uncompressed narration stays readable while memory remains bounded.
pub const MAX_ENVELOPE_BYTES: usize = 192 * 1024 * 1024;

/// Decoded duration and per-bucket absolute peaks in `0.0..=1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub duration: f64,
    pub peaks: Vec<f32>,
}

fn malformed(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::Malformed(format!("invalid audio: {error}"))
}

fn peak(values: &[f32]) -> f32 {
    values
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f32, f32::max)
}

fn open(bytes: &[u8]) -> Result<symphonia::core::probe::ProbeResult, ProviderError> {
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    symphonia::default::get_probe()
        .format(
            &Hint::new(),
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(malformed)
}

/// Mono absolute amplitudes of one decoded packet.
fn frame_peaks(block: symphonia::core::audio::AudioBufferRef<'_>) -> Vec<f32> {
    let spec = *block.spec();
    let channels = spec.channels.count().max(1);
    let mut buffer = SampleBuffer::<f32>::new(block.capacity() as u64, spec);
    buffer.copy_interleaved_ref(block);
    buffer.samples().chunks(channels).map(peak).collect()
}

/// Decode every packet of the default track into mono absolute amplitudes.
fn amplitudes(bytes: &[u8]) -> Result<(Vec<f32>, u32), ProviderError> {
    let mut probe = open(bytes)?;
    let track = probe
        .format
        .default_track()
        .ok_or_else(|| malformed("no audio track"))?;
    let (id, params) = (track.id, track.codec_params.clone());
    let rate = params
        .sample_rate
        .ok_or_else(|| malformed("no sample rate"))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(malformed)?;
    let mut mono = Vec::new();
    let packets = std::iter::from_fn(|| probe.format.next_packet().ok());
    for packet in packets.filter(|packet| packet.track_id() == id) {
        mono.extend(frame_peaks(decoder.decode(&packet).map_err(malformed)?));
    }
    Ok((mono, rate))
}

fn require_usable(bytes: &[u8], buckets: usize) -> Result<(), ProviderError> {
    let columns_supported = (1..=MAX_BUCKETS).contains(&buckets);
    let size_supported = bytes.len() <= MAX_ENVELOPE_BYTES;
    if !(columns_supported && size_supported) {
        return Err(malformed("unsupported size or bucket count"));
    }
    Ok(())
}

/// Reduce `bytes` to at most `buckets` peaks.
///
/// # Errors
/// Rejects oversized, undecodable or empty audio and a zero or excessive bucket count.
pub fn envelope(bytes: &[u8], buckets: usize) -> Result<Envelope, ProviderError> {
    require_usable(bytes, buckets)?;
    let (mono, rate) = amplitudes(bytes)?;
    if mono.is_empty() {
        return Err(malformed("no samples"));
    }
    let width = mono.len().div_ceil(buckets);
    Ok(Envelope {
        duration: retold_core::num::count_f64(mono.len()) / f64::from(rate),
        peaks: mono
            .chunks(width)
            .map(|chunk| peak(chunk).min(1.0))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, seconds: u32) -> Vec<u8> {
        let count = rate * seconds;
        let pcm: Vec<u8> = (0..count)
            .flat_map(|index| {
                let loud = index >= count / 2;
                let level: i16 = if loud { 16_000 } else { 1_600 };
                let sample = if index % 2 == 0 { level } else { -level };
                sample.to_le_bytes()
            })
            .collect();
        crate::media::pcm_wav(&pcm, rate).unwrap_or_default()
    }

    #[test]
    fn a_quiet_then_loud_tone_has_a_rising_envelope_and_its_duration() {
        let envelope = envelope(&tone(8_000, 2), 4).unwrap_or_else(|_| Envelope {
            duration: 0.0,
            peaks: Vec::new(),
        });
        assert_eq!(envelope.peaks.len(), 4);
        assert!((envelope.duration - 2.0).abs() < 0.01);
        assert!(envelope.peaks[0] < 0.1 && envelope.peaks[3] > 0.4);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        assert!(envelope(&tone(8_000, 1), 0).is_err());
        assert!(envelope(&tone(8_000, 1), MAX_BUCKETS + 1).is_err());
        assert!(envelope(b"not audio", 8).is_err());
    }
}
