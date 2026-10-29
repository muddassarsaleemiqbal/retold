# FilmCraft control protocol (Spike A, 13 October 2026)

Mapped against the installed FilmCraft by starting a private instance
(`--empty --no-recover --control 9899 --data-dir <tmp>`) and calling `command.list`. Nothing
was saved or exported. The full registry is in `filmcraft-commands.json` (675 commands with
parameter signatures). Every effect and its parameters are in `filmcraft-effects.json`.

## Wire format

TCP to `127.0.0.1:<port>`. Messages are JSON lines in both directions:

```json
→ {"id": 1, "method": "engine.execute", "params": {"command": "timeline.place", "params": {…}}}
← {"id": 1, "ok": true, "result": {…}}          or  {"id": 1, "ok": false, "error": "…"}
```

Times are ticks: 254,016,000,000 per second. `timeline.place` also accepts `frame` or
`seconds`.

## Commands Retold uses

| Need | Command | Notes |
| --- | --- | --- |
| Project, sequences | `file.newProject {name}`, `file.newSequence {name,width,height,fps,video,audio}` → `{sequence}`, `sequence.open {item}` | One sequence per language variant, shared media |
| Media | `file.import {paths}` → `{items, errors}`, `file.newAdjustmentLayer {seconds}` | |
| Placing | `timeline.place {item, track:"V1"\|"A1", time, duration?}` → `{clips}` | A clip on an occupied spot fails: text needs its own track or time |
| Keyframes | `effects.toggleAnimation`, `effects.setParam {clip,effect,param,value,time}`, `effects.setInterpolation {…,mediaTime,interpolation}` | Interpolation: linear, bezier, autoBezier, continuousBezier, hold, easeIn, easeOut |
| Effects | `effects.apply {clips, effect}` | Ids used: `motion`, `opacity`, `vignette`, `noise`, `lumetri`, `light_leaks`, `camera_shake` |
| Transitions | `sequence.applyVideoTransition {clip, effect, frames, edge, params?}` | `film_dissolve`, `whip` (direction, blur), `flash` (color, intensity), `cross_zoom` (center, strength) all exist |
| Text | `graphics.newText {text,size,seconds,track,time,position,font,fontStyle}`, `graphics.set {clip,layer,props}`, `graphics.setCharStyle {clip,layer,start,end,style}`, `graphics.list` → layer `quad` (rendered bounds) | Character ranges are per character, so the highlighted word works for every script |
| Native captions | `captions.newTrack {format:"Subtitle",language}`, `captions.add`, `captions.setStyle`, `captions.export {path, format: srt\|vtt}` | Not used for burned-in captions: those need per-word highlight. Kept as an option for sidecars |
| Audio | `clip.audioGain {clips, mode:"set", db}`, volume keyframes via `effects.setParam` on `volume.level` | |
| Mixer | `mixer.setStrip`, `mixer.setValue {strip, lane, value, time}`, `mixer.setKeyframe`, `mixer.addInsert {strip, effect}` | Track-level automation: music ducking can be written as `A2` volume keyframes |
| Loudness | `file.exportMedia {…, loudnessLufs}` | Normalised at export |
| Export | `file.exportMedia {path, format:"h264", width, height, fps, bitrateKbps, bitrateMode, loudnessLufs, sequence?, wait:false}` → `{job}`, then poll `jobs.list` | `sequence` picks the language variant; `burnCaptions` and `captionSidecar` exist too |
| Thumbnail | `playhead.set {seconds}`, `file.exportFrame {path, format:"png"}` | |
| Save | `file.saveAs {path}`, `file.save` | |
| Inspect | `project.inspect` (item tree with durations in ticks), `sequence.inspect` (tracks, clips, effect params) | Used to read media durations and verify the build |
| Fonts | `fonts.list {system}` → `[{family, styles}]`, `graphics.fonts.used` → `missing` flag | `doctor` checks channel fonts against this |
| Transcription | `transcript.generate {items, model, language}`, `transcript.models` | Local Whisper (tiny/base/small); models not downloaded on this Mac yet. A possible free alternative to OpenRouter Whisper |

## Findings

1. **Fonts.** `Inter` is installed with Regular to Bold only, so "Inter Black" (the Python
   setting) silently falls back. `Noto Sans Devanagari` is not installed. Available instead:
   `Kohinoor Devanagari` (to Bold), `Mukta Mahee`, and `Noto Nastaliq Urdu` (Regular, Bold).
   The channel template now uses installed weights, and `doctor` checks fonts.
2. **Devanagari and Nastaliq** lay out: `graphics.list` returns real widths for both, so
   captions can still be fitted by measurement. Retold fits them in Rust first with
   `harfrust` and checks against FilmCraft only when needed.
3. **Text clips on one track cannot overlap.** Captions go one clip after another on their own
   track; the title gets its own track.
4. **`command.list`** makes the protocol self-describing. `doctor` uses it to confirm every
   command Retold sends exists in the installed version.

## Rust font measurement follow-up

`fonts.list` includes FilmCraft's internal Inter faces. They are not exposed as system font
files to `fontdb` on this Mac. The Rust channel template therefore uses **Helvetica Neue Bold**
for Latin captions, with Kohinoor Devanagari Bold and Noto Nastaliq Urdu Bold for the other scripts.
`doctor` now checks these through the actual Rust shaping database. Internal FilmCraft-only
faces cannot be treated as measurable system fonts.

## Live export follow-up, 14 October 2026

`graphics.newText.track` is a zero-based video-track index: index 1 corresponds to V2.
`timeline.place.track` accepts the one-based named track V2. The adapter converts explicitly,
and the contract test checks the mapping. Session readiness polling previously left the request
timeout at 300 ms on fresh connections; private sessions now allow 120 seconds per command,
with cancellation and poisoned-connection protection retained.

FilmCraft reports jobs as `{finished: true, result: {path, bytes, frames, ...}, status: "Done in …"}`.
Successful result metadata is required; a finished error or null result fails. WAV export is used
for audio-only loudness calibration, with the same native mix and limiter as H.264 export.
Requesting −14 yielded −14.6 LUFS on the English fixture. Five measured previews converged to
−14.0 LUFS with a −12.8 native request; this offset is measured per sequence, never hardcoded.
