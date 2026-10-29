//! Reports and their scoped shortcuts share one definition for keys and visible hints.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub title: &'static str,
    pub command: &'static str,
}
pub const DOCTOR: Report = Report {
    title: "Health",
    command: "doctor",
};
pub const STORAGE: Report = Report {
    title: "Storage",
    command: "storage usage --by kind",
};
pub const VIDEO_STORAGE: Report = Report {
    title: "By video",
    command: "storage usage --by video",
};
pub const ACCOUNT: Report = Report {
    title: "Account",
    command: "account show",
};
pub const BACKUPS: Report = Report {
    title: "Backups",
    command: "account show",
};
pub const STUDIO: Report = Report {
    title: "Daemon",
    command: "studio status",
};
type Shortcuts = &'static [(&'static str, &'static str, &'static str)];
const MONEY: Shortcuts = &[
    ("i", "account init", "initialize"),
    ("b", "account backup", "backup"),
    ("v", "account verify-backup", "verify"),
];
const STORAGE_KEYS: Shortcuts = &[
    ("f", "doctor", "repair"),
    ("b", "storage backup", "backup"),
    ("v", "storage verify", "verify"),
    ("r", "storage restore", "restore"),
    ("a", "storage activate", "activate"),
];
const STUDIO_KEYS: Shortcuts = &[("s", "studio start", "start"), ("x", "studio stop", "stop")];
impl Report {
    pub fn shortcuts(&self) -> Shortcuts {
        match self.command {
            "account show" => MONEY,
            "studio status" => STUDIO_KEYS,
            _ => STORAGE_KEYS,
        }
    }
}
