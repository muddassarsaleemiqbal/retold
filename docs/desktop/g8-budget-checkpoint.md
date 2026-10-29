# Native budget report checkpoint

Budget reads the canonical installation account through the existing typed spend report.
It includes every registered workspace and makes that scope visible. Opening or refreshing
does not initialize an account, change limits, reconcile attempts or admit work.

The native report supports today, this month and lifetime periods, grouped by provider,
video, series or day. It shows known charged usage, unresolved held usage, unknown
observations, account/scope ceilings and exact available headroom. Earlier-period holds
remain separate and visible. USD formatting uses integer microdollars throughout.
Rendering groups is paginated at 50 rows; the shared report still reads the existing
bounded account state rather than using a new indexed reporting backend.

Refresh failures retain the prior report with its actual period/grouping and an error.
When selected filters differ from that report, the surface explicitly labels it as the
last successful result. Missing ceilings never imply unlimited authority.

The isolated native journey checks missing-account reads, initialization by the fixture,
exact $1.234567 headroom, period/group changes and unchanged accounting database bytes.
The existing shared spend suite covers charges, unknown observations, held usage,
period boundaries and reconciled headroom. A native formatting regression covers one
microdollar and the full unsigned integer range without floating-point conversion.

The authoritative root and standalone desktop full gates both passed for this checkpoint.

Account initialization, reviewed limit editing and reconciliation are not exposed by this
reporting slice. They remain G8 work alongside the other open GUI phase gates.
