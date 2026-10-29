# Provider Architecture & Adapter Decisions

**Scope: historical/current adapter baseline, not the rewrite's execution policy.**
For the planned rebuild, `cost-quality-contract.md` and CLI plan §19/§21 supersede subscription-first
routing, automatic subscription retries, billing-dependent candidate counts, whole-track speech
assumptions and per-file-only accounting below. Do not read this baseline as planned unlimited
subscription authority or guaranteed local quality. Provider-specific claims need re-verification
when qualifying an adapter; no new implementation is claimed here.

This document details the design decisions, API contracts, and integration models implemented for external AI providers in Retold.

---

## 1. Provider Adapter Summary

| Provider / Adapter | Role | Status | Integration Details & Constraints |
| --- | --- | --- | --- |
| **Claude Code CLI** | Text, Vision | Primary Subscription | Non-interactive JSON protocol. Image inputs passed via stream-json image blocks downscaled to 1,568 px JPEG. Schema enforcement through prompt formatting and response validation. |
| **Codex CLI** | Text, Vision | Primary Subscription | Native output-schema support with structured JSON outputs. Direct image block ingestion. |
| **Gemini CLI** | Text | Optional Subscription | JSON envelope parsing with strict schema validation. Empty tool/MCP allowlists and sandboxed execution. |
| **OpenRouter Images** | Image Synthesis | Pay-per-Use Fallback | Fixed-price quotes per endpoint. Pinned provider routing tags. Strips selector suffixes (such as `@2K`) prior to API dispatch. |
| **ElevenLabs TTS** | Speech Synthesis | Pay-per-Use Voice | Character-based quote ceilings. Raw audio streaming and MP3 decode. Supports inline pronunciation and pacing tags. |
| **OpenAI / Gemini TTS** | Speech Synthesis | Pay-per-Use Voice | Token-priced or duration-priced speech with conservative pre-dispatch holds. Gemini raw PCM is formatted as 24 kHz mono WAV. |
| **OpenRouter Whisper** | Speech-to-Text | Pay-per-Use STT | Duration-based quote ceiling. Word-level offset extraction and Unicode-aware alignment. |
| **Local Whisper (`whisper-cli`)** | Speech-to-Text | Local Offline STT | Direct invocation of `whisper-cli` executable. Filters zero-duration BPE token fragments to ensure accurate word timestamp boundaries. Zero API spend. |

---

## 2. Text and Vision Routing

Retold routes tasks by capability tier:
- `text.smart`: Complex narrative planning, hook tournaments, blind draft judging, script generation.
- `text.fast`: Fact verification, quality gate auditing, packaging metadata formatting.
- `vision.fast`: Illustration QA, detecting visible lettering, anatomical defects, safe-zone breaches, and character consistency.

By default, Retold routes text and vision through installed subscription CLIs first, falling back to pay-per-use endpoints only when configured or when subscriptions encounter rate limits.

---

## 3. Image Generation Contract

Image generation requires precise composition:
1. **Aspect Ratios & Dimensions:** Enforces delivery dimensions defined by the guide's `[format]` block.
2. **Reference Imagery:** For character-driven guides, up to three character turnarounds plus a style reference panel are passed as conditioning references. For non-character guides, only the style reference is passed.
3. **Quoting Ceiling:** Image providers must return an upfront, deterministic price quote before dispatch. If a provider cannot supply a bounded quote, Retold rejects the route before spending funds.

---

## 4. Speech Synthesis and Word-Level Alignment

Accurate subtitle synchronization requires tight coupling between speech synthesis and transcription:
1. **Continuous Narration:** Audio is generated as a single continuous take per language to maintain tonal coherence.
2. **Word Timestamps:** Transcription models extract millisecond start and end boundaries for each spoken word.
3. **Text Alignment:** Retold aligns the transcript against the authoritative script using folded Unicode string matching, tolerating minor phonetic variations while guaranteeing script fidelity.
4. **Tempo Normalization:** Small tempo adjustments (0.94–1.06) are applied when narration falls slightly outside target duration bounds, preserving natural pitch.

---

## 5. Ledger & Billing Safety

To protect against unexpected charges:
- Every billable request must receive an explicit quote and reserve funds in `.retold/ledger.json` before network dispatch.
- Ambiguous errors (such as network drops or timeouts during paid requests) hold the reservation indefinitely until verified by the user.
- The `--allow-paid` CLI flag is required for any run that touches pay-per-use providers.
