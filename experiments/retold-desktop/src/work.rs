//! A deliberately slow, cancellable fixture query for the native responsiveness gate.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn slow_query(&self) -> &'static str {
        for _ in 0..200 {
            if self.is_cancelled() {
                return "Background check stopped. Your draft is unchanged.";
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        "Background check complete. Editing and navigation remained available."
    }
}
