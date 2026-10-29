//! Chunk-aware PCM/float WAV validation and real speaker-embedding comparisons.
use crate::Error;
use retold_core::num;
#[derive(Debug, Clone)]
pub struct Wave {
    pub rate: u32,
    pub samples: Vec<f32>,
}
impl Wave {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(Error::Invalid("expected PCM or float WAV".into()));
        }
        let mut pos = 12;
        let mut format = None;
        let mut data = None;
        while pos + 8 <= bytes.len() {
            let size = u32::from_le_bytes(
                bytes[pos + 4..pos + 8]
                    .try_into()
                    .map_err(|e: std::array::TryFromSliceError| Error::Invalid(e.to_string()))?,
            ) as usize;
            let end = pos
                .checked_add(8)
                .and_then(|p| p.checked_add(size))
                .filter(|e| *e <= bytes.len())
                .ok_or_else(|| Error::Invalid("truncated WAV chunk".into()))?;
            if &bytes[pos..pos + 4] == b"fmt " {
                format = Some(&bytes[pos + 8..end]);
            }
            if &bytes[pos..pos + 4] == b"data" {
                data = Some(&bytes[pos + 8..end]);
            }
            pos = end + (size % 2);
        }
        let f = format
            .filter(|f| f.len() >= 16)
            .ok_or_else(|| Error::Invalid("WAV format missing".into()))?;
        let code = u16::from_le_bytes([f[0], f[1]]);
        let channels = usize::from(u16::from_le_bytes([f[2], f[3]]));
        let rate = u32::from_le_bytes(
            f[4..8]
                .try_into()
                .map_err(|e: std::array::TryFromSliceError| Error::Invalid(e.to_string()))?,
        );
        let bits = u16::from_le_bytes([f[14], f[15]]);
        if channels == 0
            || channels > 8
            || !(8_000..=192_000).contains(&rate)
            || !matches!((code, bits), (1, 16) | (3, 32))
        {
            return Err(Error::Invalid(
                "voice WAV needs PCM16 or float32, 8–192 kHz".into(),
            ));
        }
        let d = data.ok_or_else(|| Error::Invalid("WAV data missing".into()))?;
        let width = usize::from(bits / 8);
        if d.is_empty() || d.len() % (channels * width) != 0 {
            return Err(Error::Invalid("invalid WAV sample alignment".into()));
        }
        let mut samples = Vec::new();
        for frame in d.chunks_exact(channels * width) {
            let mut sum = 0.0;
            for x in frame.chunks_exact(width) {
                sum += if bits == 16 {
                    f32::from(i16::from_le_bytes([x[0], x[1]])) / 32768.0
                } else {
                    f32::from_le_bytes([x[0], x[1], x[2], x[3]])
                };
            }
            let s = sum / num::count_f32(channels);
            if !s.is_finite() {
                return Err(Error::Invalid("nonfinite audio sample".into()));
            }
            samples.push(s);
        }
        Ok(Self { rate, samples })
    }
    #[must_use]
    pub fn duration(&self) -> f64 {
        num::count_f64(self.samples.len()) / f64::from(self.rate)
    }
    pub fn reference(&self) -> Result<(), Error> {
        if !(10.0..=30.0).contains(&self.duration()) {
            return Err(Error::Invalid("voice sample must be 10–30 seconds".into()));
        }
        let rms = (self
            .samples
            .iter()
            .map(|s| f64::from(*s).powi(2))
            .sum::<f64>()
            / num::count_f64(self.samples.len()))
        .sqrt();
        let clipped = num::count_f64(self.samples.iter().filter(|s| s.abs() >= 0.999).count())
            / num::count_f64(self.samples.len());
        if rms < 0.001 || clipped > 0.01 {
            return Err(Error::Invalid(
                "voice sample is silent or severely clipped; replace voice-sample.wav".into(),
            ));
        }
        Ok(())
    }
    /// At most 15 s of the sample, cut at its quietest 100 ms after 8 s.
    /// `IndicF5` conditions on this exact audio and its transcript; letting
    /// the model clip a longer sample would desynchronise the two.
    #[must_use]
    pub fn conditioning(&self) -> Self {
        let rate = self.rate as usize;
        if self.samples.len() <= rate * 15 {
            return self.clone();
        }
        let window = rate / 10;
        let end = (8 * rate..=15 * rate - window)
            .step_by(window / 4)
            .min_by(|&a, &b| {
                let energy =
                    |i: usize| -> f32 { self.samples[i..i + window].iter().map(|s| s * s).sum() };
                energy(a).total_cmp(&energy(b))
            })
            .map_or(15 * rate, |i| i + window / 2);
        Self {
            rate: self.rate,
            samples: self.samples[..end].to_vec(),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let pcm: Vec<u8> = self
            .samples
            .iter()
            .flat_map(|s| num::trunc_i16(f64::from(s.clamp(-1.0, 1.0) * 32767.0)).to_le_bytes())
            .collect();
        Ok(retold_providers::media::pcm_wav(&pcm, self.rate)?)
    }
}
pub fn cosine(a: &[f64], b: &[f64]) -> Result<f64, Error> {
    if a.is_empty() || a.len() != b.len() || a.iter().chain(b).any(|x| !x.is_finite()) {
        return Err(Error::Invalid("invalid speaker embedding".into()));
    }
    let norm = (a.iter().map(|x| x * x).sum::<f64>() * b.iter().map(|x| x * x).sum::<f64>()).sqrt();
    if norm <= f64::EPSILON {
        return Err(Error::Invalid("zero speaker embedding".into()));
    }
    Ok((a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>() / norm).clamp(-1.0, 1.0))
}
// Devanagari permits homorganic nasal conjuncts or anusvara for the same
// spoken cluster (शान्त/शांत). Normalize these exact orthographic variants,
// retaining different consonants and any added or missing words.
fn fold_speech(token: &str) -> String {
    let mut value = token.to_owned();
    for (conjunct, anusvara) in [
        ("ङ्क", "ंक"),
        ("ङ्ख", "ंख"),
        ("ङ्ग", "ंग"),
        ("ङ्घ", "ंघ"),
        ("ञ्च", "ंच"),
        ("ञ्छ", "ंछ"),
        ("ञ्ज", "ंज"),
        ("ञ्झ", "ंझ"),
        ("ण्ट", "ंट"),
        ("ण्ठ", "ंठ"),
        ("ण्ड", "ंड"),
        ("ण्ढ", "ंढ"),
        ("न्त", "ंत"),
        ("न्थ", "ंथ"),
        ("न्द", "ंद"),
        ("न्ध", "ंध"),
        ("म्प", "ंप"),
        ("म्फ", "ंफ"),
        ("म्ब", "ंब"),
        ("म्भ", "ंभ"),
    ] {
        value = value.replace(conjunct, anusvara);
    }
    // Hindi/Urdu speech does not reliably mark vowel length or ai/au quality,
    // and Whisper spells these pairs inconsistently even for deterministic
    // espeak speech (दीवार → दिवार, लौटती → लोटती). Fold only these vowel
    // pairs, nukta and candrabindu. Consonant changes such as रुक → रुख
    // still count as errors.
    let value: String = value
        .chars()
        .filter(|c| *c != '\u{093c}')
        .map(fold_devanagari_vowel)
        .collect();
    retold_lang::align::fold(&value)
}
/// Character edit distance ignoring nasal marks, which fade in connected speech.
const DEVANAGARI_VOWEL_FOLDS: &[(char, char)] = &[
    ('ी', 'ि'),
    ('ू', 'ु'),
    ('ौ', 'ो'),
    ('ै', 'े'),
    ('ई', 'इ'),
    ('ऊ', 'उ'),
    ('औ', 'ओ'),
    ('ऐ', 'ए'),
    ('ँ', 'ं'),
];

fn fold_devanagari_vowel(ch: char) -> char {
    DEVANAGARI_VOWEL_FOLDS
        .iter()
        .find(|(from_ch, ..)| *from_ch == ch)
        .map_or(ch, |(_, to_ch)| *to_ch)
}

fn letters(x: &str, y: &str) -> usize {
    let strip = |w: &str| -> Vec<char> {
        w.chars().filter(|c| !matches!(c, 'ं' | 'ँ' | 'ں')).collect()
    };
    let (x, y) = (strip(x), strip(y));
    let mut row: Vec<usize> = (0..=y.len()).collect();
    for (i, a) in x.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, b) in y.iter().enumerate() {
            next.push(
                (row[j + 1] + 1)
                    .min(next[j] + 1)
                    .min(row[j] + usize::from(a != b)),
            );
        }
        row = next;
    }
    row[y.len()]
}
/// Word edit accuracy with native-script compound spacing normalized. Extra syllables count.
/// In native scripts, a word heard with a single sound changed (an aspiration,
/// a nasal ending, an Urdu /x/ written क) costs half an error: Whisper blurs
/// these even for deterministic espeak speech. Wrong, missing and extra words
/// cost a full error.
#[must_use]
pub fn accuracy(expected: &str, actual: &str) -> f64 {
    let a: Vec<_> = expected
        .split_whitespace()
        .map(fold_speech)
        .filter(|s| !s.is_empty())
        .collect();
    let b: Vec<_> = actual
        .split_whitespace()
        .map(fold_speech)
        .filter(|s| !s.is_empty())
        .collect();
    if a.is_empty() {
        return 0.0;
    }
    let mut rows = vec![(0..=b.len()).map(num::count_f64).collect::<Vec<f64>>()];
    for (i, x) in a.iter().enumerate() {
        let mut row = vec![num::count_f64(i + 1)];
        for (j, y) in b.iter().enumerate() {
            // Whisper can split or join Hindustani compounds without changing their sound.
            // Only exact concatenations in native scripts qualify; added syllables still count.
            let native = x.chars().any(|c| {
                ('\u{0900}'..='\u{097f}').contains(&c) || ('\u{0600}'..='\u{06ff}').contains(&c)
            });
            let substitution = if x == y {
                0.0
            } else if native && x.chars().count() >= 3 && letters(x, y) <= 1 {
                0.5
            } else {
                1.0
            };
            let mut distance = (rows[i][j + 1] + 1.0)
                .min(row[j] + 1.0)
                .min(rows[i][j] + substitution);
            if native && j > 0 && *x == format!("{}{y}", b[j - 1]) {
                distance = distance.min(rows[i][j - 1]);
            }
            if native && i > 0 && format!("{}{x}", a[i - 1]) == *y {
                distance = distance.min(rows[i - 1][j]);
            }
            row.push(distance);
        }
        rows.push(row);
    }
    (1.0 - rows[a.len()][b.len()] / num::count_f64(a.len())).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::Wave;

    #[test]
    fn hindi_vowel_spelling_is_folded_but_consonants_are_not() {
        let expected = "जब हवा लौटती है तो शाखाएँ फिर हिलती हैं और दीवार पर रोशनी बदलती है।";
        let heard = "जब हवा लोटती है तो शाखाएं फिर हिलती हैं और दिवार पर रोशनी बदलती है.";
        assert!((super::accuracy(expected, heard) - 1.0).abs() < 1e-9);
        let wrong = "जब हवा लौटती है तो शाखाएँ फिर हिलती हैं और दीवार पर रोशनी बदलती खिया।";
        assert!(super::accuracy(expected, wrong) < 0.95);
        // One sound changed is half an error; in a four-word line that still fails.
        assert!((super::accuracy("कभी रुक जाती है", "कभी रुख जाती है") - 0.875).abs() < 1e-9);
        assert!(super::accuracy("कभी रुक जाती है", "कभी चल जाती है") <= 0.75);
        let urdu = "اگر سب ایک جیسے بولیں تو سننے والا الجھ سکتا ہے";
        let heard = "اگر سب ایک جیسے بولے تو سننے والا اُلج سکتا ہے";
        assert!((super::accuracy(urdu, heard) - 10.0 / 11.0).abs() < 1e-9);
    }
    #[test]
    fn conditioning_cuts_long_samples_at_the_quietest_point() {
        let rate = 1000;
        let mut samples = vec![0.5_f32; rate * 20];
        samples[11 * rate..11 * rate + 200].fill(0.0);
        let cut = Wave {
            rate: u32::try_from(rate).unwrap(),
            samples,
        }
        .conditioning();
        assert!((11.0..11.2).contains(&cut.duration()), "{}", cut.duration());
        let short = Wave {
            rate: u32::try_from(rate).unwrap(),
            samples: vec![0.5; rate * 12],
        };
        assert_eq!(short.conditioning().samples.len(), rate * 12);
    }
}
