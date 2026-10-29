//! Saved artifact list, provenance and a portable image/audio preview.
use super::super::{app::App, page::VideoPage, theme::Theme};
use super::{
    Layer,
    widgets::{self, Emphasis, Row, panel},
};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use retold_app::protocol::Payload;
use serde_json::Value;

const LIST_SHARE: u16 = 38;
const COMPARISON_GAP: u16 = 3;
/// Below this width the preview sits under the list instead of beside it.
const STACK_BELOW: u16 = 100;
/// Rows kept under an image for its caption.
const CAPTION_ROWS: u16 = 2;
const REVIEW_KEYS: &str = "a approve · u unlock · n note · r redo · e edit shot · o open · space audio · f filter · L language · v compare";

pub fn layers(app: &App, video: &VideoPage, body: Rect) -> Vec<Layer> {
    let narrow = body.width < STACK_BELOW;
    let shares = [Constraint::Percentage(LIST_SHARE), Constraint::Min(1)];
    let [list, preview] = if narrow {
        Layout::vertical(shares).areas(body)
    } else {
        Layout::horizontal(shares).areas(body)
    };
    let paragraph = Paragraph::new(artifact_lines(app, video, list))
        .wrap(Wrap { trim: false })
        .block(panel(
            &format!(
                "Saved artifacts · {} · language {}",
                video.filter.label(),
                video.language.as_deref().unwrap_or("all")
            ),
            Emphasis::Focused,
            app.theme,
        ));
    vec![
        Layer::text(paragraph, list),
        Layer::text(preview_text(app, preview), preview),
    ]
}

fn artifact_line(asset: &Value, row: Row, theme: Theme) -> Line<'static> {
    let name = asset["name"].as_str().unwrap_or("Artifact");
    let kind = asset["kind"].as_str().unwrap_or("file");
    Line::from(Span::styled(format!(" {name} · {kind}"), row.style(theme)))
}

fn artifact_lines(app: &App, video: &VideoPage, list: Rect) -> Vec<Line<'static>> {
    let assets = app.review_assets();
    if assets.is_empty() {
        let filtered =
            video.language.is_some() || video.filter != super::super::review_filter::Filter::All;
        let hint = if filtered {
            "No artifacts match these filters. Press f or L to change filters."
        } else {
            "No saved artifacts yet. Continue production to create reviewable media."
        };
        return vec![Line::from(hint)];
    }
    let visible = widgets::inner_rows(list);
    let first = super::super::landing::offset(video.selected, visible);
    assets
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, asset)| {
            let row = Row::at(index, video.selected, Emphasis::Focused);
            artifact_line(asset, row, app.theme)
        })
        .collect()
}

/// The decoded image of the selected artifact, when the theme can show colour.
fn media_image(app: &App) -> Option<&Value> {
    let Some(Payload::Result { contents, .. }) = &app.media.preview else {
        return Option::default();
    };
    let colour = app.theme != Theme::NoColor;
    contents.get("image").filter(|_| colour)
}

fn media_lines(app: &App, area: Rect) -> Vec<Line<'static>> {
    let inner = area.inner(Margin::new(2, 1));
    let image_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(CAPTION_ROWS),
    );
    let comparison = comparison_images(app, image_area);
    if !comparison.is_empty() {
        return comparison;
    }
    match (media_image(app), &app.media.preview) {
        (Some(image), _) => super::super::raster::lines(image, image_area),
        (None, Some(payload)) => widgets::payload_lines(payload, app.theme),
        (None, None) => widgets::loading(app.now, "Select an artifact to preview it.", app.theme),
    }
}

fn preview_text(app: &App, area: Rect) -> Paragraph<'static> {
    let lines: Vec<Line<'static>> = std::iter::once(Line::from(REVIEW_KEYS))
        .chain(comparison_lines(app))
        .chain(media_lines(app, area))
        .collect();
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel("Review preview", Emphasis::Quiet, app.theme))
}

fn comparison_lines(app: &App) -> Vec<Line<'static>> {
    let Some(super::super::page::Page::Video(video)) = app.page() else {
        return Vec::new();
    };
    let Some(path) = &video.comparison else {
        return Vec::new();
    };
    let Some(baseline) = comparison_asset(app, (&video.reference, path)) else {
        return vec![Line::from("Comparison artifact no longer available.")];
    };
    let Some(current) = app.review_asset() else {
        return Vec::new();
    };
    comparison_details(baseline, &current)
}

fn comparison_details(baseline: &Value, current: &Value) -> Vec<Line<'static>> {
    let same = baseline["sha256"]
        .as_str()
        .is_some_and(|hash| !hash.is_empty() && current["sha256"].as_str() == Some(hash));
    let difference = if same {
        "identical file content"
    } else {
        "different file content"
    };
    vec![
        Line::from(format!(
            "Compare with {} · {difference}",
            crate::human::text(baseline, "name")
        )),
        Line::from(format!(
            "Bytes: {} → {} · kind: {} → {}",
            baseline["bytes"],
            current["bytes"],
            crate::human::text(baseline, "kind"),
            crate::human::text(current, "kind")
        )),
    ]
}

fn comparison_asset<'a>(app: &'a App, subject: (&str, &str)) -> Option<&'a Value> {
    let (reference, path) = subject;
    let words = super::super::app::review::asset_words(reference);
    app.store.contents(&words)?["items"]
        .as_array()?
        .iter()
        .find(|asset| asset["path"].as_str() == Some(path))
}

fn comparison_images(app: &App, area: Rect) -> Vec<Line<'static>> {
    let Some(current) = media_image(app) else {
        return Vec::new();
    };
    let Some(Payload::Result { contents, .. }) = &app.media.comparison else {
        return Vec::new();
    };
    let Some(baseline) = contents.get("image") else {
        return Vec::new();
    };
    let width = area.width.saturating_sub(COMPARISON_GAP) / 2;
    let half = Rect::new(0, 0, width, area.height.saturating_sub(1));
    let left = super::super::raster::lines(baseline, half);
    let right = super::super::raster::lines(current, half);
    image_rows(&left, &right, width)
}
fn image_rows(left: &[Line<'static>], right: &[Line<'static>], width: u16) -> Vec<Line<'static>> {
    let length = left.len().max(right.len());
    let rows = (0..length).map(|index| image_row((left.get(index), right.get(index)), width));
    comparison_heading(width).into_iter().chain(rows).collect()
}
fn comparison_heading(width: u16) -> Vec<Line<'static>> {
    vec![Line::from(format!(
        "{:<width$} │ Selected",
        "Baseline",
        width = usize::from(width)
    ))]
}

fn image_row(pair: (Option<&Line<'static>>, Option<&Line<'static>>), width: u16) -> Line<'static> {
    let (left, right) = pair;
    let first = left.cloned().unwrap_or_default();
    let pad = usize::from(width).saturating_sub(first.width());
    let gap = Span::raw(format!("{} │ ", " ".repeat(pad)));
    let second = right.cloned().unwrap_or_default();
    Line::from(
        first
            .spans
            .into_iter()
            .chain([gap])
            .chain(second.spans)
            .collect::<Vec<_>>(),
    )
}
