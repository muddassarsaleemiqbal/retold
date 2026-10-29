//! A private, deterministic directory alias keeps Unix socket addresses below platform limits.
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

use super::{StudioError, server::SOCKET_NAME};

const DIRECT_LIMIT: usize = 100;
const HASH_CHARACTERS: usize = 24;
const PRIVATE_MODE: u32 = 0o700;
const PERMISSION_MASK: u32 = 0o777;

struct Endpoint {
    state: PathBuf,
    alias_directory: Option<PathBuf>,
    owner: u32,
}

fn resolve_endpoint(state: &Path) -> Result<Endpoint, StudioError> {
    let state = state.canonicalize()?;
    let owner = state.metadata()?.uid();
    let alias_directory = short_directory(&state, owner);
    Ok(Endpoint {
        state,
        alias_directory,
        owner,
    })
}

fn short_directory(state: &Path, owner: u32) -> Option<PathBuf> {
    let socket = state.join(SOCKET_NAME);
    let long_address = address_exceeds_limit(&socket);
    long_address.then(|| Path::new("/tmp").join(format!("rg-ipc-{owner}-{}", state_hash(state))))
}

fn address_exceeds_limit(socket: &Path) -> bool {
    let address = socket.as_os_str().as_bytes();
    address.len() >= DIRECT_LIMIT
}

fn state_hash(state: &Path) -> String {
    let hash = blake3::hash(state.as_os_str().as_bytes()).to_hex();
    hash[..HASH_CHARACTERS].into()
}

impl Endpoint {
    fn socket(&self) -> PathBuf {
        self.alias_directory.as_ref().map_or_else(
            || self.state.join(SOCKET_NAME),
            |directory| directory.join("workspace").join(SOCKET_NAME),
        )
    }

    fn validate_directory(&self, directory: &Path) -> Result<(), StudioError> {
        let metadata = std::fs::symlink_metadata(directory)?;
        let private = metadata.permissions().mode() & PERMISSION_MASK == PRIVATE_MODE;
        let owned = metadata.uid() == self.owner;
        let regular = metadata.is_dir() && !metadata.file_type().is_symlink();
        if !regular || !private || !owned {
            return Err(invalid_alias());
        }
        Ok(())
    }

    fn validate_link(&self, directory: &Path) -> Result<(), StudioError> {
        self.validate_directory(directory)?;
        let link = directory.join("workspace");
        let metadata = std::fs::symlink_metadata(&link)?;
        let target = std::fs::read_link(&link)?;
        let valid = metadata.file_type().is_symlink() && target == self.state;
        if !valid {
            return Err(invalid_alias());
        }
        Ok(())
    }
}

pub(super) fn prepare(state: &Path) -> Result<PathBuf, StudioError> {
    let endpoint = resolve_endpoint(state)?;
    if let Some(directory) = &endpoint.alias_directory {
        create_private_directory(directory)?;
        endpoint.validate_directory(directory)?;
        let link = directory.join("workspace");
        match symlink(&endpoint.state, &link) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        endpoint.validate_link(directory)?;
    }
    Ok(endpoint.socket())
}

pub(super) fn address(state: &Path) -> Result<PathBuf, StudioError> {
    let endpoint = resolve_endpoint(state)?;
    if let Some(directory) = &endpoint.alias_directory {
        endpoint.validate_link(directory)?;
    }
    Ok(endpoint.socket())
}

pub(super) fn cleanup(state: &Path) {
    let Ok(endpoint) = resolve_endpoint(state) else {
        return;
    };
    if let Some(directory) = &endpoint.alias_directory {
        let valid = endpoint.validate_link(directory).is_ok();
        if valid {
            let _ = std::fs::remove_file(directory.join("workspace"));
            let _ = std::fs::remove_dir(directory);
        }
    }
}

fn create_private_directory(directory: &Path) -> Result<(), StudioError> {
    match std::fs::DirBuilder::new()
        .mode(PRIVATE_MODE)
        .create(directory)
    {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn invalid_alias() -> StudioError {
    StudioError::Protocol("The private studio socket alias is invalid or belongs to another owner. It was preserved; inspect the IPC path before retrying.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_state() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&"state".repeat(HASH_CHARACTERS))
            .tempdir_in("/tmp")
            .unwrap()
    }

    #[test]
    fn observation_never_creates_aliases_and_preparation_cleans_up() {
        let state = long_state();
        let endpoint = resolve_endpoint(state.path()).unwrap();
        let directory = endpoint.alias_directory.as_ref().unwrap();
        assert!(address(state.path()).is_err());
        assert!(!directory.exists());
        let socket = prepare(state.path()).unwrap();
        assert!(socket.as_os_str().as_bytes().len() < DIRECT_LIMIT);
        assert_eq!(address(state.path()).unwrap(), socket);
        cleanup(state.path());
        assert!(!directory.exists());
    }

    #[test]
    fn foreign_alias_files_are_preserved() {
        let state = long_state();
        let endpoint = resolve_endpoint(state.path()).unwrap();
        let directory = endpoint.alias_directory.as_ref().unwrap();
        std::fs::write(directory, b"preserve me").unwrap();
        assert!(prepare(state.path()).is_err());
        assert!(address(state.path()).is_err());
        cleanup(state.path());
        assert_eq!(std::fs::read(directory).unwrap(), b"preserve me");
        std::fs::remove_file(directory).unwrap();
    }
}
