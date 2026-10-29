//! Observe terminal state without reaping; keep the leader PID pinned until group signals finish.
use std::io;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn exited(pid: u32) -> io::Result<bool> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let raw = i32::try_from(pid).map_err(io::Error::other)?;
    let identity = Pid::from_raw(raw).ok_or_else(|| io::Error::other("Invalid worker PID."))?;
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    match waitid(WaitId::Pid(identity), options) {
        Ok(status) => Ok(status.is_some()),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn exited(_pid: u32) -> io::Result<bool> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Non-reaping child observation is unavailable.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_observation_keeps_the_direct_child_waitable() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !exited(child.id()).unwrap() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // A second WNOWAIT observes the same terminal child, then std::Child still reaps it.
        assert!(exited(child.id()).unwrap());
        assert_eq!(child.wait().unwrap().code(), Some(7));
    }
}
