//! Reviewed targeted retakes: one frame or narration, bound to video identity and revision.
use std::path::{Path, PathBuf};

use crate::protocol::{Diagnostic, ErrorCode};
use retold_engine::editing::retake;

pub use retake::{Retake, RetakePlan};

/// Opaque retake review; requesting it grants no provider dispatch.
#[derive(Debug, Clone)]
pub struct ReviewedRetake {
    library: PathBuf,
    video_id: String,
    reference: String,
    pub target: Retake,
    pub plan: RetakePlan,
}

/// Read-only effect of a retake.
pub fn review(root: &Path, reference: &str, target: Retake) -> Result<ReviewedRetake, Diagnostic> {
    let library = root.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(reference)?;
    let plan =
        retake::plan(&video.layout, &target).map_err(|error| crate::editing::diagnostic(&error))?;
    Ok(ReviewedRetake {
        library,
        video_id: video.id.clone(),
        reference: video.reference.clone(),
        target,
        plan,
    })
}

/// Request exactly the reviewed retake; a lost response can be retried safely.
pub fn request(review: &ReviewedRetake) -> Result<RetakePlan, Diagnostic> {
    let _activity = crate::account::activity("request a reviewed retake")
        .map_err(|error| crate::account::commands::diagnostic(&error))?;
    let library = review.library.canonicalize().map_err(invalid)?;
    let catalog = crate::discovery::scan(&library)?;
    let video = catalog.resolve(&review.reference)?;
    let same = library == review.library && video.id == review.video_id;
    if !same {
        return Err(Diagnostic::new(
            ErrorCode::PlanChanged,
            "The video changed. Review the retake again.",
        ));
    }
    retake::request(&video.layout, &review.plan, &review.target)
        .map_err(|error| crate::editing::diagnostic(&error))
}

fn invalid(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::new(ErrorCode::DataInvalid, &format!("Retake: {error}"))
}
