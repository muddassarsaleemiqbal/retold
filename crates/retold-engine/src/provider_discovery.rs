//! Offline inventory only: no subprocesses, login-file reads or provider construction.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::Serialize;

const MAX_PATH_ENTRIES: usize = 256;
const MAX_HOME_ENTRIES: usize = 4096;
const TOOL_NAMES: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "llama-completion",
    "filmcraft",
    "ffmpeg",
];
const KEY_VARIABLES: &[&str] = &[
    "OPENROUTER_API_KEY",
    "ELEVENLABS_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
];

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub tools: Vec<Tool>,
    pub profiles: Vec<PathBuf>,
    pub credential_variables: Vec<CredentialVariable>,
    pub authentication: &'static str,
    pub versions: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct CredentialVariable {
    pub name: &'static str,
    pub present: bool,
}

/// Explicit inputs keep filesystem discovery deterministic and independent of provider settings.
pub fn scan(home: &Path, search_path: &OsStr) -> Result<Inventory, String> {
    let directories: Vec<_> = std::env::split_paths(search_path).collect();
    if directories.len() > MAX_PATH_ENTRIES {
        return Err("PATH exceeds the offline scan limit".into());
    }
    let tools = TOOL_NAMES
        .iter()
        .map(|name| Tool {
            name,
            path: find_tool(name, &directories),
        })
        .collect();
    Ok(Inventory {
        tools,
        profiles: profiles(home)?,
        credential_variables: credential_variables(),
        authentication: "not_checked",
        versions: "not_checked",
    })
}

fn find_tool(name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    directories
        .iter()
        .map(|directory| directory.join(name))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && has_executable_mode(&metadata))
}

fn has_executable_mode(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        const EXECUTE_BITS: u32 = 0o111;
        metadata.permissions().mode() & EXECUTE_BITS != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}

fn profiles(home: &Path) -> Result<Vec<PathBuf>, String> {
    let entries =
        std::fs::read_dir(home).map_err(|_| "Could not inspect profile folders".to_owned())?;
    let mut found = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_HOME_ENTRIES {
            return Err("Home folder exceeds the offline scan limit".into());
        }
        let entry = entry.map_err(|_| "Could not inspect profile folders".to_owned())?;
        if let Some(path) = profile_path(&entry) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

fn credential_variables() -> Vec<CredentialVariable> {
    KEY_VARIABLES
        .iter()
        .map(|name| CredentialVariable {
            name,
            present: std::env::var_os(name).is_some_and(|value| !value.is_empty()),
        })
        .collect()
}

fn profile_path(entry: &std::fs::DirEntry) -> Option<PathBuf> {
    let name = entry.file_name();
    let name = name.to_string_lossy();
    let selected = name.starts_with(".claude") || name.starts_with(".codex");
    let path = entry.path();
    (selected && path.is_dir()).then_some(path)
}
