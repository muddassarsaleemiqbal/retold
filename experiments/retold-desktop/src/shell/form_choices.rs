//! Pick-lists for form fields that name something the workspace already has.
//! Operations that create a new item never suggest existing names.
use std::collections::BTreeMap;

use super::Shell;

/// (label shown, value entered) pairs for one field.
pub(super) type Choices = Vec<(String, String)>;

/// Verbs that need a new name rather than an existing one.
const CREATING: [&str; 6] = ["new", "create", "save", "add", "init", "import"];

fn creating(path: &str) -> bool {
    path.rsplit(' ')
        .next()
        .is_some_and(|verb| CREATING.contains(&verb))
}

fn pairs<'a>(items: impl Iterator<Item = (&'a str, &'a str)>) -> Choices {
    items
        .map(|(label, value)| (label.to_owned(), value.to_owned()))
        .collect()
}

impl Shell {
    fn video_choices(&self) -> Choices {
        self.snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
            pairs(
                snapshot
                    .videos
                    .iter()
                    .map(|video| (video.title.as_str(), video.reference.as_str())),
            )
        })
    }

    fn series_ids(&self) -> Choices {
        self.series.list.as_ref().map_or_else(Vec::new, |list| {
            pairs(
                list.iter()
                    .map(|series| (series.work.as_str(), series.id.as_str())),
            )
        })
    }

    fn jobs(&self) -> Choices {
        let page = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.activity.as_ref().ok());
        page.map_or_else(Vec::new, |activity| {
            activity
                .page
                .jobs
                .iter()
                .map(|job| {
                    (
                        format!("{} · {} · {}", job.id, job.video_ref, job.status.as_str()),
                        job.id.clone(),
                    )
                })
                .collect()
        })
    }

    fn providers_ids(&self) -> Choices {
        self.providers
            .snapshot
            .as_ref()
            .map_or_else(Vec::new, |snapshot| {
                pairs(
                    snapshot
                        .providers
                        .iter()
                        .map(|provider| (provider.id.as_str(), provider.id.as_str())),
                )
            })
    }

    fn guides_and_channels(&self, guides: bool) -> Choices {
        let choices = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.choices.as_ref().ok());
        choices.map_or_else(Vec::new, |choices| {
            if guides {
                pairs(
                    choices
                        .guides
                        .iter()
                        .map(|guide| (guide.name.as_str(), guide.id.as_str())),
                )
            } else {
                pairs(
                    choices
                        .channels
                        .iter()
                        .map(|channel| (channel.as_str(), channel.as_str())),
                )
            }
        })
    }

    fn named_records(&self, path: &str) -> Choices {
        if path.starts_with("export-presets ") {
            return self
                .admin
                .presets
                .iter()
                .flatten()
                .map(|preset| (preset.name.clone(), preset.name.clone()))
                .collect();
        }
        if path.starts_with("views ") {
            return self
                .views
                .saved
                .iter()
                .flatten()
                .map(|view| (view.name.clone(), view.name.clone()))
                .collect();
        }
        if path.starts_with("config profile ") {
            return self
                .admin
                .profiles
                .iter()
                .flat_map(|(profiles, _)| profiles)
                .map(|profile| (profile.name.clone(), profile.name.clone()))
                .collect();
        }
        Vec::new()
    }

    fn materials(&self, path: &str) -> Choices {
        let Some(panel) = &self.materials else {
            return Vec::new();
        };
        if path.starts_with("library ") {
            let category = path.split(' ').nth(1).unwrap_or_default();
            return panel
                .audio
                .iter()
                .flatten()
                .filter(|item| item.category.as_str() == category)
                .map(|item| (item.title.clone(), item.id.clone()))
                .collect();
        }
        panel
            .items
            .iter()
            .flatten()
            .map(|item| (item.name.clone(), item.id.clone()))
            .collect()
    }

    /// Spending scopes: the whole account, then each video, series and provider by name.
    fn scopes(&self) -> Choices {
        let prefixed = |kind: &str, label: &str, choices: Choices| {
            choices
                .into_iter()
                .map(|(name, id)| (format!("{label}: {name}"), format!("{kind}:{id}")))
                .collect::<Vec<_>>()
        };
        std::iter::once(("Whole account".to_owned(), "account".to_owned()))
            .chain(prefixed("video", "Video", self.video_choices()))
            .chain(prefixed("series", "Series", self.series_ids()))
            .chain(prefixed("provider", "Provider", self.providers_ids()))
            .collect()
    }

    /// Pick-lists for the fields of `path`, keyed by field id.
    pub(super) fn field_choices(&self, path: &str) -> BTreeMap<String, Choices> {
        let mut choices = BTreeMap::new();
        for field in ["reference", "video"] {
            choices.insert(field.to_owned(), self.video_choices());
        }
        if creating(path) {
            return choices;
        }
        let id = if path.starts_with("series ") {
            self.series_ids()
        } else if path.starts_with("queue ") {
            self.jobs()
        } else if path.starts_with("providers ") {
            self.providers_ids()
        } else if path.starts_with("guides ") {
            self.guides_and_channels(true)
        } else if path.starts_with("channels ") {
            self.guides_and_channels(false)
        } else {
            self.materials(path)
        };
        if path.starts_with("account ") {
            choices.insert("scope".into(), self.scopes());
        }
        choices.insert("id".into(), id.clone());
        choices.insert("asset-id".into(), id);
        choices.insert("series".into(), self.series_ids());
        choices.insert("channel".into(), self.guides_and_channels(false));
        choices.insert("name".into(), self.named_records(path));
        choices
    }
}
