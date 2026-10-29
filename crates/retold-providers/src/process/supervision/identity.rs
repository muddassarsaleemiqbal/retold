//! Native boot and process-start identity; PIDs alone are never execution identity.
use std::io;

use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "linux", test))]
#[path = "identity/proc_stat.rs"]
mod proc_stat;

#[cfg(target_os = "macos")]
#[path = "identity/darwin.rs"]
mod native;
#[cfg(target_os = "linux")]
#[path = "identity/linux.rs"]
mod native;
#[cfg(windows)]
#[path = "identity/windows.rs"]
mod native;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub pid: u32,
    pub group: u32,
    pub boot: String,
    pub start: String,
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub fn read(pid: u32) -> io::Result<Identity> {
    native::read(pid)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn read(_pid: u32) -> io::Result<Identity> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Native process identity is unavailable.",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    Matching,
    Different,
    Absent,
    Unknown,
}

/// Observation is diagnostic evidence, never a signal, release or retry authorization.
#[must_use]
pub fn observe(saved: &Identity) -> Observation {
    match read(saved.pid) {
        Ok(current) if current == *saved => Observation::Matching,
        Ok(_) => Observation::Different,
        Err(_) => absence(saved.pid),
    }
}

#[cfg(windows)]
fn absence(pid: u32) -> Observation {
    native::absence(pid)
}

#[cfg(unix)]
fn absence(pid: u32) -> Observation {
    use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
    let Ok(pid) = i32::try_from(pid) else {
        return Observation::Unknown;
    };
    match kill(
        Pid::from_raw(pid),
        Option::<nix::sys::signal::Signal>::default(),
    ) {
        Err(Errno::ESRCH) => Observation::Absent,
        _ => Observation::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_identity_is_stable_and_contains_boot_and_start_evidence() {
        let identity = read(std::process::id()).unwrap();
        assert_eq!(identity, read(std::process::id()).unwrap());
        assert_ne!(identity.boot, "");
        assert_ne!(identity.start, "");
        assert_ne!(identity.group, 0);
        assert_eq!(observe(&identity), Observation::Matching);
        let mut previous = identity.clone();
        previous.start.push_str("-different");
        assert_eq!(observe(&previous), Observation::Different);
        previous = identity;
        previous.boot.push_str("-different");
        assert_eq!(observe(&previous), Observation::Different);
    }
}
