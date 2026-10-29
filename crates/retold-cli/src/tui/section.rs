//! The sidebar: every screen belongs to one section, reachable by its number key.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    Home,
    Create,
    Videos,
    Series,
    Studio,
    Library,
    Providers,
    Money,
    Configure,
    System,
    Tools,
}

/// What the sidebar and palette say about a section.
struct Facts {
    title: &'static str,
    glyph: &'static str,
    summary: &'static str,
    /// `1`–`9`, then `0`; Tools has no digit and is reached from the sidebar.
    digit: Option<char>,
}

/// In sidebar order; `Section::position` indexes this table.
const SIDEBAR: [Facts; 11] = [
    Facts {
        title: "Home",
        glyph: "⌂",
        summary: "Where you left off, what needs attention, and what to do next.",
        digit: Some('1'),
    },
    Facts {
        title: "Create",
        glyph: "+",
        summary: "Start a new video or series from one of your guides.",
        digit: Some('2'),
    },
    Facts {
        title: "Videos",
        glyph: "▶",
        summary: "Every saved video: plan, continue, review, edit and export.",
        digit: Some('3'),
    },
    Facts {
        title: "Series",
        glyph: "≡",
        summary: "Multi-part series and the progress of each part.",
        digit: Some('4'),
    },
    Facts {
        title: "Studio",
        glyph: "◎",
        summary: "Background work: the studio worker and its job queue.",
        digit: Some('5'),
    },
    Facts {
        title: "Library",
        glyph: "♪",
        summary: "Generated assets, background music and sound effects.",
        digit: Some('6'),
    },
    Facts {
        title: "Providers",
        glyph: "⇄",
        summary: "The AI services Retold uses, their routes, logins and health.",
        digit: Some('7'),
    },
    Facts {
        title: "Money",
        glyph: "$",
        summary: "What has been spent, what is held, and the limits that cap it.",
        digit: Some('8'),
    },
    Facts {
        title: "Configure",
        glyph: "✎",
        summary: "Guides, channels, export presets, saved views and profiles.",
        digit: Some('9'),
    },
    Facts {
        title: "System",
        glyph: "⚙",
        summary: "Workspaces, storage, backups, diagnostics and reference.",
        digit: Some('0'),
    },
    Facts {
        title: "Tools",
        glyph: "✦",
        summary: "Creative audits and the cinematic prompt compiler.",
        digit: None,
    },
];

pub const ALL: [Section; 11] = [
    Section::Home,
    Section::Create,
    Section::Videos,
    Section::Series,
    Section::Studio,
    Section::Library,
    Section::Providers,
    Section::Money,
    Section::Configure,
    Section::System,
    Section::Tools,
];

impl Section {
    /// Where the section sits in the sidebar; also its row in `SIDEBAR`.
    #[must_use]
    pub const fn position(self) -> usize {
        self as usize
    }

    const fn facts(self) -> &'static Facts {
        &SIDEBAR[self.position()]
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        self.facts().title
    }

    /// A one-cell mark shown beside the title.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        self.facts().glyph
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        self.facts().summary
    }

    #[must_use]
    pub const fn digit(self) -> Option<char> {
        self.facts().digit
    }

    #[must_use]
    pub fn from_digit(digit: char) -> Option<Self> {
        ALL.into_iter()
            .find(|section| section.digit() == Some(digit))
    }

    /// The section `step` places above (negative) or below in the sidebar, wrapping around.
    #[must_use]
    pub fn offset(self, step: isize) -> Self {
        let count = ALL.len().cast_signed();
        let index = self.position().cast_signed() + step;
        let wrapped = index.rem_euclid(count).cast_unsigned();
        ALL.get(wrapped).copied().unwrap_or(self)
    }
}
