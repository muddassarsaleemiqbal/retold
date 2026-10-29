//! Review actions target the selected saved artifact; previews never generate media.
use super::super::{
    files::Task,
    page::Page,
    runtime::Job,
    store::{Want, words},
};
use super::App;
use ratatui::crossterm::event::KeyCode;
use serde_json::Value;
use std::path::PathBuf;

pub const REVIEW_TAB: usize = 2;

pub fn asset_words(reference: &str) -> Vec<String> {
    vec![
        "assets".into(),
        "list".into(),
        "--video".into(),
        reference.into(),
    ]
}

pub fn wants(reference: &str) -> Vec<Want> {
    let paths = vec!["video".into(), "paths".into(), reference.into()];
    let review = vec!["video".into(), "review".into(), reference.into()];
    vec![
        Want::of(asset_words(reference)),
        Want::of(paths),
        Want::of(review),
    ]
}

impl App {
    pub fn review_assets(&self) -> Vec<Value> {
        let Some(Page::Video(video)) = self.page() else {
            return Vec::new();
        };
        let assets = self
            .store
            .contents(&asset_words(&video.reference))
            .and_then(|contents| contents["items"].as_array())
            .cloned()
            .unwrap_or_default();
        let review = vec!["video".into(), "review".into(), video.reference.clone()];
        let approvals = self
            .store
            .contents(&review)
            .and_then(|contents| contents["items"].as_array())
            .cloned()
            .unwrap_or_default();
        assets
            .into_iter()
            .filter(|asset| video.filter.includes(asset, &approvals))
            .filter(|asset| {
                video.language.as_ref().is_none_or(|language| {
                    asset["path"]
                        .as_str()
                        .is_some_and(|path| path.split('/').any(|part| part == language))
                })
            })
            .collect()
    }

    pub fn review_asset(&self) -> Option<Value> {
        let Some(Page::Video(video)) = self.page() else {
            return None;
        };
        self.review_assets().get(video.selected).cloned()
    }

    pub fn video_directory(&self) -> Option<PathBuf> {
        let Some(Page::Video(video)) = self.page() else {
            return None;
        };
        let mut command = words("video paths");
        command.push(video.reference.clone());
        self.store.contents(&command)?["paths"]["root"]
            .as_str()
            .map(PathBuf::from)
    }

    pub fn review_path(&self) -> Option<PathBuf> {
        let asset = self.review_asset()?;
        Some(self.video_directory()?.join(asset["path"].as_str()?))
    }

    pub(super) fn refresh_media(&mut self) {
        let review = matches!(self.page(), Some(Page::Video(video)) if video.tab == REVIEW_TAB);
        if !review {
            return;
        }
        let path = self.review_path();
        if path.is_some() && path == self.media.requested {
            return;
        }
        self.media.requested.clone_from(&path);
        self.media.preview = None;
        if let Some(path) = path {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Preview(path),
            });
        }
    }

    /// Whether the Review tab takes this key: moving, acting on, opening or playing an item.
    #[must_use]
    pub(super) fn review_claims(&self, code: KeyCode) -> bool {
        let review = matches!(self.page(), Some(Page::Video(video)) if video.tab == REVIEW_TAB);
        let reviews = matches!(
            code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Char(
                    'j' | 'k' | 'a' | 'u' | 'n' | 'r' | 'e' | 'o' | ' ' | 'f' | 'v' | 'L'
                )
        );
        review && reviews
    }

    pub(super) fn review_command(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('L') => self.review_language(),
            KeyCode::Char('f') => self.filter_review(),
            KeyCode::Char('v') => self.compare_review(),
            KeyCode::Char('a') => self.review_action("video approve"),
            KeyCode::Char('u') => self.review_action("video unlock"),
            KeyCode::Char('n') => self.review_action("video note"),
            KeyCode::Char('r') => self.review_action("video redo"),
            KeyCode::Char('e') => self.review_action("video edit shot"),
            KeyCode::Char('o') => self.open_review(),
            KeyCode::Char(' ') => self.play_review(),
            code => self.move_review(code),
        }
    }

    fn filter_review(&mut self) {
        let here = self.here();
        let page = self.pages[here].last_mut();
        if let Some(Page::Video(video)) = page {
            video.filter = video.filter.next();
            video.selected = 0;
        }
        self.media.preview = None;
        self.media.requested = None;
        self.refresh_media();
    }

    fn compare_review(&mut self) {
        let path = self
            .review_asset()
            .and_then(|asset| asset["path"].as_str().map(str::to_owned));
        let here = self.here();
        let page = self.pages[here].last_mut();
        if let Some(Page::Video(video)) = page {
            let comparing = video.comparison.is_some();
            video.comparison = if comparing { None } else { path };
        }
        self.refresh_comparison();
    }

    fn move_review(&mut self, code: KeyCode) {
        let count = self.review_assets().len();
        let here = self.here();
        let top = self.pages[here].last_mut();
        if let Some(Page::Video(video)) = top {
            video.selected = super::super::landing::moved(video.selected, count, code);
        }
        self.refresh_media();
    }

    fn review_action(&mut self, command: &str) {
        let Some(Page::Video(video)) = self.page() else {
            return;
        };
        let Some(asset) = self.review_asset() else {
            return;
        };
        let target = asset["path"].as_str().unwrap_or_default();
        let mut fill = vec![
            ("reference".into(), video.reference.clone()),
            ("target".into(), target.into()),
            ("item".into(), target.into()),
        ];
        let scope = redo_scope(target);
        let frame = scope.iter().find(|(name, _)| name == "items");
        fill.extend(frame.map(|(_, item)| ("frame".to_owned(), item.clone())));
        fill.extend(scope);
        self.open_path(command, &fill);
    }

    fn open_review(&mut self) {
        if let Some(path) = self.review_path() {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Open(path),
            });
        }
    }

    fn play_review(&mut self) {
        if let Some(path) = self.review_path() {
            self.jobs.push(Job::Local {
                id: 0,
                task: Task::Audio(path),
            });
        }
    }
}

impl App {
    pub(super) fn refresh_comparison(&mut self) {
        let path = match self.page() {
            Some(Page::Video(video)) => video
                .comparison
                .as_ref()
                .and_then(|path| self.video_directory().map(|root| root.join(path))),
            _ => None,
        };
        if path == self.media.comparison_requested {
            return;
        }
        self.media.comparison_requested.clone_from(&path);
        self.media.comparison = None;
        if let Some(path) = path {
            self.jobs.push(Job::Local {
                id: u64::MAX,
                task: Task::Preview(path),
            });
        }
    }
}

impl App {
    fn review_language(&mut self) {
        let Some(Page::Video(video)) = self.page() else {
            return;
        };
        let words = vec!["video".into(), "show".into(), video.reference.clone()];
        let editions = self
            .store
            .contents(&words)
            .and_then(|value| value["video"]["editions"].as_array())
            .cloned()
            .unwrap_or_default();
        let selected = video.language.as_ref().and_then(|language| {
            editions
                .iter()
                .position(|value| value.as_str() == Some(language))
        });
        let next = selected.map_or(0, |index| index + 1);
        let language = editions
            .get(next)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let here = self.here();
        let page = self.pages[here].last_mut();
        if let Some(Page::Video(video)) = page {
            video.language = language;
            video.selected = 0;
        }
        self.media.requested = None;
        self.refresh_media();
    }
}

/// A saved artifact under `work/NN-<stage>/` names its stage; a trailing number is its item.
#[must_use]
pub fn redo_scope(path: &str) -> Vec<(String, String)> {
    let Some((stage, file)) = stage_and_file(path) else {
        return Vec::new();
    };
    let item = trailing_number(file);
    std::iter::once(("stage".to_owned(), stage.to_owned()))
        .chain(item.map(|item| ("items".to_owned(), item)))
        .collect()
}

fn stage_and_file(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.split('/').skip_while(|part| *part != "work").skip(1);
    let (_, stage) = parts.next()?.split_once('-')?;
    Some((stage, parts.last().unwrap_or_default()))
}

fn trailing_number(file: &str) -> Option<String> {
    let stem = file.split('.').next().unwrap_or_default();
    let prefix = stem.trim_end_matches(|letter: char| letter.is_ascii_digit());
    let digits = stem.strip_prefix(prefix).unwrap_or_default();
    let item = digits.trim_start_matches('0');
    (!item.is_empty()).then(|| item.to_owned())
}
