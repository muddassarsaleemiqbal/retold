//! Platform seams for the studio: local sockets and owner-only permissions.
//! Unix uses `std` sockets and 0700/0600 modes; Windows uses `AF_UNIX` sockets (Windows 10+)
//! and relies on the per-user profile directory for privacy.
use std::fs::OpenOptions;
use std::path::Path;

#[cfg(unix)]
pub(super) use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
pub(super) use uds_windows::{UnixListener, UnixStream};

/// Restrict a directory or socket to its owner where the platform has modes.
pub(super) fn make_private(path: &Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Read-write options that create a file without truncating it, owner-only where the
/// platform has modes.
pub(super) fn owner_only_options(mode: u32) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options.create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options
}

/// Whether an existing path is a socket that may be replaced.
pub(super) fn is_socket(file_type: std::fs::FileType) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt as _;
        file_type.is_socket()
    }
    #[cfg(not(unix))]
    {
        !file_type.is_dir() && !file_type.is_file()
    }
}
