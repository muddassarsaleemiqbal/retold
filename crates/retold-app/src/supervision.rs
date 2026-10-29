//! The host registers its own executable; supervisor children retain maintenance activity.
use std::ffi::OsString;
use std::io;

use retold_engine::process_supervision;

#[must_use]
pub fn is_entry(args: &[OsString]) -> bool {
    process_supervision::is_entry(args)
}

pub fn bootstrap(args: &[OsString]) -> io::Result<Option<u8>> {
    if process_supervision::is_worker(args) {
        return process_supervision::run_worker(args).map(Some);
    }
    if process_supervision::is_entry(args) {
        let activity = crate::account::activity("supervised provider process")
            .map_err(|error| io::Error::other(error.to_string()))?;
        let claim = activity
            .as_ref()
            .map(crate::account::Account::physical_claim)
            .transpose()
            .map_err(|error| io::Error::other(error.to_string()))?;
        return serve_claimed(args, claim).map(Some);
    }
    process_supervision::initialize(&std::env::current_exe()?)?;
    Ok(Option::default())
}

fn serve_claimed(
    args: &[OsString],
    mut claim: Option<crate::account::physical::Claim>,
) -> io::Result<u8> {
    let register = |identity| {
        if let Some(active) = claim.take() {
            claim = Some(
                active
                    .register_worker(identity)
                    .map_err(|error| io::Error::other(error.to_string()))?,
            );
        }
        Ok(())
    };
    let outcome = process_supervision::serve(args, register);
    finish(claim, outcome)
}

fn finish(
    claim: Option<crate::account::physical::Claim>,
    outcome: Result<u8, process_supervision::Failure>,
) -> io::Result<u8> {
    match outcome {
        Ok(code) => {
            if let Some(claim) = claim {
                claim
                    .complete()
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
            Ok(code)
        }
        Err(failure) => {
            match (claim, failure.started) {
                (Some(claim), true) => {
                    let _ = claim.retain_failure(&failure.error);
                }
                (Some(claim), false) => claim
                    .complete()
                    .map_err(|error| io::Error::other(error.to_string()))?,
                (None, _) => (),
            }
            Err(failure.error)
        }
    }
}
