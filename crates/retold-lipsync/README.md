# Illustrated lip-sync

This crate is part of the Retold binary. It provides eight IPA-derived mouth
shapes, seeded blinks, keyed mouth/eye-sheet slicing, and fitted, colour-matched
sprites. `retold-engine::mouths` owns provider requests, cast caching, landmark
measurement, quality checks and shot attachment.

- `viseme::ipa`: local espeak-ng transcription for a word.
- `viseme::timed_plan`: frame-centred sampling of acoustically measured phoneme
  intervals, with a two-frame minimum hold. The engine's managed local model
  measures actual pronunciation on a 20 ms grid. Brief shapes may be suppressed
  by smoothing; model resolution does not certify human-labelled boundary accuracy.
- `viseme::plan`: weighted-duration planning for callers without acoustic timing;
  production engine shots use `timed_plan`.
- `sheet`: the sheet layout contract, magenta keying and validation.
- `fit`: source-to-face similarity transforms with skin-ring colour matching.
- `typography`: native title layout parameters, wired into engine fitting and
  FilmCraft's opt-in vertical CJK text path.

Enable `[visuals.lipsync] enabled = true` in a dialogue guide. espeak-ng is required;
`RETOLD_ESPEAK` overrides its executable. Install acoustic assets with
`retold models install phonemes`. Mouth sheets, acoustic timing and composites rejected by
QA preserve the still artwork; execution errors are reported to the caller.

Run offline checks with `cargo test -p retold-lipsync` and
`cargo test -p retold-engine --test mouths_workflow`.
