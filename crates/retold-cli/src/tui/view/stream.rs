//! Live log viewport follows the tail until the user scrolls away.
use super::super::{app::App, page::Reading, stream::Stream};
use super::{
    Layer,
    widgets::{self, Emphasis, panel},
};
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use serde_json::Value;

/// Rows above the events: status and keys.
const HEADING_ROWS: usize = 2;

pub fn layer(app: &App, reading: &Reading, body: Rect) -> Layer {
    let lines = reading
        .stream
        .as_ref()
        .map(|stream| stream_lines(app, reading, (stream, body)))
        .unwrap_or_default();
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(panel("Studio events", Emphasis::Focused, app.theme));
    Layer::text(paragraph, body)
}

fn status_line(app: &App, stream: &Stream) -> Line<'static> {
    let state = if stream.terminal { "Finished" } else { "Live" };
    let following = if stream.follow {
        "following"
    } else {
        "scroll paused"
    };
    let text = format!("{state} · cursor {} · {following}", stream.sequence);
    Line::from(Span::styled(text, app.theme.accent()))
}

fn event_line(event: &Value) -> Line<'static> {
    let sequence = event["sequence"].as_u64().unwrap_or_default();
    let stage = event["stage"].as_str().unwrap_or("work");
    let message = event["message"].as_str().unwrap_or_default();
    Line::from(format!("{sequence:>4}  {stage}  {message}"))
}

/// The status, the keys, then the events that fit: the newest while following.
fn stream_lines(app: &App, reading: &Reading, shown: (&Stream, Rect)) -> Vec<Line<'static>> {
    let (stream, body) = shown;
    let events = stream.visible();
    let visible = widgets::inner_rows(body).saturating_sub(HEADING_ROWS);
    let first = if stream.follow {
        events.len().saturating_sub(visible)
    } else {
        usize::from(reading.scroll)
    };
    let keys = format!(
        "p pause · r resume · x stop · f follow · / filter: {}",
        stream.filter.text()
    );
    let waiting = events.is_empty().then(|| {
        let view = app.store.view(&reading.words);
        super::landing::view_lines(app, view, "Waiting for studio events…")
    });
    [status_line(app, stream), Line::from(keys)]
        .into_iter()
        .chain(
            events
                .iter()
                .skip(first)
                .take(visible)
                .map(|event| event_line(event)),
        )
        .chain(waiting.into_iter().flatten())
        .collect()
}
