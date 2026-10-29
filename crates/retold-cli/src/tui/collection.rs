//! A collection is anything listed by a `list` command: videos, guides, providers, jobs. Each one
//! is described by data, so every collection gets the same list, detail, filter and actions.
use serde_json::Value;

use super::store::{self, Want};
use crate::human::text;

#[derive(Debug, PartialEq, Eq)]
pub struct Collection {
    pub name: &'static str,
    /// The read command that lists the items.
    pub list: &'static str,
    /// The array inside the result that holds the items.
    pub items: &'static str,
    /// Read command shown for the selected item; the item key is appended.
    pub detail: Option<&'static str>,
    /// Command groups whose actions apply to an item, such as `video`.
    pub groups: &'static [&'static str],
    /// Argument ids that take the item key when an item action opens.
    pub subjects: &'static [&'static str],
    /// The action `n` opens.
    pub create: Option<&'static str>,
    pub empty: &'static str,
    /// Refresh on a timer, for things that change by themselves.
    pub live: bool,
    /// What Enter on an item does.
    pub opening: Opening,
    /// Values `n` fills into the create action, and what the key hint calls it.
    pub create_with: &'static [(&'static str, &'static str)],
    pub create_label: &'static str,
}

/// What Enter on a collection item opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// The video workspace.
    Video,
    /// The job's live event stream.
    Attach,
    Route,
    Reconcile,
    Document,
    Series,
    Diagnostic,
    Limit,
    ProviderStatus,
    Discovery,
    /// The actions that apply to the item.
    Menu,
    Inspect,
}

pub const VIDEOS: Collection = Collection {
    name: "Videos",
    list: "video list",
    items: "items",
    detail: Some("video plan"),
    groups: &["video"],
    subjects: &["reference"],
    create: Some("new"),
    empty: "No videos yet. Press n to create your first one.",
    live: false,
    opening: Opening::Video,
    create_with: &[],
    create_label: "new",
};

pub const PROCESSES: Collection = Collection {
    name: "Processes",
    list: "account processes --limit 100",
    items: "processes",
    detail: None,
    groups: &[],
    subjects: &[],
    create: None,
    empty: "No supervised processes are recorded.",
    live: true,
    opening: Opening::Inspect,
    create_with: &[],
    create_label: "new",
};

pub const ATTEMPTS: Collection = Collection {
    name: "Attempts",
    list: "account attempts --limit 200",
    items: "attempts",
    detail: None,
    groups: &["account reconcile"],
    subjects: &["attempt"],
    create: None,
    empty: "No recorded attempts.",
    live: true,
    opening: Opening::Reconcile,
    create_with: &[],
    create_label: "new",
};

pub const LIMITS: Collection = Collection {
    name: "Limits",
    list: "account show",
    items: "limits",
    detail: None,
    groups: &["account limit"],
    subjects: &["scope"],
    create: Some("account limit"),
    empty: "No limits. Press n to set a finite ceiling.",
    live: true,
    opening: Opening::Limit,
    create_with: &[],
    create_label: "set limit",
};

pub const SERIES: Collection = Collection {
    name: "Series",
    list: "series list",
    items: "items",
    detail: Some("series show"),
    groups: &["series"],
    subjects: &["series"],
    create: Some("series new"),
    empty: "No series yet. Press n to start one.",
    live: false,
    opening: Opening::Series,
    create_with: &[],
    create_label: "new",
};

pub const JOBS: Collection = Collection {
    name: "Jobs",
    list: "queue list",
    items: "jobs",
    detail: Some("queue show"),
    groups: &["queue"],
    subjects: &["id"],
    create: None,
    empty: "No jobs. Continue a video in the background and it appears here.",
    live: true,
    opening: Opening::Attach,
    create_with: &[],
    create_label: "new",
};

pub const ASSETS: Collection = Collection {
    name: "Assets",
    list: "assets list",
    items: "items",
    detail: Some("assets show"),
    groups: &["assets"],
    subjects: &["asset-id"],
    create: None,
    empty: "No generated assets yet. They appear as videos are made.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const MUSIC: Collection = Collection {
    name: "Music",
    list: "library music list",
    items: "items",
    detail: None,
    groups: &["library music"],
    subjects: &["id"],
    create: Some("library music import"),
    empty: "No music yet. Press n to import a track.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const EFFECTS: Collection = Collection {
    name: "Sound effects",
    list: "library sfx list",
    items: "items",
    detail: None,
    groups: &["library sfx"],
    subjects: &["id"],
    create: Some("library sfx import"),
    empty: "No sound effects yet. Press n to import one.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const PROVIDERS: Collection = Collection {
    name: "Providers",
    list: "providers list",
    items: "providers",
    detail: None,
    groups: &["providers"],
    subjects: &["id"],
    create: Some("providers add"),
    empty: "No providers yet. Press n to add one, or open the Discover tab.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const ROUTES: Collection = Collection {
    name: "Routes",
    list: "providers route",
    items: "routing",
    detail: None,
    groups: &["providers route"],
    subjects: &["tier"],
    create: Some("providers route"),
    empty: "No routing tiers available.",
    live: false,
    opening: Opening::Route,
    create_with: &[("suggest", "true")],
    create_label: "suggest routes",
};

pub const HEALTH: Collection = Collection {
    name: "Health",
    list: "providers health",
    items: "health",
    detail: None,
    groups: &["providers"],
    subjects: &["id"],
    create: None,
    empty: "No providers yet. Add a provider in the Providers tab.",
    live: true,
    opening: Opening::ProviderStatus,
    create_with: &[],
    create_label: "new",
};

pub const LOGINS: Collection = Collection {
    name: "Login",
    list: "providers list",
    items: "providers",
    detail: None,
    groups: &["providers"],
    subjects: &["id"],
    create: None,
    empty: "No providers yet. Add a provider in the Providers tab.",
    live: false,
    opening: Opening::ProviderStatus,
    create_with: &[],
    create_label: "new",
};

pub const DISCOVER: Collection = Collection {
    name: "Discover",
    list: "providers scan",
    items: "inventory",
    detail: None,
    groups: &[],
    subjects: &[],
    create: Some("providers add"),
    empty: "No supported providers discovered. Press n to add one manually, or Ctrl-R to scan again.",
    live: false,
    opening: Opening::Discovery,
    create_with: &[],
    create_label: "new",
};

pub const GUIDES: Collection = Collection {
    name: "Guides",
    list: "guides list",
    items: "items",
    detail: Some("guides show"),
    groups: &["guides"],
    subjects: &["guide", "id"],
    create: Some("guides create"),
    empty: "No guides yet. Press n to create one.",
    live: false,
    opening: Opening::Document,
    create_with: &[],
    create_label: "new",
};

pub const CHANNELS: Collection = Collection {
    name: "Channels",
    list: "channels list",
    items: "items",
    detail: Some("channels show"),
    groups: &["channels"],
    subjects: &["channel", "id"],
    create: Some("channels create"),
    empty: "No channels yet. Press n to create one.",
    live: false,
    opening: Opening::Document,
    create_with: &[],
    create_label: "new",
};

pub const PRESETS: Collection = Collection {
    name: "Export presets",
    list: "export-presets list",
    items: "items",
    detail: Some("export-presets show"),
    groups: &["export-presets"],
    subjects: &["name"],
    create: Some("export-presets new"),
    empty: "No export presets yet. Press n to add one.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const VIEWS: Collection = Collection {
    name: "Saved views",
    list: "views list",
    items: "items",
    detail: Some("views show"),
    groups: &["views"],
    subjects: &["name"],
    create: Some("views save"),
    empty: "No saved views yet.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const PROFILES: Collection = Collection {
    name: "Profiles",
    list: "config profile list",
    items: "items",
    detail: Some("config profile show"),
    groups: &["config profile"],
    subjects: &["name"],
    create: Some("config profile new"),
    empty: "No preference profiles yet. Press n to create one.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

pub const WORKSPACES: Collection = Collection {
    name: "Workspaces",
    list: "workspace list",
    items: "items",
    detail: None,
    groups: &["workspace"],
    subjects: &["path"],
    create: Some("workspace init"),
    empty: "No registered workspaces. Press n to create one.",
    live: false,
    opening: Opening::Menu,
    create_with: &[],
    create_label: "new",
};

impl Collection {
    /// The reads its tab shows: the list, the selected item's detail, and any extras.
    #[must_use]
    pub fn wants(&self, selected: Option<&Item>) -> Vec<Want> {
        let list = if self.live {
            Want::live(self.list)
        } else {
            Want::once(self.list)
        };
        let detail = self
            .detail
            .zip(selected)
            .map(|(command, item)| Want::of(detail_words(command, item)));
        let is_providers = self == &PROVIDERS;
        let extras = is_providers.then(|| Want::once("providers route"));
        let status_reads = self.status_wants(selected);
        let route_providers = (self == &ROUTES).then(|| Want::once("providers list"));
        std::iter::once(list)
            .chain(detail)
            .chain(route_providers)
            .chain(status_reads)
            .chain(extras)
            .collect()
    }
}

impl Collection {
    fn status_wants(&self, selected: Option<&Item>) -> Vec<Want> {
        let status_tab = self == &PROVIDERS || self == &HEALTH || self == &LOGINS;
        if !status_tab {
            return Vec::new();
        }
        let mut wanted = Vec::new();
        if self != &HEALTH {
            wanted.push(Want::live("providers health"));
        }
        if let Some(item) = selected {
            let mut login = Want::of(super::page::Reading::provider(&item.key).words);
            login.live = true;
            wanted.push(login);
        }
        wanted
    }
}

/// `command` followed by the item's key, such as `guides show story`.
#[must_use]
pub fn detail_words(command: &str, item: &Item) -> Vec<String> {
    let mut words = store::words(command);
    words.push(item.key.clone());
    words
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// What commands take: a video reference, an ID or a name.
    pub key: String,
    pub label: String,
    pub note: String,
    pub raw: Value,
}

const KEY_FIELDS: [&str; 4] = ["ref", "id", "name", "path"];
const LABEL_FIELDS: [&str; 3] = ["title", "name", "work"];
const NOTE_FIELDS: [&str; 6] = ["summary", "status", "state", "kind", "handle", "category"];

#[must_use]
pub fn items(collection: &Collection, contents: &Value) -> Vec<Item> {
    if collection == &PROCESSES {
        return super::processes::items(contents);
    }
    if collection == &ATTEMPTS {
        return contents["account_attempts"]["attempts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(item)
            .collect();
    }
    if collection == &LIMITS {
        return super::money::limits(contents);
    }
    if collection == &DISCOVER {
        return super::discovery::items(contents);
    }
    if collection == &ROUTES {
        return route_items(contents);
    }
    contents[collection.items]
        .as_array()
        .map(|list| list.iter().map(item).collect())
        .unwrap_or_default()
}

fn route_items(contents: &Value) -> Vec<Item> {
    let routing = contents["routing"].as_object();
    retold_app::providers::routing_tiers()
        .into_iter()
        .map(|tier| {
            let key = tier.to_string();
            let chain = routing
                .and_then(|configured| configured.get(&key))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([]));
            let note = chain
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" → ");
            Item {
                label: key.clone(),
                key,
                note,
                raw: chain,
            }
        })
        .collect()
}

fn item(raw: &Value) -> Item {
    let key = first(raw, &KEY_FIELDS);
    let label = first(raw, &LABEL_FIELDS);
    let note = NOTE_FIELDS
        .iter()
        .map(|field| text(raw, field))
        .find(|value| value != "unknown" && *value != label)
        .unwrap_or_default();
    Item {
        label: if label.is_empty() { key.clone() } else { label },
        key,
        note,
        raw: raw.clone(),
    }
}

fn first(raw: &Value, fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| text(raw, field))
        .find(|value| value != "unknown")
        .unwrap_or_default()
}

/// Items whose key, label or note contain every word of `filter`, ignoring case.
#[must_use]
pub fn matching<'a>(items: &'a [Item], filter: &str) -> Vec<&'a Item> {
    let needles: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    items
        .iter()
        .filter(|item| {
            let haystack = format!("{} {} {}", item.key, item.label, item.note).to_lowercase();
            needles.iter().all(|needle| haystack.contains(needle))
        })
        .collect()
}

pub const DIAGNOSTICS: Collection = Collection {
    name: "Diagnostics",
    list: "diagnostics",
    items: "items",
    detail: Some("explain"),
    groups: &["explain"],
    subjects: &["code"],
    create: None,
    empty: "No diagnostic codes.",
    live: false,
    opening: Opening::Diagnostic,
    create_with: &[],
    create_label: "new",
};
