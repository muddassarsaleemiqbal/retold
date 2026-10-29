//! Windows process identity: the boot moment and the process start time, read through sysinfo.
use std::io;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{Identity, Observation};

fn start_time_of(pid: u32) -> Option<u64> {
    let mut system = System::new();
    let target = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[target]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system.process(target).map(sysinfo::Process::start_time)
}

pub(super) fn read(pid: u32) -> io::Result<Identity> {
    let start = start_time_of(pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Process is not running."))?;
    Ok(Identity {
        pid,
        group: pid,
        boot: System::boot_time().to_string(),
        start: start.to_string(),
    })
}

pub(super) fn absence(pid: u32) -> Observation {
    if start_time_of(pid).is_none() {
        Observation::Absent
    } else {
        Observation::Unknown
    }
}
