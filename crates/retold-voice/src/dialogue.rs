//! Multi-character dialogue parsing with emotional metadata tags.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors arising during dialogue parsing and scene extraction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DialogueError {
    #[error("Empty screenplay script provided")]
    EmptyScript,
    #[error("unknown emotion: {0}")]
    UnknownEmotion(String),
    #[error("unknown speaker: {0}")]
    UnknownSpeaker(String),

    #[error("Invalid dialogue turn format at line {0}: '{1}'")]
    InvalidTurnFormat(usize, String),
}

/// Emotional prosody modulation tags attached to dialogue lines.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmotionalTag {
    Neutral,
    Furious,
    Terrified,
    Whispering,
    Solemn,
    Joyful,
    Sarcastic,
    Melancholy,
}

impl EmotionalTag {
    /// Parses an emotional tag from a bracketed string like `"[furious]"`.
    pub fn parse_tag(raw: &str) -> Result<Self, DialogueError> {
        const EMOTIONAL_TAG_KEYWORDS: &[(&[&str], EmotionalTag)] = &[
            (&["furious", "angry", "rage"], EmotionalTag::Furious),
            (
                &["terrified", "afraid", "panicked"],
                EmotionalTag::Terrified,
            ),
            (
                &["whispering", "whisper", "quiet"],
                EmotionalTag::Whispering,
            ),
            (&["solemn", "grave", "serious"], EmotionalTag::Solemn),
            (&["joyful", "happy", "excited"], EmotionalTag::Joyful),
            (&["sarcastic", "ironic"], EmotionalTag::Sarcastic),
            (&["melancholy", "sad", "mournful"], EmotionalTag::Melancholy),
            (&["", "neutral"], EmotionalTag::Neutral),
        ];
        let clean = raw
            .trim()
            .trim_matches(|c| c == '[' || c == ']')
            .to_lowercase();
        for (keywords, tag) in EMOTIONAL_TAG_KEYWORDS {
            if keywords.contains(&clean.as_str()) {
                return Ok(tag.clone());
            }
        }
        Err(DialogueError::UnknownEmotion(clean))
    }
}

/// Narrative role within the theatrical dramatis personae.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpeakerRole {
    Narrator,
    Protagonist,
    Antagonist,
    Witness,
    Supporting(String),
}

impl SpeakerRole {
    /// Infers speaker role from character name.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let upper = name.trim().to_uppercase();
        if upper.contains("NARRATOR") || upper.contains("VOICEOVER") || upper == "VO" {
            Self::Narrator
        } else if upper.contains("PROTAGONIST") || upper.contains("HERO") {
            Self::Protagonist
        } else if upper.contains("ANTAGONIST") || upper.contains("VILLAIN") {
            Self::Antagonist
        } else if upper.contains("WITNESS") || upper.contains("INFORMANT") {
            Self::Witness
        } else {
            Self::Supporting(name.trim().to_string())
        }
    }
}

/// A single character dialogue turn with emotional delivery tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueTurn {
    pub speaker: String,
    pub role: SpeakerRole,
    pub emotion: EmotionalTag,
    pub text: String,
}

/// A parsed screenplay scene comprising multiple character turns.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DialogueScript {
    pub turns: Vec<DialogueTurn>,
}

impl DialogueScript {
    /// Parses screenplay text formatted with character cues and emotional brackets.
    ///
    /// Example format:
    ///
    /// ```text
    /// NARRATOR: The cathedral fell silent.
    /// MARCUS [furious]: You were supposed to guard the gates!
    /// ELENA [whispering]: Be quiet, they are still listening.
    /// ```
    pub fn parse(text: &str) -> Result<Self, DialogueError> {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();

        if lines.is_empty() {
            return Err(DialogueError::EmptyScript);
        }

        let mut turns = Vec::with_capacity(lines.len());
        for (idx, line) in lines.iter().enumerate() {
            turns.push(Self::parse_line(idx + 1, line)?);
        }

        Ok(Self { turns })
    }

    /// Parses a single line into a `DialogueTurn`.
    fn parse_line(line_num: usize, line: &str) -> Result<DialogueTurn, DialogueError> {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() < 2 {
            return Err(DialogueError::InvalidTurnFormat(line_num, line.to_string()));
        }

        let speaker_header = parts[0].trim();
        let spoken_text = parts[1].trim();

        if speaker_header.is_empty() || spoken_text.is_empty() {
            return Err(DialogueError::InvalidTurnFormat(line_num, line.to_string()));
        }

        let (speaker, emotion) = extract_speaker_and_emotion(speaker_header)?;
        let role = SpeakerRole::from_name(&speaker);

        Ok(DialogueTurn {
            speaker,
            role,
            emotion,
            text: spoken_text.to_string(),
        })
    }

    pub fn validate_speakers(&self, allowed: &[String]) -> Result<(), DialogueError> {
        for turn in &self.turns {
            if turn.speaker != "NARRATOR" && !allowed.contains(&turn.speaker) {
                return Err(DialogueError::UnknownSpeaker(turn.speaker.clone()));
            }
        }
        Ok(())
    }
    /// Collects unique speaker names present in the scene.
    #[must_use]
    pub fn speakers(&self) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for turn in &self.turns {
            set.insert(turn.speaker.clone());
        }
        set.into_iter().collect()
    }
}

/// Extracts speaker name and optional emotional bracket from header.
fn extract_speaker_and_emotion(header: &str) -> Result<(String, EmotionalTag), DialogueError> {
    if let Some(start) = header.find('[') {
        let content_end = header.len().saturating_sub(1);
        if !header.ends_with(']')
            || header[start + 1..content_end].contains(['[', ']'])
            || header[..start].trim().is_empty()
        {
            return Err(DialogueError::InvalidTurnFormat(0, header.into()));
        }
        return Ok((
            header[..start].trim().into(),
            EmotionalTag::parse_tag(&header[start..])?,
        ));
    }
    if header.contains(']') {
        return Err(DialogueError::InvalidTurnFormat(0, header.into()));
    }
    Ok((header.into(), EmotionalTag::Neutral))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_theatrical_dialogue_with_emotions() {
        let script_text = r"
            NARRATOR: The night was bitterly cold.
            MARCUS [furious]: How could you betray the covenant?
            ELENA [whispering]: I had no choice, they took the children.
            SOLDIER [terrified]: The perimeter is breached!
        ";

        let script = DialogueScript::parse(script_text).unwrap();
        assert_eq!(script.turns.len(), 4);

        assert_eq!(script.turns[0].speaker, "NARRATOR");
        assert_eq!(script.turns[0].role, SpeakerRole::Narrator);
        assert_eq!(script.turns[0].emotion, EmotionalTag::Neutral);

        assert_eq!(script.turns[1].speaker, "MARCUS");
        assert_eq!(script.turns[1].emotion, EmotionalTag::Furious);

        assert_eq!(script.turns[2].speaker, "ELENA");
        assert_eq!(script.turns[2].emotion, EmotionalTag::Whispering);

        assert_eq!(script.turns[3].speaker, "SOLDIER");
        assert_eq!(script.turns[3].emotion, EmotionalTag::Terrified);

        let speakers = script.speakers();
        assert_eq!(speakers, vec!["ELENA", "MARCUS", "NARRATOR", "SOLDIER"]);
    }
}
