//! Startup joins maintenance admission only while creating or recovering studio state.
use std::path::Path;

use crate::account::{self, Account, commands};
use crate::protocol::{Diagnostic, ErrorCode};

pub fn validate_startup(library: Option<&Path>, state: &Path) -> Result<(), Diagnostic> {
    for path in library.into_iter().chain(std::iter::once(state)) {
        retold_engine::recovery::ensure_active(path)
            .map_err(|error| Diagnostic::new(ErrorCode::NotReady, &error.to_string()))?;
    }
    Ok(())
}

pub fn hold_startup(library: Option<&Path>, state: &Path) -> Result<Option<Account>, Diagnostic> {
    let activity =
        account::activity("start studio").map_err(|error| commands::diagnostic(&error))?;
    validate_startup(library, state)?;
    Ok(activity)
}
