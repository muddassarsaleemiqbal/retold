//! Text reduction before model calls: near-duplicate paragraph removal (min-hash with LSH
//! banding) and BM25 passage retrieval. Both are deterministic and run locally.

use retold_core::num;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

const SHINGLE: usize = 5;
const HASHES: usize = 64;
const BANDS: usize = 16;
const ROWS: usize = HASHES / BANDS;

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn hash_with<T: Hash + ?Sized>(seed: u64, value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    seed.hash(&mut h);
    value.hash(&mut h);
    h.finish()
}

/// min-hash signature of the paragraph's word 5-shingles; `None` when it is too short.
fn signature(text: &str) -> Option<([u64; HASHES], HashSet<u64>)> {
    let w = words(text);
    if w.len() < SHINGLE * 4 {
        return None;
    }
    let shingles: HashSet<u64> = w.windows(SHINGLE).map(|s| hash_with(0, &s)).collect();
    let mut sig = [u64::MAX; HASHES];
    for s in &shingles {
        for (i, slot) in sig.iter_mut().enumerate() {
            *slot = (*slot).min(hash_with(i as u64 + 1, s));
        }
    }
    Some((sig, shingles))
}

/// For each paragraph (in order), whether it repeats an earlier one: shingle Jaccard
/// similarity of at least `threshold`. Short paragraphs are never marked.
#[must_use]
pub fn near_duplicates(paragraphs: &[&str], threshold: f64) -> Vec<bool> {
    let mut buckets: HashMap<(usize, u64), Vec<usize>> = HashMap::new();
    let mut kept: Vec<Option<HashSet<u64>>> = Vec::with_capacity(paragraphs.len());
    let mut out = Vec::with_capacity(paragraphs.len());
    for (index, text) in paragraphs.iter().enumerate() {
        let Some((sig, shingles)) = signature(text) else {
            kept.push(None);
            out.push(false);
            continue;
        };
        let bands: Vec<_> = (0..BANDS)
            .map(|b| (b, hash_with(0, &sig[b * ROWS..(b + 1) * ROWS])))
            .collect();
        let candidates: BTreeSet<usize> = bands
            .iter()
            .filter_map(|key| buckets.get(key))
            .flatten()
            .copied()
            .collect();
        let duplicate = candidates.into_iter().any(|c| {
            kept[c].as_ref().is_some_and(|other| {
                let shared = shingles.intersection(other).count();
                let left_len = shingles.len();
                let right_len = other.len();
                let union_size = (left_len + right_len).saturating_sub(shared);
                num::count_f64(shared) / num::count_f64(union_size) >= threshold
            })
        });
        out.push(duplicate);
        if duplicate {
            kept.push(None);
        } else {
            for key in bands {
                buckets.entry(key).or_default().push(index);
            }
            kept.push(Some(shingles));
        }
    }
    out
}

/// Ranks `passages` against `query` with Okapi BM25 (k1 = 1.2, b = 0.75); higher is better.
#[must_use]
pub fn bm25(passages: &[&str], query: &str) -> Vec<f64> {
    let docs: Vec<Vec<String>> = passages.iter().map(|p| words(p)).collect();
    let n = num::count_f64(docs.len());
    let avg = num::count_f64(docs.iter().map(Vec::len).sum::<usize>()) / n.max(1.0);
    let mut df: HashMap<&str, f64> = HashMap::new();
    for d in &docs {
        // Each distinct word counts once per passage (document frequency).
        let distinct: HashSet<&str> = d.iter().map(String::as_str).collect();
        for w in distinct {
            *df.entry(w).or_default() += 1.0;
        }
    }
    let terms: HashSet<String> = words(query).into_iter().collect();
    docs.iter()
        .map(|d| {
            let len = num::count_f64(d.len());
            terms
                .iter()
                .map(|t| {
                    let matches = d.iter().filter(|w| *w == t).count();
                    if matches == 0 {
                        return 0.0;
                    }
                    let tf = num::count_f64(matches);
                    let df = df.get(t.as_str()).copied().unwrap_or(0.0);
                    let idf = ((n - df + 0.5) / (df + 0.5)).ln_1p();
                    idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * len / avg.max(1.0)))
                })
                .sum()
        })
        .collect()
}

/// The passages that best support any of `queries`, in their original order, within
/// `budget` characters: each query contributes its top `per_query` passages, best first.
#[must_use]
pub fn retrieve(
    passages: &[&str],
    queries: &[&str],
    per_query: usize,
    budget: usize,
) -> Vec<usize> {
    let mut picks: Vec<(usize, usize)> = Vec::new();
    for query in queries {
        let scores = bm25(passages, query);
        let mut ranked: Vec<_> = (0..passages.len()).filter(|i| scores[*i] > 0.0).collect();
        ranked.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]));
        picks.extend(ranked.into_iter().take(per_query).enumerate());
    }
    // Round-robin by rank, so every query gets its best passage before any gets its second.
    picks.sort_by_key(|(rank, _)| *rank);
    let mut chosen = BTreeSet::new();
    let mut used = 0;
    for (_, i) in picks {
        let size = passages[i].chars().count();
        if chosen.contains(&i) || used + size > budget {
            continue;
        }
        used += size;
        chosen.insert(i);
    }
    chosen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "The judge sat alone in the drawing room of the island house, listening to the storm beat against the shutters while the others slept upstairs and the clock ticked on.";

    #[test]
    fn repeated_paragraphs_are_marked_and_distinct_ones_kept() {
        let edited = A.replace("clock ticked on", "clock ticked on and on");
        let other = "Vera walked down to the beach at dawn, where the sea had left a line of weed and broken shells along the sand, and she thought of the boy in the water long ago.";
        let marks = near_duplicates(&[A, other, &edited, "short"], 0.7);
        assert_eq!(marks, [false, false, true, false]);
    }

    #[test]
    fn bm25_ranks_the_passage_about_the_query_first() {
        let passages = [
            A,
            "Lombard kept a revolver in his drawer.",
            "The weather was mild.",
        ];
        let scores = bm25(&passages, "Where did Lombard keep the revolver?");
        assert!(scores[1] > scores[0] && scores[1] > scores[2]);
        assert_eq!(
            retrieve(&passages, &["revolver drawer", "judge storm"], 1, 10_000),
            [0, 1]
        );
        assert_eq!(
            retrieve(&passages, &["revolver drawer", "judge storm"], 1, 50),
            [1]
        );
    }
}
