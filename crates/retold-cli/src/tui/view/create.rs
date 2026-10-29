//! Draft creation review states the choices without granting production authority.
use super::super::{form::Form, theme::Theme};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};
const WIDTH: u16 = 96;
const HEIGHT: u16 = 20;
pub fn layers(form: &Form, area: Rect, theme: Theme) -> Vec<Layer> {
    let rect = widgets::centered(area, WIDTH, HEIGHT);
    let mut lines = summary(form);
    lines.push(Line::default());
    lines.push(Line::from(widgets::clean(&format!(
        "$ {}",
        form.command_line()
    ))));
    let text = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel("Review new draft", Emphasis::Focused, theme));
    vec![Layer::Blank(rect), Layer::text(text, rect)]
}
fn choice_lines(form: &Form) -> Vec<Line<'static>> {
    CHOICES
        .into_iter()
        .map(|(label, field)| {
            let value = form
                .value(field)
                .unwrap_or_else(|| "command default".into());
            Line::from(widgets::clean(&format!("{label}: {value}")))
        })
        .collect()
}
pub fn summary(form: &Form) -> Vec<Line<'static>> {
    choice_lines(form)
        .into_iter()
        .chain([
            edition_line(form),
            Line::default(),
            Line::from(
                "Creates a saved draft. Production and export require separate reviewed plans.",
            ),
        ])
        .collect()
}

const CHOICES: [(&str, &str); 6] = [
    ("Story", "work"),
    ("Guide", "guide"),
    ("Channel", "channel"),
    ("Quality", "quality"),
    ("Video budget", "budget"),
    ("Source file", "source-file"),
];

fn edition_line(form: &Form) -> Line<'static> {
    let languages = form.list_values("lang");
    let editions = if languages.is_empty() {
        "guide default".into()
    } else {
        languages.join(", ")
    };
    Line::from(widgets::clean(&format!("Editions: {editions}")))
}

/// A tier, the provider chain that serves it and the guide roles that use it.
type TierUse = (String, Vec<String>, Vec<String>);

/// Which provider chain serves each tier the guide's roles use, with unrouted tiers flagged.
pub fn route_lines(contents: &serde_json::Value, theme: Theme) -> Vec<Line<'static>> {
    let Some(roles) = contents["routes"].as_array() else {
        return Vec::new();
    };
    let rows = tier_uses(roles)
        .into_iter()
        .map(|tier| tier_line(tier, theme));
    std::iter::once(Line::styled("Providers used", theme.muted()))
        .chain(rows)
        .collect()
}

fn tier_uses(roles: &[serde_json::Value]) -> Vec<TierUse> {
    roles
        .iter()
        .enumerate()
        .filter(|(index, role)| first_of_tier(roles, role) == Some(*index))
        .map(|(_, role)| {
            let tier = crate::human::text(role, "tier");
            let chain = role["chain"].as_array().into_iter().flatten();
            let chain = chain.filter_map(|entry| entry.as_str().map(str::to_owned));
            let names = roles
                .iter()
                .filter(|other| other["tier"] == role["tier"])
                .map(|other| crate::human::text(other, "role"));
            (tier, chain.collect(), names.collect())
        })
        .collect()
}

fn first_of_tier(roles: &[serde_json::Value], role: &serde_json::Value) -> Option<usize> {
    roles.iter().position(|other| other["tier"] == role["tier"])
}

fn tier_line((tier, chain, names): TierUse, theme: Theme) -> Line<'static> {
    let used = names.join(", ");
    if chain.is_empty() {
        let text = format!("  {tier} → not routed ({used})");
        return Line::styled(widgets::clean(&text), theme.danger());
    }
    let text = format!("  {tier} → {} ({used})", chain.join(" › "));
    Line::from(widgets::clean(&text))
}
