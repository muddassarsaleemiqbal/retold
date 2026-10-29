//! Native budget reads select the canonical installation account, never a workspace copy.
use crate::account::{
    self, Account,
    report::{Query, Report},
};
use crate::protocol::Diagnostic;

pub fn inspect(query: Query) -> Result<Report, Diagnostic> {
    account::canonical_directory()
        .and_then(|directory| Account::open(&directory))
        .and_then(|account| account.spend(query))
        .map_err(|error| account::commands::diagnostic(&error))
}
