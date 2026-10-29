//! Budget reporting keeps charged, held and unknown usage visibly distinct.
use super::{error_notice, heading_small, muted, notice, page_header, refresh_button, segmented};
use crate::shell::Shell;
use gpui_kit::component::{Disableable as _, Selectable as _, button::Button};
use gpui_kit::{Context, Div, div, prelude::*};
use retold_app::account::{
    report::{Ceiling, Grouping, Totals},
    types::{Unit, Window},
};

const ROWS_PER_PAGE: usize = 50;

impl Shell {
    pub(super) fn budget_page(&self, cx: &mut Context<Self>) -> Div {
        let panel = &self.budget;
        let p = self.palette();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(page_header(
                "Budget",
                "Spending across all your workspaces.",
                [refresh_button(
                    "desktop-budget-refresh",
                    if self.busy {
                        "Reading account…"
                    } else {
                        "Refresh report"
                    },
                    self.busy,
                )
                .on_click(cx.listener(|this, _, _, cx| this.load_budget(cx)))
                .into_any_element()],
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_6()
                    .child(segmented(
                        [
                            (Window::Month, "This month"),
                            (Window::Day, "Today"),
                            (Window::Lifetime, "All time"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (period, label))| {
                            Button::new(("desktop-budget-period", index))
                                .self_start()
                                .label(label)
                                .selected(panel.window == period)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.budget.window = period;
                                    this.load_budget(cx);
                                }))
                        }),
                        p,
                    ))
                    .child(segmented(
                        [
                            (Grouping::Provider, "By provider"),
                            (Grouping::Video, "By video"),
                            (Grouping::Series, "By series"),
                            (Grouping::Day, "By day"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (grouping, label))| {
                            Button::new(("desktop-budget-group", index))
                                .self_start()
                                .label(label)
                                .selected(panel.grouping == grouping)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.budget.grouping = grouping;
                                    this.load_budget(cx);
                                }))
                        }),
                        p,
                    )),
            )
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            });
        let Some(report) = &panel.report else {
            return body.child(notice("No account report is available. Initialize the installation account with account init, or restore authoritative accounting if it is unavailable, then refresh.", p));
        };
        body = body.child(heading_small(&format!("{} · {}", window_label(report.window), report.period.as_deref().map_or_else(|| "All dates".to_owned(), period_label))))
            .when(panel.window != report.window || panel.grouping != report.by, |body| body.child(notice("Showing the last successful report. The selected filters have not loaded; retry Refresh report.", p)))
            .child(totals(&report.total, p))
            .child(ceiling(report.ceiling.as_ref(), p));
        if let Some(carried) = report
            .carried
            .as_ref()
            .filter(|carried| carried.unresolved > 0)
        {
            body = body
                .child(heading_small("Still pending from earlier periods"))
                .child(muted(
                    "These count against what is left until they are settled.",
                    p,
                ))
                .child(totals(carried, p));
        }
        body = body.child(heading_small(group_label(report.by)));
        if report.rows.is_empty() {
            return body.child(muted("No spending in this period yet.", p));
        }
        let pages = report.rows.len().div_ceil(ROWS_PER_PAGE);
        let page = panel.page.min(pages - 1);
        body.children(
            report
                .rows
                .iter()
                .skip(page * ROWS_PER_PAGE)
                .take(ROWS_PER_PAGE)
                .map(|row| {
                    div()
                        .p_4()
                        .border_1()
                        .border_color(p.border)
                        .rounded_lg()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(heading_small(
                            row.key.as_deref().unwrap_or("Outside a series"),
                        ))
                        .child(totals(&row.totals, p))
                        .child(ceiling(row.ceiling.as_ref(), p))
                }),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("desktop-budget-previous")
                        .self_start()
                        .label("Previous")
                        .disabled(page == 0 || self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.budget.page = this.budget.page.saturating_sub(1);
                            cx.notify();
                        })),
                )
                .child(muted(&format!("Page {} of {pages}", page + 1), p))
                .child(
                    Button::new("desktop-budget-next")
                        .self_start()
                        .label("Next")
                        .disabled(page + 1 >= pages || self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.budget.page = (this.budget.page + 1).min(pages - 1);
                            cx.notify();
                        })),
                ),
        )
    }
}

fn totals(total: &Totals, p: crate::theme::Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_3()
                .children(total.resources.iter().map(|(unit, balance)| {
                    super::metric(
                        if *unit == Unit::UsdMicros {
                            "Spent"
                        } else {
                            unit_label(*unit)
                        },
                        &amount(*unit, balance.charged),
                        p,
                    )
                    .min_w(gpui_kit::px(200.))
                    .child(muted(
                        &format!("{} pending", amount(*unit, balance.held)),
                        p,
                    ))
                })),
        )
        .when(total.unresolved > 0, |body| {
            body.child(muted(
                &format!("{} charges not yet confirmed", total.unresolved),
                p,
            ))
        })
}

fn ceiling(ceiling: Option<&Ceiling>, p: crate::theme::Palette) -> Div {
    let Some(ceiling) = ceiling else {
        return muted("Limit: none set.", p);
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(ceiling.iter().map(|(unit, headroom)| {
            muted(
                &format!(
                    "{}: {} of {} used · {} left",
                    if *unit == Unit::UsdMicros {
                        "Spending"
                    } else {
                        unit_label(*unit)
                    },
                    amount(*unit, headroom.used),
                    amount(*unit, headroom.limit),
                    amount(*unit, headroom.available)
                ),
                p,
            )
        }))
}

pub(super) fn amount(unit: Unit, value: u64) -> String {
    if unit == Unit::UsdMicros {
        let fraction = format!("{:06}", value % 1_000_000);
        let trimmed = fraction.trim_end_matches('0');
        let shown = if trimmed.len() < 2 {
            &fraction[..2]
        } else {
            trimmed
        };
        return format!("${}.{shown}", value / 1_000_000);
    }
    value.to_string()
}

fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::UsdMicros => "USD",
        Unit::Requests => "Paid requests",
        Unit::InputTokens => "Input tokens",
        Unit::OutputTokens => "Output tokens",
        Unit::AudioMillis => "Audio milliseconds",
        Unit::LocalMillis => "Local processing milliseconds",
        Unit::VisionPixels => "Vision pixels",
    }
}

/// "2026-10" → "October 2026"; other periods are shown as saved.
fn period_label(period: &str) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month = period
        .split_once('-')
        .and_then(|(year, month)| Some((year, month.parse::<usize>().ok()?)))
        .and_then(|(year, month)| Some((year, *MONTHS.get(month.checked_sub(1)?)?)));
    month.map_or_else(
        || period.to_owned(),
        |(year, name)| format!("{name} {year}"),
    )
}

fn window_label(window: Window) -> &'static str {
    match window {
        Window::Month => "This month",
        Window::Day => "Today",
        Window::Lifetime => "All time",
    }
}

fn group_label(grouping: Grouping) -> &'static str {
    match grouping {
        Grouping::Provider => "Provider accounts",
        Grouping::Video => "Videos",
        Grouping::Series => "Series",
        Grouping::Day => "Days",
    }
}

#[cfg(test)]
mod tests {
    use super::{Unit, amount};
    #[test]
    fn money_retains_every_micro_without_floating_point_or_signed_conversion() {
        assert_eq!(amount(Unit::UsdMicros, 1), "$0.000001");
        assert_eq!(amount(Unit::UsdMicros, 0), "$0.00");
        assert_eq!(amount(Unit::UsdMicros, 1_230_000), "$1.23");
        assert_eq!(amount(Unit::UsdMicros, 1_234_500), "$1.2345");
        assert_eq!(amount(Unit::UsdMicros, u64::MAX), "$18446744073709.551615");
    }
}
