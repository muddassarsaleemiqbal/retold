//! Draft fallback chains; only the shared provider command may persist a change.
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;

use super::input::Input;

#[derive(Debug, Clone)]
pub struct Editor {
    pub tier: String,
    pub chain: Vec<String>,
    pub selected: usize,
    pub providers: Vec<String>,
    pub adding: Option<Input>,
    pub candidate: usize,
    pub problem: Option<String>,
}

impl Editor {
    pub fn new(tier: &str, chain: &Value, providers: &Value) -> Self {
        let chain = chain
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let providers = providers["providers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|provider| {
                provider["supported_tiers"]
                    .as_array()
                    .is_some_and(|tiers| tiers.iter().any(|value| value.as_str() == Some(tier)))
            })
            .filter_map(|provider| provider["id"].as_str())
            .map(str::to_owned)
            .collect();
        Self {
            tier: tier.into(),
            chain,
            selected: 0,
            providers,
            adding: None,
            candidate: 0,
            problem: None,
        }
    }

    pub fn words(&self) -> Vec<String> {
        vec![
            "providers".into(),
            "route".into(),
            self.tier.clone(),
            "--order".into(),
            self.chain.join(","),
        ]
    }

    /// Enter outside the entry field requests review; Esc outside it discards the draft.
    pub fn press(&mut self, key: KeyEvent) {
        let adding = self.adding.is_some();
        if adding {
            self.add_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('+') => self.begin_add(),
            KeyCode::Char('-') | KeyCode::Delete => self.remove(),
            KeyCode::Char('J') => self.reorder(1),
            KeyCode::Char('K') => self.reorder(-1),
            code => self.selected = super::landing::moved(self.selected, self.chain.len(), code),
        }
    }

    fn begin_add(&mut self) {
        self.adding = Some(Input::default());
        self.pick_provider();
        let unavailable = self.providers.is_empty();
        if unavailable {
            self.problem = Some("No compatible providers. Add one in Providers first.".into());
        }
    }

    fn remove(&mut self) {
        let exists = self.selected < self.chain.len();
        if exists {
            self.chain.remove(self.selected);
            self.clamp_selection();
        }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.chain.len().saturating_sub(1));
    }

    fn reorder(&mut self, step: isize) {
        let next = self.selected.saturating_add_signed(step);
        let in_bounds = self.selected < self.chain.len() && next < self.chain.len();
        if in_bounds {
            self.chain.swap(self.selected, next);
            self.selected = next;
        }
    }

    fn add_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.adding = None;
                self.problem = None;
            }
            KeyCode::Tab => {
                self.candidate = self.candidate.saturating_add(1);
                self.pick_provider();
            }
            KeyCode::Enter => self.append(),
            _ => {
                if let Some(input) = &mut self.adding {
                    input.edit(key);
                }
            }
        }
    }

    fn pick_provider(&mut self) {
        let unavailable = self.providers.is_empty();
        if unavailable {
            return;
        }
        self.candidate %= self.providers.len();
        let mut input = Input::default();
        let value = format!("{}:", self.providers[self.candidate]);
        for letter in value.chars() {
            input.edit(KeyEvent::from(KeyCode::Char(letter)));
        }
        self.adding = Some(input);
    }

    fn append(&mut self) {
        let value = self
            .adding
            .as_ref()
            .map(Input::text)
            .unwrap_or_default()
            .to_owned();
        self.problem = self.entry_problem(&value);
        let invalid = self.problem.is_some();
        if invalid {
            return;
        }
        self.accept_entry(value);
    }

    fn accept_entry(&mut self, value: String) {
        self.chain.push(value);
        self.selected = self.chain.len().saturating_sub(1);
        self.adding = None;
    }

    fn entry_problem(&self, value: &str) -> Option<String> {
        let supported = self.supports_entry(value);
        let syntax = !value.chars().any(char::is_whitespace) && !value.contains(',');
        let duplicate = self.chain.iter().any(|entry| entry == value);
        match (supported && syntax, duplicate) {
            (false, _) => {
                Some("Choose a compatible provider and enter its selector after ':'.".into())
            }
            (true, true) => Some("That entry is already in the chain.".into()),
            (true, false) => None,
        }
    }

    fn supports_entry(&self, value: &str) -> bool {
        value.split_once(':').is_some_and(|(id, selector)| {
            self.providers.iter().any(|provider| provider == id) && !selector.is_empty()
        })
    }

    pub fn react(mut self, key: KeyEvent) -> Decision {
        let adding = self.adding.is_some();
        let filled = !self.chain.is_empty();
        match key.code {
            KeyCode::Esc if !adding => Decision::Discard,
            KeyCode::Enter if !adding && filled => Decision::Review {
                tier: self.tier,
                order: self.chain.join(","),
            },
            _ => {
                self.press(key);
                Decision::Editing(self)
            }
        }
    }
}

pub enum Decision {
    Discard,
    Review { tier: String, order: String },
    Editing(Editor),
}
