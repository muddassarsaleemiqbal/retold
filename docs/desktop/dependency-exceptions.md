# Desktop dependency exceptions

Approved by the user on 2026-10-25: “Prepare narrow desktop-only exceptions.” These apply
only to the isolated GPUI desktop workspace. The repository's root `deny.toml` is unchanged.

The desktop policy keeps the root's all-target advisory checks, license confidence and
allowlist, registry restrictions, and bans. It adds only the exceptions below. They accept
the maintenance risk of the pinned upstream toolkit graph; they do not establish that
these packages are free of vulnerabilities or that the GUI is release-qualified.

| Advisory | Exact package version | Scope of exception |
| --- | --- | --- |
| RUSTSEC-2024-0384 | instant 0.1.13 | Unmaintained package |
| RUSTSEC-2024-0436 | paste 1.0.15 | Unmaintained package |
| RUSTSEC-2025-0134 | rustls-pemfile 2.2.0 | Unmaintained package |
| RUSTSEC-2026-0206 | rustybuzz 0.20.1 | Unmaintained package |
| RUSTSEC-2026-0192 | ttf-parser 0.25.1 | Unmaintained package |
| RUSTSEC-2026-0371 | zbus_names 4.3.5 | Unmaintained package |
| RUSTSEC-2026-0370 | zvariant 5.15.1 | Unmaintained package |
| RUSTSEC-2026-0372 | zvariant_derive 5.15.1 | Unmaintained package |

`0BSD` is allowed only for `enum-iterator =2.3.0` and `enum-iterator-derive =1.5.0`.
It is not added to the general allowlist. GPUI Kit remains pinned to `=0.8.0`.

Run `python3.11 experiments/retold-desktop/audit-policy.py` before the desktop dependency audit.
The guard uses Python 3.11's standard-library TOML parser; it needs no Python packages.
It fails if an exception package version changes, another version of it appears, an ID is
added or removed, a license exception broadens, or the desktop policy drifts from the root.
The root policy remains authoritative for every other rule. New advisories still fail
`cargo deny` unless they are one of these exact approved IDs.

Every toolkit/dependency upgrade requires revisiting these exceptions, seeking maintained
upstream replacements and removing resolved entries. Do not copy this policy into the root,
use it for headless releases, suppress all unmaintained advisories, or filter away platforms
to obtain a passing audit. Production promotion still needs the remaining native input,
media, accessibility, lifecycle, and platform evidence from the GUI plan.
