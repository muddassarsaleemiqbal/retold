//! Procfs start ticks are interpreted within the kernel's boot UUID.
use super::Identity;
use std::io;

const MAX_STAT_BYTES: u64 = 4096;
const MAX_BOOT_BYTES: u64 = 128;

pub(super) fn read(pid: u32) -> io::Result<Identity> {
    let stat_record = bounded(&format!("/proc/{pid}/stat"), MAX_STAT_BYTES)?;
    let boot = bounded("/proc/sys/kernel/random/boot_id", MAX_BOOT_BYTES)?;
    let (group, start) = super::proc_stat::parse(&stat_record)?;
    Ok(Identity {
        pid,
        group,
        boot: boot.trim().into(),
        start: format!("linux:{start}"),
    })
}

fn bounded(path: &str, limit: u64) -> io::Result<String> {
    use std::io::Read;
    let mut bytes = String::new();
    let mut file = std::fs::File::open(path)?.take(limit.saturating_add(1));
    file.read_to_string(&mut bytes)?;
    validate(bytes, limit)
}

fn validate(bytes: String, limit: u64) -> io::Result<String> {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let empty = bytes.trim().is_empty();
    if length > limit || empty {
        return Err(io::Error::other(
            "Native identity exceeds its bound or is empty.",
        ));
    }
    Ok(bytes)
}
