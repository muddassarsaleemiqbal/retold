//! Speech-interval merging for ducking music under narration. Words closer than the
//! hysteresis gap become one interval, so the music bed does not pump between words.

use retold_core::Frames;

/// Merges overlapping and close speech intervals within hysteresis gap threshold.
#[must_use]
pub fn merge_intervals(
    intervals: &[(Frames, Frames)],
    hysteresis_gap: u32,
) -> Vec<(Frames, Frames)> {
    if intervals.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<(u64, u64)> = intervals.iter().map(|(s, e)| (s.0, e.0.max(s.0))).collect();
    sorted.sort_by_key(|(s, _)| *s);

    let mut merged: Vec<(u64, u64)> = Vec::with_capacity(sorted.len());
    let mut current = sorted[0];

    for next in sorted.into_iter().skip(1) {
        let threshold = current.1.saturating_add(u64::from(hysteresis_gap));
        if next.0 <= threshold {
            current.1 = current.1.max(next.1);
        } else {
            merged.push(current);
            current = next;
        }
    }
    merged.push(current);
    merged
        .into_iter()
        .map(|(s, e)| (Frames(s), Frames(e)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_intervals_with_hysteresis() {
        let intervals = vec![
            (Frames(10), Frames(30)),
            (Frames(40), Frames(60)), // gap = 10 <= 24 -> merge to (10, 60)
            (Frames(120), Frames(150)), // gap = 60 > 24 -> distinct
        ];
        let merged = merge_intervals(&intervals, 24);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], (Frames(10), Frames(60)));
        assert_eq!(merged[1], (Frames(120), Frames(150)));
    }
}
