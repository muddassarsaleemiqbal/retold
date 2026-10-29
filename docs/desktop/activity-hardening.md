# Live activity and interruption hardening

An open job has **Follow live**: while it is unfinished, the app re-reads only newer events once a
second, keeping at most one pending tick and the existing 128-event visible window; ticks are
skipped while another request is in flight, and following stops by itself at a terminal state,
when another job is opened, or on **Stop following**. Following is observation only: it never
controls, retries or resumes generation.

`tests/queue_hardening.rs` adds two qualifications at the shared queue layer:

- a 20,000-event flood is replayed in pages of at most 128 events, and a fresh read-only observer
  resumes from the previous cursor without changing the running job's status;
- a damaged queue journal makes the native activity snapshot fail with a diagnostic while the
  file's bytes stay unchanged (no repair, no recreation).

- with the queue at its 1,024-job capacity, the native activity snapshot returns one bounded page
  and the exact total within a 500 ms budget.

Fault cases already qualified by shared tests: daemon SIGKILL during a slow provider call reaps
the provider and recovers without redispatch (`retold-cli/tests/studio_execution/control.rs`),
foreground SIGKILL cleanup by the independent supervisor, a slow follower receiving a large
terminal outcome (`queue_follow.rs`), and GUI process crashes recovering retained drafts without
creating jobs (`retold-desktop/tests/recovery`, `tests/scenes`). Still open: per-error recovery
links validated with VoiceOver.
