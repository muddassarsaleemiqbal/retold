//! Darwin exposes the kernel process start time and boot-session UUID through native APIs.
use super::Identity;
use std::io;
use sysctl::Sysctl;

pub(super) fn read(pid: u32) -> io::Result<Identity> {
    let native_pid = i32::try_from(pid).map_err(io::Error::other)?;
    let process = libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(native_pid, 0)
        .map_err(io::Error::other)?;
    let boot = sysctl::Ctl::new("kern.bootsessionuuid")
        .and_then(|control| control.value_string())
        .map_err(io::Error::other)?;
    if boot.is_empty() || process.pbi_pid != pid || process.pbi_start_tvsec == 0 {
        return Err(io::Error::other("Native process identity is incomplete."));
    }
    Ok(Identity {
        pid,
        group: process.pbi_pgid,
        boot,
        start: format!(
            "darwin:{}:{}",
            process.pbi_start_tvsec, process.pbi_start_tvusec
        ),
    })
}
