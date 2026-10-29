//! The frame around every screen: header, sidebar, tab row or breadcrumb, footer and notices.
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::app::{App, Focus, Toast, Tone};
use super::super::catalog::Risk;
use super::super::collection::{Collection, DISCOVER};
use super::super::dashboard;
use super::super::landing::{self, Tab};
use super::super::page::{Modal, Page, VIDEO_TABS};
use super::super::section::{self, Section};
use super::super::theme::Theme;
use super::widgets::{self, hints};
use super::{Layer, Regions};

pub fn layers(app: &App, regions: Regions) -> Vec<Layer> {
    vec![
        Layer::text(header(app, regions.header.width), regions.header),
        Layer::text(sidebar(app, regions.sidebar.width), regions.sidebar),
        Layer::text(tab_row(app), regions.tabs),
        Layer::text(footer(app), regions.footer),
    ]
}

fn header(app: &App, width: u16) -> Paragraph<'static> {
    let theme = app.theme;
    let workspace = dashboard::workspace(&app.store).unwrap_or_default();
    let left = vec![
        Span::styled(" ◆ Retold ", theme.chip(theme.accent())),
        Span::styled(format!("  {workspace}"), theme.muted()),
    ];
    let right = header_status(app);
    let used = Line::from(left.clone()).width() + Line::from(right.clone()).width();
    let gap = Span::raw(" ".repeat(usize::from(width).saturating_sub(used)));
    let spans: Vec<Span<'static>> = left.into_iter().chain([gap]).chain(right).collect();
    Paragraph::new(Line::from(spans)).style(theme.surface())
}

fn header_status(app: &App) -> Vec<Span<'static>> {
    let theme = app.theme;
    let health = dashboard::health(&app.store)
        .into_iter()
        .flat_map(|(id, status)| {
            [
                Span::styled("● ", health_style(status, theme)),
                Span::styled(format!("{id}  "), theme.text()),
            ]
        });
    let today = dashboard::today(&app.store).into_iter().flat_map(|today| {
        [
            Span::styled("today ", theme.muted()),
            Span::styled(format!("{today}  "), theme.strong()),
        ]
    });
    let busy = app.store.busy();
    let activity = if busy { widgets::spinner(app.now) } else { " " };
    let spinner = Span::styled(format!("{activity} "), theme.accent());
    health.chain(today).chain([spinner]).collect()
}

fn health_style(status: &str, theme: Theme) -> ratatui::style::Style {
    match status {
        "ok" => theme.success(),
        "limited" => theme.warning(),
        _ => theme.danger(),
    }
}

fn sidebar(app: &App, width: u16) -> Paragraph<'static> {
    let narrow = width < super::SIDEBAR_WIDE;
    let label = if narrow { icon_label } else { full_label };
    let rows = section::ALL
        .into_iter()
        .map(|section| sidebar_line(app, section, &label(section)));
    let lines: Vec<Line<'static>> = std::iter::once(Line::default()).chain(rows).collect();
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(app.theme.border());
    Paragraph::new(lines).block(block)
}

/// The icon alone, for a narrow sidebar.
fn icon_label(section: Section) -> String {
    format!("{} ", section.glyph())
}

/// Icon, title and number key.
fn full_label(section: Section) -> String {
    let digit = section.digit().map_or_else(|| " ".to_owned(), String::from);
    format!("{}  {:<11} {digit} ", section.glyph(), section.title())
}

fn sidebar_line(app: &App, section: Section, label: &str) -> Line<'static> {
    let theme = app.theme;
    let current = section == app.section;
    let style = match (current, app.focus) {
        (true, Focus::Sidebar) => theme.selected(),
        (true, Focus::Main) => theme.title(),
        (false, _) => theme.muted(),
    };
    let marker = if current { "▌" } else { " " };
    Line::from(vec![
        Span::styled(marker, theme.accent()),
        Span::styled(format!(" {label}"), style),
    ])
}

fn tab_row(app: &App) -> Paragraph<'static> {
    let line = app
        .page()
        .map_or_else(|| tab_line(app), |page| breadcrumb(app, page));
    Paragraph::new(line)
}

fn tab_line(app: &App) -> Line<'static> {
    let titles: Vec<&str> = landing::tabs(app.section)
        .iter()
        .map(|tab| tab.title())
        .collect();
    chips(&titles, app.landing().tab, app.theme)
}

/// Titles as a row of chips, the selected one filled.
fn chips(titles: &[&str], selected: usize, theme: Theme) -> Line<'static> {
    let spans = titles.iter().enumerate().flat_map(|(index, title)| {
        let style = if index == selected {
            theme.chip(theme.accent())
        } else {
            theme.muted()
        };
        [Span::styled(format!(" {title} "), style), Span::raw(" ")]
    });
    Line::from(spans.collect::<Vec<_>>())
}

fn breadcrumb(app: &App, page: &Page) -> Line<'static> {
    let theme = app.theme;
    let title = widgets::clean(&page.title());
    let section = app.section.title();
    let repeats = title.starts_with(section);
    let parent = (!repeats).then(|| {
        [
            Span::styled(format!(" {section} "), theme.muted()),
            Span::styled("› ", theme.faint()),
        ]
    });
    let trail = parent
        .into_iter()
        .flatten()
        .chain([Span::styled(title, theme.strong()), Span::raw("   ")]);
    let tabs = match page {
        Page::Video(video) => {
            let titles: Vec<&str> = VIDEO_TABS.iter().map(|tab| tab.title).collect();
            chips(&titles, video.tab, theme).spans
        }
        _ => Vec::new(),
    };
    Line::from(trail.chain(tabs).collect::<Vec<_>>())
}

fn footer(app: &App) -> Paragraph<'static> {
    let theme = app.theme;
    let keys = hints(&key_hints(app), theme);
    let command = Line::from(vec![
        Span::styled(" $ ", theme.faint()),
        Span::styled(widgets::clean(&app.command()), theme.muted()),
    ]);
    Paragraph::new(vec![keys, command]).style(theme.surface())
}

type Hints = Vec<(&'static str, &'static str)>;

fn key_hints(app: &App) -> Hints {
    if let Some(modal) = &app.modal {
        return modal_hints(modal);
    }
    match app.page() {
        Some(page) => page_hints(page),
        None => landing_hints(app),
    }
}

fn page_hints(page: &Page) -> Hints {
    match page {
        Page::Form(_) => vec![
            ("↑↓", "field"),
            ("space", "switch"),
            ("←→", "choose"),
            ("⏎", "next field"),
            ("ctrl-s", "continue · run"),
            ("esc", "cancel"),
        ],
        Page::Reading(_) | Page::Outcome(_) => result_hints(page),
        Page::Video(_) => vec![
            ("⇥", "tab"),
            ("c", "continue"),
            ("e", "export"),
            ("a", "approve"),
            (".", "all actions"),
            ("esc", "back"),
        ],
    }
}

fn result_hints(page: &Page) -> Hints {
    let series = matches!(page, Page::Reading(reading) if reading.series_id().is_some());
    if series {
        return vec![
            ("↑↓", "part"),
            ("⏎", "video"),
            ("p", "plan"),
            ("n", "draft"),
            ("l", "link"),
            ("r", "run part"),
            ("a", "add parts"),
            ("esc", "back"),
        ];
    }
    let provider = matches!(page, Page::Reading(reading) if reading.provider_id().is_some());
    if provider {
        return vec![
            ("c", "configure login"),
            ("t", "review test"),
            ("r", "refresh"),
            ("esc", "back"),
        ];
    }
    vec![
        ("↑↓", "scroll"),
        ("e", "edit and run again"),
        ("r", "refresh"),
        ("esc", "back"),
    ]
}

fn landing_hints(app: &App) -> Hints {
    let filtering = app.landing().filtering;
    match (filtering, app.focus) {
        (true, _) => vec![("type", "filter"), ("⏎", "done"), ("esc", "clear")],
        (false, Focus::Sidebar) => vec![
            ("↑↓", "section"),
            ("⏎", "open"),
            (":", "commands"),
            ("q", "quit"),
        ],
        (false, Focus::Main) => tab_hints(app.tab()),
    }
}

fn tab_hints(tab: Tab) -> Hints {
    let specific: Hints = match tab {
        Tab::Collection(collection) => collection_hints(collection),
        Tab::Actions => vec![("/", "filter")],
        Tab::Dashboard => Vec::new(),
        Tab::Report(report) => report
            .shortcuts()
            .iter()
            .map(|(key, _, label)| (*key, *label))
            .collect(),
        Tab::Spend => vec![
            ("w", "window"),
            ("b", "group"),
            ("l", "limit"),
            ("a", "account"),
        ],
    };
    [("↑↓", "move"), open_hint(tab)]
        .into_iter()
        .chain(specific)
        .chain([
            ("⇥", "tab"),
            (":", "commands"),
            ("?", "help"),
            ("q", "quit"),
        ])
        .collect()
}

fn open_hint(tab: Tab) -> (&'static str, &'static str) {
    let discovery = matches!(tab, Tab::Collection(collection) if collection == &DISCOVER);
    ("⏎", if discovery { "review" } else { "open" })
}

fn collection_hints(collection: &Collection) -> Hints {
    if let Some(hints) = account_hints(collection) {
        return hints;
    }
    if collection == &super::super::collection::JOBS {
        return vec![
            ("p/r/x", "pause/resume/stop"),
            ("J/K", "reorder"),
            ("/", "filter"),
        ];
    }
    if collection == &super::super::collection::MUSIC
        || collection == &super::super::collection::EFFECTS
    {
        return vec![
            ("space", "play/stop"),
            ("n", "import"),
            (".", "tags/licence"),
            ("/", "filter"),
        ];
    }
    video_collection_hints(collection)
}

fn video_collection_hints(collection: &Collection) -> Hints {
    if collection == &super::super::collection::VIDEOS {
        return vec![
            ("space", "select"),
            ("A/D", "archive/trash selection"),
            ("n", "new"),
            ("/", "filter"),
        ];
    }
    ordinary_collection_hints(collection)
}

fn ordinary_collection_hints(collection: &Collection) -> Hints {
    let actions = (collection != &DISCOVER).then_some((".", "actions"));
    let create = collection.create.map(|_| ("n", collection.create_label));
    actions
        .into_iter()
        .chain(create)
        .chain([("/", "filter")])
        .collect()
}

type Pairs = &'static [(&'static str, &'static str)];

const PALETTE_HINTS: Pairs = &[
    ("type", "search"),
    ("↑↓", "choose"),
    ("⏎", "open"),
    ("esc", "close"),
];
const VALUES_HINTS: Pairs = &[
    ("type", "filter"),
    ("↑↓", "select"),
    ("⏎", "choose"),
    ("esc", "back"),
];
const BROWSER_HINTS: Pairs = &[
    ("↑↓", "file"),
    ("⏎", "choose"),
    ("F4", "folder"),
    ("F6", "edit"),
    ("esc", "back"),
];
const SETUP_HINTS: Pairs = &[
    ("↑↓", "step"),
    ("⏎", "configure"),
    ("↓", "skip"),
    ("esc", "later"),
];
const DESTROY_HINTS: Pairs = &[
    ("type", "exact reference"),
    ("⏎", "delete permanently"),
    ("esc", "cancel"),
];
const PLAN_HINTS: Pairs = &[
    ("↑↓", "review"),
    ("⏎", "run reviewed plan"),
    ("esc", "edit"),
];
const MENU_HINTS: Pairs = &[("↑↓", "choose"), ("⏎", "open"), ("esc", "close")];
const HELP_HINTS: Pairs = &[("any key", "close")];
const QUIT_HINTS: Pairs = &[("⏎", "leave when idle"), ("esc", "keep working")];

fn modal_hints(modal: &Modal) -> Hints {
    let pairs = match modal {
        Modal::Route(editor) => return route_hints(editor),
        Modal::Confirm(form) => return confirm_hints(form.action.risk),
        Modal::Palette(_) => PALETTE_HINTS,
        Modal::Values(_) => VALUES_HINTS,
        Modal::Browser(_) => BROWSER_HINTS,
        Modal::Setup(_) => SETUP_HINTS,
        Modal::Destroy { .. } => DESTROY_HINTS,
        Modal::BulkTag(_)
        | Modal::Plan(_)
        | Modal::Preview(_)
        | Modal::Document(_)
        | Modal::Batch(_) => PLAN_HINTS,
        Modal::Menu(_) => MENU_HINTS,
        Modal::Help => HELP_HINTS,
        Modal::Quit => QUIT_HINTS,
    };
    pairs.to_vec()
}

fn route_hints(editor: &super::super::routes::Editor) -> Hints {
    let adding = editor.adding.is_some();
    if adding {
        vec![
            ("type", "selector"),
            ("⇥", "provider"),
            ("⏎", "add"),
            ("esc", "cancel entry"),
        ]
    } else {
        vec![
            ("↑↓", "entry"),
            ("J/K", "reorder"),
            ("+/-", "add/remove"),
            ("⏎", "review"),
            ("esc", "cancel"),
        ]
    }
}

fn confirm_hints(risk: Risk) -> Hints {
    let run = match risk {
        Risk::Spend => ("⏎", "run within the allowance"),
        Risk::Read | Risk::Write | Risk::Destructive => ("⏎", "run"),
    };
    vec![run, ("esc", "cancel")]
}

/// Notices stack in the bottom-right corner above the footer, newest lowest.
pub fn toasts(app: &App, regions: Regions) -> Vec<Layer> {
    let room = usize::from(regions.body.height);
    app.toasts
        .iter()
        .rev()
        .take(room)
        .zip(1_u16..)
        .flat_map(|(toast, row)| toast_layers(app.theme, toast, toast_area(regions, toast, row)))
        .collect()
}

fn toast_text(toast: &Toast) -> String {
    let mark = match toast.tone {
        Tone::Notice => "•",
        Tone::Success => "✓",
        Tone::Failure => "✗",
    };
    format!(" {mark} {} ", widgets::clean(&toast.text))
}

/// `row` counts up from the footer: 1 is just above it.
fn toast_area(regions: Regions, toast: &Toast, row: u16) -> Rect {
    let width = widgets::columns(&toast_text(toast)).min(regions.body.width);
    let x = regions.body.right().saturating_sub(width.saturating_add(1));
    let y = regions.footer.y.saturating_sub(row);
    Rect::new(x, y, width, 1)
}

fn toast_layers(theme: Theme, toast: &Toast, area: Rect) -> [Layer; 2] {
    let role = match toast.tone {
        Tone::Notice => theme.notice(),
        Tone::Success => theme.success(),
        Tone::Failure => theme.danger(),
    };
    let paragraph = Paragraph::new(toast_text(toast)).style(theme.chip(role));
    [Layer::Blank(area), Layer::text(paragraph, area)]
}

fn account_hints(collection: &Collection) -> Option<Hints> {
    match collection.list {
        "account processes --limit 100" => Some(vec![
            ("</>", "previous/next page"),
            ("/", "filter"),
            ("Ctrl-R", "refresh"),
        ]),
        "account attempts --limit 200" => Some(vec![
            ("⏎", "reconcile"),
            ("</>", "previous/next page"),
            ("/", "filter"),
        ]),
        _ => None,
    }
}
