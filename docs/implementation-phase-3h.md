# Phase 3H: durable provider cooldown admission

`retold-next providers health --root CONFIG_DIR` reads recorded cooldowns without
sending requests, loading credentials or writing files. JSON returns `data.health`;
plain output sanitizes IDs and shows retry times as Unix seconds. Unknown observations
are `not_checked`; expired observations are `cooldown_elapsed`, never a health/login pass.

New foreground sessions share a durable health admission service between text and media.
Text checks after acquiring concurrency permits on every attempt; media checks after
acquiring its permits and before quotes/reservations or generation. Rate-limit and
unavailable responses persist bounded observations before subsequent work is admitted.
Corrupt, future or inaccessible health state refuses dispatch. Writer contention refuses
the observation update; ambiguous paid holds are not released. A later/longer cooldown
cannot be shortened by concurrent older observations. Cancellation is not a health failure.

The ledger is versioned bounded JSON at `provider-health.json`, with a nonblocking OS
writer lock, re-read/merge under lock and atomic publication. Raw provider messages,
selectors, credentials and profile paths are not stored. Identity hashes provider kind
and account-relevant settings; concurrency, timeout, model aliases, context/token sizing
and binary selection do not clear cooldowns. Identical profile definitions share state.
This does not establish canonical identity across different credential-variable names,
endpoint aliases, equivalent profile paths or installations. Those resource-account identity
requirements remain pending; configuration removal does not delete health history.

Adapters supply retry durations where available; otherwise the conservative fallback is
120 seconds. Parsing human reset-time messages, a full success/failure/login ledger,
notifications and authorized tests remain pending. Legacy execution paths do not opt into
this service yet. Already admitted concurrent calls are not cancelled by a new observation;
this is cooldown admission, not global provider-pool ownership or a retry permission.

Offline regression coverage: recreation/restart admission, alias sharing, monotonic cooldowns,
expiry without writes, corrupt/future files, writer contention, redacted diagnostics and
plain/JSON read-only inspection. No live provider or media export is part of these tests.

Focused local checks pass: five durable health tests, seven paid-gate tests and one
CLI health test; strict workspace Clippy, Clean Code (one existing conditional finding
removed and baseline lowered), formatting, architecture and whitespace checks pass.
The full workspace suite is tracked separately while the implementation loop continues.
No live provider calls, login changes or video exports were made.
