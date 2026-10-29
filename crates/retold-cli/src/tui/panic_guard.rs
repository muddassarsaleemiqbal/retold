//! Recoverable dispatch panics stay inside their worker; fatal panics restore the terminal.
use std::cell::Cell;

thread_local! {
    static RECOVERING: Cell<bool> = const { Cell::new(false) };
}

pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_details| {
        let recovering = RECOVERING.with(Cell::get);
        if !recovering {
            super::release_input();
            previous(panic_details);
        }
    }));
}

pub fn catch<Output>(run: impl FnOnce() -> Output) -> std::thread::Result<Output> {
    let _scope = Recovering::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
}

struct Recovering(bool);

impl Recovering {
    fn enter() -> Self {
        Self(RECOVERING.with(|flag| flag.replace(true)))
    }
}

impl Drop for Recovering {
    fn drop(&mut self) {
        RECOVERING.with(|flag| flag.set(self.0));
    }
}
