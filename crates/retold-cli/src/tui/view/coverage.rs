//! Which score moods the music library can serve, from the tags on saved tracks.
use super::super::{app::App, collection, theme::Theme};
use ratatui::text::{Line, Span};
use retold_app::audio::SCORE_MOODS;
use serde_json::Value;

pub fn lines(app: &App) -> Vec<Line<'static>> {
    let words = app.collection_words(&collection::MUSIC);
    let Some(items) = app
        .store
        .contents(&words)
        .and_then(|contents| contents["items"].as_array())
    else {
        return Vec::new();
    };
    let covered: Vec<bool> = SCORE_MOODS
        .iter()
        .map(|mood| items.iter().any(|item| tagged(item, mood)))
        .collect();
    let missing = covered.iter().filter(|covered| !**covered).count();
    let summary = if missing == 0 {
        "Every score mood has a track.".to_owned()
    } else {
        format!("{missing} score moods have no tagged track; those scenes play unmatched music.")
    };
    vec![
        Line::default(),
        Line::styled("Mood coverage", app.theme.notice()),
        Line::from(cells(&covered, app.theme)),
        Line::styled(summary, app.theme.muted()),
    ]
}

fn tagged(item: &Value, mood: &str) -> bool {
    item["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|tag| tag.eq_ignore_ascii_case(mood))
}

fn cells(covered: &[bool], theme: Theme) -> Vec<Span<'static>> {
    SCORE_MOODS
        .iter()
        .zip(covered)
        .flat_map(|(mood, covered)| {
            let (glyph, style) = if *covered {
                ("✓", theme.success())
            } else {
                ("✗", theme.danger())
            };
            [
                Span::styled(format!("{glyph} {mood}"), style),
                Span::raw("  "),
            ]
        })
        .collect()
}
