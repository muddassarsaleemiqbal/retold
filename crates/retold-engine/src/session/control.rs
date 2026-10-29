//! A session's cancellation and event observer can be owned by a foreground or studio client.
use crate::events::CheckedObserver;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default)]
pub struct Control {
    pub(super) cancel: CancellationToken,
    pub(super) observer: Option<CheckedObserver>,
}

impl Control {
    #[must_use]
    pub fn with_observer(mut self, observer: CheckedObserver) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}
