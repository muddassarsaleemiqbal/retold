# Retold Studio design

The product direction is a quiet screening room with readable editing surfaces. The user's
story supplies the character. The shell uses restrained slate, clear system typography and
blue focus/selection. See [the product plan](../plan-gui.md) for journeys and architecture;
this file records implementation rules rather than duplicating that plan.

## Color and typography

`experiments/retold-desktop/src/theme.rs` owns the native palette and its projection into
GPUI Kit. Canvas/surface/text/secondary/border/accent/review pairs are:

| Role | Dark | Light |
| --- | --- | --- |
| Canvas | `#1E2935` | `#F4F7FA` |
| Surface | `#263442` | `#FFFFFF` |
| Text | `#EDF3F7` | `#202F3B` |
| Secondary | `#AAB8C6` | `#516273` |
| Border | `#3C4B59` | `#CBD7E1` |
| Focus/action | `#83B8E8` | `#245C91` |
| Review/attention | `#E6C482` | `#795400` |

System appearance follows the window's platform appearance. Explicit light/dark overrides
and a high-contrast option remain user choices. High contrast uses black/white reading surfaces and text, distinct neutral button surfaces,
and a contrasting action/focus color. Status must always have readable words.

Use the platform system sans and its script fallback shaping. Native shell text sizes use
relative units: body 0.875 rem, metadata 0.75 rem, subsection 1.0625 rem and page title
1.75 rem. The base rem is 16 px at 100%; preferences expose 100/125/150/200%. Keep section
editing comfortable and scrollable. Never shorten or normalize narration to make it fit.

## Structure and behavior

- The header retains workspace scope, emergency stopping and appearance. Controls wrap
  when text size or window width requires it. The sidebar and main content scroll separately.
- Prefer one primary task per view. Collection search, applied filters, page navigation,
  selection and execution confirmation are visibly distinct.
- Use 4/8/12/16/24/32 spacing and align related controls. Group only meaningful information;
  avoid decorative metrics or repeated cards that replace a useful work surface.
- Use Kit buttons, fields, dialogs and command list semantics. Keep visible focus. Command
  search uses Kit's virtualized command component and modal focus restoration.
- Put actions next to the saved content or scope they change. Explain invalidation before
  saving. Approval applies to current saved content, and run confirmation remains separate.
- Preserve the last successful observations when reads fail. An error is persistent and
  actionable. Empty, unavailable, partial, blocked and stopped are separate states.
- Retain a script draft through navigation and conflicts. Do not silently adopt another
  writer's revision or apply text to a newly selected video.
- Use language display names in collection metadata; retain canonical edition specifications
  in the backend for narration, caption and delivery identity.

The reference native sizes are 1440×900, 1024×700 and 760×640. Recheck large text at the
compact size. Scroll a specific control into view when checking reachability; scrolling to
the end is not proof that controls earlier in a long form are unreachable.

## Verification and limits

Use actual Metal frames from `tests/native.rs` and `tests/shell_native.rs`, not HTML concept
captures. The gallery inventory records evidence and open qualifications. These developer
checks do not substitute for real VoiceOver, IME composition, audible playback or platform
qualification. No appearance or text preference grants production execution authority.
