//! Packaged display-free worker entry point, selected before any GPUI initialization.
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn run(library: &Path) -> anyhow::Result<()> {
    let library = library.canonicalize()?;
    let state = retold_app::desktop::queue_state(&library)
        .map_err(retold_app::studio::StudioError::from)?;
    let interrupt = Interrupt::new()?;
    let server = {
        let _startup = retold_app::studio::hold_startup(Some(&library), &state)
            .map_err(retold_app::studio::StudioError::from)?;
        retold_app::studio::StudioServer::bind(&state)?
    };
    server.run_until(&interrupt.flag)?;
    Ok(())
}

#[derive(Debug)]
struct Interrupt {
    flag: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl Interrupt {
    fn new() -> std::io::Result<Self> {
        let mut guard = Self {
            flag: Arc::default(),
            registrations: Vec::new(),
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            guard.registrations.push(signal_hook::flag::register(
                signal,
                Arc::clone(&guard.flag),
            )?);
        }
        Ok(guard)
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        for registration in &self.registrations {
            signal_hook::low_level::unregister(*registration);
        }
    }
}
