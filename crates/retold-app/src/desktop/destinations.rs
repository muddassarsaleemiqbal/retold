//! Copy a delivered file to a chosen folder, never replacing a file without explicit consent,
//! and keep a per-video delivery history whose entries can be re-validated later.
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::desktop::media::Item;
use crate::protocol::{Diagnostic, ErrorCode};

const HISTORY_FILE: &str = "deliveries.jsonl";
/// History is read with a bound; older entries beyond it are not shown.
const HISTORY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overwrite {
    Refuse,
    Replace,
}

/// One copy made from this video to a destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delivered {
    pub source: String,
    pub destination: PathBuf,
    pub bytes: u64,
    pub blake3: String,
    pub delivered_at: String,
}

/// A history entry with whether the destination file still matches what was delivered.
#[derive(Debug, Clone)]
pub struct Checked {
    pub delivered: Delivered,
    pub intact: bool,
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Delivery: {error}"))
}

fn digest(path: &Path) -> Result<(u64, String), Diagnostic> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path).map_err(invalid)?;
    let bytes = std::io::copy(&mut file, &mut hasher).map_err(invalid)?;
    Ok((bytes, hasher.finalize().to_hex().to_string()))
}

fn history_path(root: &Path, reference: &str) -> Result<PathBuf, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    let video = catalog.resolve(reference)?;
    Ok(video.layout.meta().join(HISTORY_FILE))
}

/// Where a copy of `item` would land in `folder`, and whether a file already exists there.
#[must_use]
pub fn target(item: &Item, folder: &Path) -> (PathBuf, bool) {
    let name = Path::new(&item.relative)
        .file_name()
        .map_or_else(|| "delivery".into(), ToOwned::to_owned);
    let target = folder.join(name);
    let exists = target.symlink_metadata().is_ok();
    (target, exists)
}

fn copy_verified(source: &Path, target: &Path, expected: &(u64, String)) -> Result<(), Diagnostic> {
    let folder = target
        .parent()
        .ok_or_else(|| invalid("destination has no folder"))?;
    let staging = folder.join(format!(".retold-delivery-{}", std::process::id()));
    std::fs::copy(source, &staging).map_err(invalid)?;
    let copied = digest(&staging)?;
    if copied != *expected {
        let _ = std::fs::remove_file(&staging);
        return Err(invalid("the copy does not match the delivered file"));
    }
    std::fs::rename(&staging, target).map_err(invalid)
}

fn record(root: &Path, reference: &str, delivered: &Delivered) -> Result<(), Diagnostic> {
    let path = history_path(root, reference)?;
    let mut line = serde_json::to_vec(delivered).map_err(invalid)?;
    line.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(invalid)?;
    file.write_all(&line).map_err(invalid)
}

/// Copy a validated delivery into `folder`; an existing file is replaced only with `Replace`.
pub fn deliver(
    root: &Path,
    reference: &str,
    (item, folder, overwrite): (&Item, &Path, Overwrite),
) -> Result<Delivered, Diagnostic> {
    let source = item.validated_path()?;
    let (destination, exists) = target(item, folder);
    let refused = exists && overwrite == Overwrite::Refuse;
    if refused {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "A file with this name already exists there. Confirm replacing it or choose another folder.",
        ));
    }
    let expected = digest(&source)?;
    item.validated_path()?;
    copy_verified(&source, &destination, &expected)?;
    let delivered = Delivered {
        source: item.relative.clone(),
        destination,
        bytes: expected.0,
        blake3: expected.1,
        delivered_at: jiff::Timestamp::now().to_string(),
    };
    record(root, reference, &delivered)?;
    Ok(delivered)
}

/// Delivery history, newest first, each entry re-hashed against its destination.
pub fn history(root: &Path, reference: &str) -> Result<Vec<Checked>, Diagnostic> {
    let path = history_path(root, reference)?;
    let Some(bytes) = retold_engine::fsx::read_bounded(&path, HISTORY_BYTES).map_err(invalid)?
    else {
        return Ok(Vec::new());
    };
    let mut entries: Vec<Checked> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<Delivered>(line).ok())
        .map(|delivered| {
            let intact = digest(&delivered.destination)
                .is_ok_and(|found| found == (delivered.bytes, delivered.blake3.clone()));
            Checked { delivered, intact }
        })
        .collect();
    entries.reverse();
    Ok(entries)
}
