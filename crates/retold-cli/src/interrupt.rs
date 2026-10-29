//! Scoped process signals; observers detach while studio owners drain their workers.
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub struct Interrupt {
    requested: Arc<AtomicBool>,
    registrations: Vec<signal_hook::SigId>,
}

impl Interrupt {
    pub fn new() -> io::Result<Self> {
        let guard = Self {
            requested: Arc::default(),
            registrations: Vec::new(),
        };
        guard
            .register(signal_hook::consts::SIGINT)?
            .register(signal_hook::consts::SIGTERM)
    }

    /// Session owners also observe terminal hangup, alongside interrupt and termination.
    pub fn session() -> io::Result<Self> {
        #[cfg(unix)]
        {
            Self::new()?.register(signal_hook::consts::SIGHUP)
        }
        #[cfg(not(unix))]
        {
            Self::new()
        }
    }

    fn register(mut self, signal: i32) -> io::Result<Self> {
        let id = signal_hook::flag::register(signal, Arc::clone(&self.requested))?;
        self.registrations.push(id);
        Ok(self)
    }

    pub fn flag(&self) -> &AtomicBool {
        &self.requested
    }

    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        for id in &self.registrations {
            signal_hook::low_level::unregister(*id);
        }
    }
}
