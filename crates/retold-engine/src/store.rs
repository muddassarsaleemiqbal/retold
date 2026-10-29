//! Exclusive run ownership and content-addressed, integrity-checked stage results.
//! Cache envelopes are committed before the state index. Resume repairs the index from
//! the envelopes, including calls completed immediately before a crash.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use retold_core::Usd;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{EngineError, RunLayout, fsx};

pub mod inspection;
mod serialization;

const SCHEMA: u32 = 1;
const STATE_BACKUP: &str = "state.json.bak";
const DAMAGED_STATE_PREFIX: &str = "state.json.damaged-";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: Usd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub key: String,
    pub output_hash: String,
    pub receipt: Option<Receipt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u32,
    pub active: Option<String>,
    #[serde(default)]
    pub active_tasks: std::collections::BTreeSet<String>,
    pub error: Option<String>,
    pub completed: BTreeMap<String, Record>,
    #[serde(default)]
    pub receipts: BTreeMap<String, Receipt>,
    #[serde(default)]
    pub revisions: BTreeMap<String, u64>,
}

impl State {
    /// The same scope rules serve read-only impact inspection and active generation.
    #[must_use]
    pub fn revision(&self, step: &str) -> u64 {
        self.revisions
            .iter()
            .filter(|(scope, _)| {
                step == scope.as_str()
                    || step
                        .strip_prefix(scope.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
                    || stage_of(step) == scope.as_str()
            })
            .map(|(_, revision)| *revision)
            .fold(0, u64::saturating_add)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    step: String,
    record: Record,
    value: serde_json::Value,
}

/// OS locks release on exit or kill. The lock file stays in place so a second process
/// cannot lock a replacement inode while the first still owns the original.
#[derive(Debug)]
pub struct Store {
    layout: RunLayout,
    lock: File,
    pub state: State,
    events: Option<std::sync::Arc<crate::events::Events>>,
    started: BTreeMap<String, std::time::Instant>,
    /// Set when the state index was unreadable and the previous copy was used instead.
    pub restored: Option<String>,
}

impl Drop for Store {
    fn drop(&mut self) {
        // A child spawned while this store was open may still hold a duplicate.
        let _ = self.lock.unlock();
    }
}

fn cache_path(layout: &RunLayout, step: &str, key: &str) -> Result<PathBuf, EngineError> {
    if step.is_empty()
        || !step
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || key.len() != 64
        || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(EngineError::Setup("invalid cache identifier".into()));
    }
    let lower = step.to_ascii_lowercase();
    Ok(layout
        .meta()
        .join("cache")
        .join(format!("{lower}-{key}.json")))
}

impl Store {
    pub fn open(layout: &RunLayout) -> Result<Self, EngineError> {
        open_store_at(layout)
    }

    #[must_use]
    pub fn revision(&self, step: &str) -> u64 {
        self.state.revision(step)
    }
    pub fn bump_revision(&mut self, scope: &str) -> Result<(), EngineError> {
        let revision = self.state.revisions.entry(scope.into()).or_default();
        *revision = revision.saturating_add(1);
        let affected = crate::graph::Graph::stages().affected(stage_of(scope));
        self.state
            .completed
            .retain(|step, _| !affected.contains(stage_of(step)));
        self.save()
    }

    /// Manual narration changes affect both storyboard and speech, while retaining writer caches.
    pub(crate) fn invalidate_script_descendants(mut self) -> Result<Self, EngineError> {
        let revisions = &mut self.state.revisions;
        for scope in ["storyboard", "voice"] {
            let revision = revisions.entry(scope.into()).or_default();
            *revision = revision.saturating_add(1);
        }
        let mut affected = crate::graph::Graph::stages().affected("script");
        affected.remove("script");
        self.state
            .completed
            .retain(|step, _| !affected.contains(stage_of(step)));
        self.save()?;
        Ok(self)
    }

    pub fn set_events(&mut self, events: std::sync::Arc<crate::events::Events>) {
        self.events = Some(events);
    }

    pub(crate) fn branch_failed(
        &self,
        stage: &str,
        edition: Option<&str>,
    ) -> Result<(), EngineError> {
        if let Some(events) = &self.events {
            events.emit(&crate::events::Event::branch_failed(stage, edition))?;
        }
        Ok(())
    }

    pub(crate) fn task_stopped(&mut self, step: &str, message: &str) -> Result<(), EngineError> {
        self.state.active_tasks.remove(step);
        self.state.active = self.state.active_tasks.iter().next().cloned();
        self.emit(
            step,
            crate::events::Status::Stopped,
            None,
            Some(message.into()),
        )?;
        self.save()
    }
    pub(crate) fn emit(
        &self,
        step: &str,
        status: crate::events::Status,
        elapsed: Option<f64>,
        message: Option<String>,
    ) -> Result<(), EngineError> {
        if let Some(events) = &self.events {
            events.emit(&crate::events::Event::new(step, status, elapsed, message))?;
        }
        Ok(())
    }

    fn cache_file(&self, step: &str, key: &str) -> Result<PathBuf, EngineError> {
        cache_path(&self.layout, step, key)
    }

    pub fn begin(&mut self, step: &str) -> Result<(), EngineError> {
        let stage = stage_of(step);
        let mut affected = crate::graph::Graph::stages().affected(stage);
        affected.remove(stage);
        self.state.completed.remove(step);
        self.state.completed.remove(stage);
        self.state
            .completed
            .retain(|step, _| !affected.contains(stage_of(step)));
        self.started.insert(step.into(), std::time::Instant::now());
        self.emit(step, crate::events::Status::Running, None, None)?;
        self.state.active_tasks.insert(step.into());
        self.state.active = Some(step.into());
        self.state.error = None;
        self.save()
    }

    pub fn fail(&mut self, message: &str) -> Result<(), EngineError> {
        self.emit(
            self.state.active.as_deref().unwrap_or("run"),
            crate::events::Status::Stopped,
            None,
            Some(message.into()),
        )?;
        self.state.error = Some(message.into());
        self.save()
    }

    pub fn finish(&mut self) -> Result<(), EngineError> {
        self.state.active_tasks.clear();
        self.state.active = None;
        self.state.error = None;
        self.save()
    }

    /// Results are immutable and keyed by all inputs. Corrupt entries fail closed, rather
    /// than silently making another provider call.
    pub fn cached<T: DeserializeOwned>(
        &mut self,
        step: &str,
        key: &str,
    ) -> Result<Option<T>, EngineError> {
        let path = self.cache_file(step, key)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(EngineError::io(&path, e)),
        };
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
                path: path.clone(),
                message: e.to_string(),
            })?;
        let same_step = envelope.step.eq_ignore_ascii_case(step);
        let valid = envelope.schema == SCHEMA
            && same_step
            && envelope.record.key == key
            && hash(&envelope.value)? == envelope.record.output_hash;
        if !valid {
            return Err(EngineError::Config {
                path,
                message:
                    "cache integrity check failed; restore the original result before resuming"
                        .into(),
            });
        }
        let value = serde_json::from_value(envelope.value).map_err(|e| EngineError::Config {
            path,
            message: e.to_string(),
        })?;
        if let Some(receipt) = &envelope.record.receipt {
            self.state
                .receipts
                .insert(format!("{step}-{key}"), receipt.clone());
        }
        self.state.completed.insert(step.into(), envelope.record);
        self.save()?;
        self.emit(step, crate::events::Status::Cached, None, None)?;
        Ok(Some(value))
    }

    pub fn commit<T: Serialize>(
        &mut self,
        step: &str,
        key: &str,
        value: &T,
        receipt: Option<Receipt>,
    ) -> Result<(), EngineError> {
        let value = serde_json::to_value(value).map_err(|e| EngineError::Setup(e.to_string()))?;
        let record = Record {
            key: key.into(),
            output_hash: hash(&value)?,
            receipt,
        };
        let envelope = Envelope {
            schema: SCHEMA,
            step: step.into(),
            record: record.clone(),
            value,
        };
        fsx::write_json_atomic(&self.cache_file(step, key)?, &envelope)?;
        if let Some(receipt) = &record.receipt {
            self.state
                .receipts
                .insert(format!("{step}-{key}"), receipt.clone());
        }
        self.state.completed.insert(step.into(), record);
        let elapsed = self
            .started
            .remove(step)
            .map(|started| started.elapsed().as_secs_f64());
        self.emit(step, crate::events::Status::Complete, elapsed, None)?;
        self.state.active_tasks.remove(step);
        self.state.active = self.state.active_tasks.iter().next().cloned();
        self.save()
    }

    fn save(&self) -> Result<(), EngineError> {
        let state_file = self.layout.state_file();
        // The last good index is kept beside the live one; see `read_state_or_backup`.
        match std::fs::copy(&state_file, backup_file(&self.layout)) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(EngineError::io(&state_file, e)),
        }
        fsx::write_json_atomic(&state_file, &self.state)?;
        let receipts: Vec<_> = self.state.receipts.values().collect();
        let total: Usd = receipts.iter().map(|r| r.cost).sum();
        fsx::write_json_atomic(
            &self.layout.cost_file(),
            &serde_json::json!({ "total": total, "calls": receipts }),
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Holder {
    pid: u32,
    started: u64,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes who owns the lock into the (still locked) file, purely so a second process can say
/// who it is waiting for. The OS lock, not this text, is what grants ownership.
fn record_holder(lock: &File, path: &std::path::Path) -> Result<(), EngineError> {
    use std::io::{Seek, Write};
    let holder = Holder {
        pid: std::process::id(),
        started: unix_now(),
    };
    let bytes = serde_json::to_vec(&holder).map_err(|e| EngineError::Setup(e.to_string()))?;
    let mut file = lock;
    file.set_len(0)
        .and_then(|()| file.rewind())
        .and_then(|()| file.write_all(&bytes))
        .map_err(|e| EngineError::io(path, e))
}

fn describe_holder(path: &std::path::Path) -> Option<String> {
    let holder: Holder = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let minutes = unix_now().saturating_sub(holder.started) / 60;
    Some(match minutes {
        0 => format!("pid {}, started just now", holder.pid),
        m => format!("pid {}, running for {m} min", holder.pid),
    })
}

fn backup_file(layout: &RunLayout) -> PathBuf {
    layout.meta().join(STATE_BACKUP)
}

/// Engine-owned recovery evidence, retained in portable backups as ordinary private data.
#[must_use]
pub fn is_recovery_filename(name: &str) -> bool {
    name == STATE_BACKUP
        || name
            .strip_prefix(DAMAGED_STATE_PREFIX)
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
}

/// Reads the state index. If it does not parse (disk corruption, a hand edit), the copy taken
/// before the last save is used and the damaged file is kept beside it for inspection. Schema
/// mismatches are never papered over: they mean a different `Retold` version wrote the run.
fn read_state_or_backup(layout: &RunLayout) -> Result<(State, Option<String>), EngineError> {
    let error = match read_state(layout) {
        Ok(state) => return Ok((state, None)),
        Err(EngineError::Config { path, message }) if !message.contains("schema") => {
            EngineError::Config { path, message }
        }
        Err(error) => return Err(error),
    };
    let backup = backup_file(layout);
    let Ok(bytes) = std::fs::read(&backup) else {
        return Err(error);
    };
    let Ok(state) = serde_json::from_slice::<State>(&bytes) else {
        return Err(error);
    };
    if state.schema != SCHEMA {
        return Err(error);
    }
    let damaged = layout
        .meta()
        .join(format!("{DAMAGED_STATE_PREFIX}{}", unix_now()));
    std::fs::rename(layout.state_file(), &damaged).map_err(|e| EngineError::io(&damaged, e))?;
    fsx::write_json_atomic(&layout.state_file(), &state)?;
    Ok((
        state,
        Some(format!(
            "the run index was damaged ({error}); restored the previous copy and kept the damaged file as {}",
            damaged.display()
        )),
    ))
}

pub fn read_state(layout: &RunLayout) -> Result<State, EngineError> {
    let path = layout.state_file();
    match std::fs::read(&path) {
        Ok(bytes) => {
            let state: State = serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
                path: path.clone(),
                message: e.to_string(),
            })?;
            if state.schema != SCHEMA {
                return Err(EngineError::Config {
                    path,
                    message: "unsupported state schema; update Retold".into(),
                });
            }
            Ok(state)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State {
            schema: SCHEMA,
            ..State::default()
        }),
        Err(e) => Err(EngineError::io(&path, e)),
    }
}

pub fn hash<T: Serialize>(value: &T) -> Result<String, EngineError> {
    let bytes = serialization::bytes(value).map_err(|e| EngineError::Setup(e.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

pub(crate) fn strip_part_scope(step: &str) -> &str {
    let Some((prefix, rest)) = step.split_once('-') else {
        return step;
    };
    if is_part_id(prefix) { rest } else { step }
}

pub(crate) fn is_part_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    matches!(bytes.next(), Some(b'p' | b'P'))
        && bytes.len() > 0
        && bytes.all(|byte| byte.is_ascii_digit())
}

const STAGE_PREFIXES: &[(&str, &[&str])] = &[
    (
        "research",
        &["research", "web", "extract", "dossier", "bible", "canon"],
    ),
    ("storyboard", &["storyboard", "direct"]),
    ("characters", &["characters", "design", "sheet", "style"]),
    (
        "images",
        &["image", "panels", "place", "prop", "qa", "focus", "cover"],
    ),
    (
        "voices",
        &[
            "cast-",
            "speaker-embedding",
            "speech-translit",
            "reference-transcript",
            "repair-voices",
        ],
    ),
    ("voice", &["voice", "transcript", "align", "transcreate"]),
    (
        "captions",
        &[
            "caption",
            "translate",
            "transliterate",
            "sidecar",
            "package",
        ],
    ),
    ("mouths", &["mouth-", "phonemes"]),
    (
        "edit",
        &[
            "timeline",
            "edit",
            "build",
            "export",
            "thumbnail",
            "loudness",
        ],
    ),
];

#[must_use]
pub fn stage_of(step: &str) -> &str {
    let step = strip_part_scope(step);
    for stage in ["depth", "shots", "voices", "mouths", "deliver", "metrics"] {
        if step == stage || step.strip_prefix(stage).is_some_and(|s| s.starts_with('-')) {
            return stage;
        }
    }
    for (group, prefixes) in STAGE_PREFIXES {
        if prefixes.iter().any(|prefix| step.starts_with(prefix)) {
            return group;
        }
    }
    "script"
}

fn open_store_at(layout: &RunLayout) -> Result<Store, EngineError> {
    crate::recovery::ensure_active(layout.video_dir())?;
    let path = layout.lock_file();
    std::fs::create_dir_all(layout.meta()).map_err(|e| EngineError::io(&path, e))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| EngineError::io(&path, e))?;
    lock.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => EngineError::Locked {
            path: layout.video_dir().to_owned(),
            holder: describe_holder(&path),
        },
        std::fs::TryLockError::Error(e) => EngineError::io(&path, e),
    })?;
    crate::recovery::ensure_active(layout.video_dir())?;
    record_holder(&lock, &path)?;
    let (mut state, restored) = read_state_or_backup(layout)?;
    let cache = layout.meta().join("cache");
    match std::fs::read_dir(&cache) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(|e| EngineError::io(&cache, e))?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    let bytes = std::fs::read(&path).map_err(|e| EngineError::io(&path, e))?;
                    let envelope: Envelope =
                        serde_json::from_slice(&bytes).map_err(|e| EngineError::Config {
                            path: path.clone(),
                            message: e.to_string(),
                        })?;
                    if envelope.schema != SCHEMA
                        || hash(&envelope.value)? != envelope.record.output_hash
                    {
                        return Err(EngineError::Config {
                            path,
                            message: "cache integrity check failed".into(),
                        });
                    }
                    if let Some(receipt) = envelope.record.receipt {
                        state.receipts.insert(
                            format!("{}-{}", envelope.step, envelope.record.key),
                            receipt,
                        );
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(EngineError::io(&cache, e)),
    }
    Ok(Store {
        layout: layout.clone(),
        lock,
        state,
        events: None,
        started: BTreeMap::new(),
        restored,
    })
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    #[test]
    fn store_drop_releases_inherited_descriptors_but_live_ownership_blocks_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let layout = RunLayout::from_video_dir(
            directory.path(),
            "channel",
            &directory.path().join("channel/films/fixture"),
            2,
        )
        .unwrap();
        let store = Store::open(&layout).unwrap();
        let _inherited = store.lock.try_clone().unwrap();
        assert!(matches!(
            Store::open(&layout),
            Err(EngineError::Locked { .. })
        ));
        drop(store);
        assert!(Store::open(&layout).is_ok());
    }
}

#[cfg(test)]
mod numeric_cache_tests {
    use super::*;
    #[test]
    fn precise_timeline_numbers_survive_cache_reopen_without_hash_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = RunLayout::from_video_dir(
            tmp.path(),
            "channel",
            &tmp.path().join("channel/films/fixture"),
            2,
        )
        .unwrap();
        let values: Vec<f64> = vec![
            1.0 / 30.0,
            113.510_666_666_666_68,
            0.266_666_666_666_668_7,
            0.989_824_565_569_851_9,
            1.000_000_000_000_000_2,
        ];
        let key = hash(&values).unwrap();
        let mut store = Store::open(&layout).unwrap();
        store.commit("timeline", &key, &values, None).unwrap();
        drop(store);
        let mut reopened = Store::open(&layout).unwrap();
        let restored: Vec<f64> = reopened.cached("timeline", &key).unwrap().unwrap();
        assert!(
            restored
                .iter()
                .zip(&values)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        assert_eq!(hash(&values).unwrap(), hash(&restored).unwrap());
    }

    #[test]
    fn stage_classification_keeps_later_steps_from_evicting_completed_stages() {
        for (step, expected) in [
            ("research", "research"),
            ("web-0", "research"),
            ("extract-s1-1", "research"),
            ("dossier", "research"),
            ("bible-fix-0", "research"),
            ("canon-parts", "research"),
            ("p01-dossier", "research"),
            ("p01-extract-s1-1", "research"),
            ("plan", "script"),
            ("draft-0", "script"),
            ("storyboard", "storyboard"),
            ("direct-fix-0", "storyboard"),
            ("characters", "characters"),
            ("sheet-check-vera-0", "characters"),
            ("place-castle", "images"),
            ("prop-lantern", "images"),
            ("panels-0-1", "images"),
            ("image-0-0", "images"),
            ("voice-en", "voice"),
            ("transcreate-hi", "voice"),
            ("cast-voice-abc", "voices"),
            ("speech-translit-ur-0", "voices"),
            ("repair-voices-0-ur-r0-c0", "voices"),
            ("package-en", "captions"),
            ("transliterate-ur-ur-latn", "captions"),
            ("mouth-sheet-vera-0", "mouths"),
            ("phonemes-en-0-abc", "mouths"),
            ("timeline", "edit"),
            ("loudness-en", "edit"),
            ("export-en", "edit"),
            ("deliver-package-en", "deliver"),
        ] {
            assert_eq!(stage_of(step), expected, "step `{step}`");
        }
    }
}
