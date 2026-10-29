//! A resized frame must own its cursor; filter editing follows the actual grapheme position.
use super::*;
use ratatui::crossterm::event::Event;
use ratatui::layout::{Margin, Position, Rect};

const BOUNDARIES: [(u16, u16); 12] = [
    (0, 0),
    (1, 1),
    (5, 4),
    (20, 8),
    (1, NORMAL.1),
    (20, NORMAL.1),
    (40, 12),
    (71, 20),
    (72, 20),
    COMPACT,
    NORMAL,
    WIDE,
];

fn assert_cursor_bounds(mut app: App) {
    for (width, height) in BOUNDARIES {
        app.react(&Event::Resize(width, height));
        let picture = super::super::view::picture(&app, app.area);
        if let Some(position) = picture.cursor {
            assert!(
                app.area.contains(position),
                "cursor {position:?} escaped {width}x{height}"
            );
        }
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(super::super::view::frame(&app)).unwrap();
    }
    let normal = Rect::new(0, 0, NORMAL.0, NORMAL.1);
    assert!(super::super::view::picture(&app, normal).cursor.is_some());
}

fn cursor(app: &App) -> Position {
    let area = Rect::new(0, 0, NORMAL.0, NORMAL.1);
    super::super::view::picture(app, area)
        .cursor
        .expect("input has room for a cursor")
}

fn filtered() -> App {
    let mut app = fresh();
    app.go(section::Section::Providers);
    app.press(key(KeyCode::Char('/')));
    assert!(app.landing().filtering);
    app
}

fn assert_narrow_input_panel(app: &App) {
    const NARROW: u16 = 10;
    let picture = super::super::view::picture(app, Rect::new(0, 0, NARROW, NORMAL.1));
    let panel = input_panel(&picture);
    if let Some(position) = picture.cursor {
        assert!(
            panel.inner(Margin::new(2, 1)).contains(position),
            "cursor {position:?} escaped its input panel {panel:?}"
        );
    }
}

fn input_panel(picture: &super::super::view::Picture) -> Rect {
    for layer in &picture.layers {
        if let super::super::view::Layer::Blank(area) = layer {
            return *area;
        }
    }
    let mut layers = picture.layers.iter().rev();
    layers
        .find_map(|layer| match layer {
            super::super::view::Layer::Text(_, area) => Some(*area),
            _ => None,
        })
        .unwrap()
}

#[test]
fn form_cursor_stays_inside_each_resized_frame() {
    let mut app = fresh();
    app.open_path("providers add", &vec![("id".into(), "界".repeat(100))]);
    assert_narrow_input_panel(&app);
    assert_cursor_bounds(app);
}

#[test]
fn palette_cursor_stays_inside_each_resized_frame() {
    let mut app = fresh();
    app.press(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert!(matches!(app.modal, Some(Modal::Palette(_))));
    assert_narrow_input_panel(&app);
    assert_cursor_bounds(app);
}

#[test]
fn filter_cursor_stays_inside_each_resized_frame() {
    assert_cursor_bounds(filtered());
}

#[test]
fn filter_cursor_tracks_left_home_and_wide_graphemes() {
    let mut app = filtered();
    app.react(&Event::Paste("A界B".into()));
    let end = cursor(&app);
    app.press(key(KeyCode::Left));
    assert_eq!(cursor(&app).x.saturating_add(1), end.x);
    app.press(key(KeyCode::Left));
    assert_eq!(cursor(&app).x.saturating_add(3), end.x);
    app.press(key(KeyCode::Home));
    assert_eq!(cursor(&app).x.saturating_add(4), end.x);
}

#[test]
fn long_filter_keeps_the_edited_suffix_and_cursor_in_the_list() {
    let mut app = filtered();
    app.react(&Event::Paste(format!("{}TAIL", "界".repeat(100))));
    let position = cursor(&app);
    let regions = super::super::view::regions(Rect::new(0, 0, NORMAL.0, NORMAL.1));
    let (list, _) = super::super::view::split(regions.body);
    assert!(
        list.contains(position),
        "cursor escaped the edited list: {position:?}"
    );
    assert!(screen(&app, NORMAL.0, NORMAL.1).contains("TAIL"));
    assert_eq!(app.landing().filter().chars().count(), 104);
}

#[test]
fn quit_confirmation_hides_the_underlying_filter_cursor() {
    let mut app = fresh();
    app.go(section::Section::Providers);
    app.press(key(KeyCode::Char('y')));
    app.press(key(KeyCode::Char('/')));
    app.press(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(app.modal, Some(Modal::Quit)));
    let picture = super::super::view::picture(&app, Rect::new(0, 0, NORMAL.0, NORMAL.1));
    assert!(picture.cursor.is_none());
}
