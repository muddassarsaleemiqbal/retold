# Phase 6A: read-only account attempt inspection

`retold-next account attempts [--state reserved|dispatched|settled|not_dispatched]
[--after ATTEMPT_ID] [--limit 1..200]` reads only the canonical installation account.
Default page size is 50. Results are sorted by attempt ID, with an exclusive after
cursor and `next_after` only when more matching records exist. Repeat filters on
subsequent pages. Separate invocations are current snapshots, not a frozen pagination
transaction; concurrent state changes can change filter membership.

JSON returns `data.account_attempts`: attempt/job/workspace/video IDs, state, quoted
resource vectors and observed receipt values. Missing/null observations are unknown,
not zero. Raw receipt history and provider messages are not exposed by this projection.
Plain output sanitizes labels, provides an empty state and the next cursor. No charge,
hold, permission, receipt or execution state is changed. This is inspection only;
reconciliation commands and complete spend/resource reporting remain pending.

Local proof: account pagination/filter/balance-preservation regression and CLI empty/
invalid-input fixtures passed, along with strict workspace Clippy and Clean Code.
The legacy snapshot harness now normalizes the tested binary's actual version,
avoiding differing commit stamps between separately built test and binary targets;
all 29 legacy CLI tests passed after that fix. Full workspace verification was started
at `/tmp/retold-final-workspace.log` but was not complete at the quota stopping point.
