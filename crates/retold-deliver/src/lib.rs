//! Validated local posting packages. No network, platform clients or CTR predictions.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aspect {
    #[serde(rename = "9:16")]
    Portrait,
    #[serde(rename = "16:9")]
    Landscape,
    #[serde(rename = "1:1")]
    Square,
}
impl Aspect {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "9:16" => Ok(Self::Portrait),
            "16:9" => Ok(Self::Landscape),
            "1:1" => Ok(Self::Square),
            _ => Err(format!("unsupported delivery aspect: {value}")),
        }
    }
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Portrait => "9x16",
            Self::Landscape => "16x9",
            Self::Square => "1x1",
        }
    }
    #[must_use]
    pub const fn ratio(self) -> &'static str {
        match self {
            Self::Portrait => "9:16",
            Self::Landscape => "16:9",
            Self::Square => "1:1",
        }
    }
    #[must_use]
    pub const fn dimensions(self) -> [u32; 2] {
        match self {
            Self::Portrait => [1080, 1920],
            Self::Landscape => [1920, 1080],
            Self::Square => [1080, 1080],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub seconds: f64,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub platform: String,
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
}
/// Conservative portable bounds; `YouTube` additionally limits UTF-8 description bytes.
pub fn validate_posts(posts: &[Post]) -> Result<(), String> {
    let mut platforms = std::collections::BTreeSet::new();
    if posts.is_empty() {
        return Err("delivery requires posting metadata".into());
    }
    for p in posts {
        let text = format!("{}\n\n{}", p.description, p.hashtags.join(" "));
        if !["youtube", "tiktok", "instagram", "facebook"].contains(&p.platform.as_str())
            || !platforms.insert(&p.platform)
            || p.title.trim().is_empty()
            || p.title.encode_utf16().count() > 100
            || p.title.contains(['<', '>', '\n', '\r'])
            || p.description.trim().is_empty()
            || text.encode_utf16().count() > 2200
            || text.len() > 5000
            || text.contains(['<', '>'])
            || p.hashtags.len() > 5
            || p.hashtags
                .iter()
                .any(|t| !t.starts_with('#') || t.len() < 2 || t.chars().any(char::is_whitespace))
            || p.hashtags
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != p.hashtags.len()
        {
            return Err(format!(
                "{} metadata exceeds portable posting limits",
                p.platform
            ));
        }
    }
    Ok(())
}

pub fn validate_chapters(chapters: &[Chapter], duration: f64) -> Result<(), String> {
    if !duration.is_finite()
        || duration <= 0.0
        || chapters.is_empty()
        || chapters[0].seconds.abs() > f64::EPSILON
        || chapters.iter().any(|c| {
            !c.seconds.is_finite()
                || c.seconds < 0.0
                || c.seconds >= duration
                || c.title.trim().is_empty()
                || c.title.contains(['\n', '\r'])
        })
        || chapters.windows(2).any(|p| p[0].seconds >= p[1].seconds)
    {
        return Err("invalid delivery chapters".into());
    }
    Ok(())
}
pub fn chapters_text(chapters: &[Chapter]) -> Result<String, String> {
    use std::fmt::Write;
    let mut out = String::new();
    for c in chapters {
        let seconds = std::time::Duration::try_from_secs_f64(c.seconds)
            .map_err(|e| e.to_string())?
            .as_secs();
        let _ = writeln!(
            out,
            "{:02}:{:02}:{:02} {}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            c.title
        );
    }
    Ok(out)
}
