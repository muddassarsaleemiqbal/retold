//! A review owns the exact request and hash; submission cannot silently accept a fresh plan.
use std::path::{Path, PathBuf};

use crate::continuation::ContinueOptions;
use crate::plan::{ExecutablePlan, ExportOptions, PlanOptions};
use crate::protocol::{Diagnostic, ErrorCode};
use crate::queue::JobRecord;
use crate::studio::SubmitOptions;

#[derive(Debug, Clone)]
pub struct ReviewedRun {
    pub library: PathBuf,
    pub reference: String,
    pub options: PlanOptions,
    pub plan: ExecutablePlan,
}

/// Export has a separate review and immutable authority.
#[derive(Debug, Clone)]
pub struct ReviewedExport {
    request: ReviewedRun,
    input: ExportRequest,
    available: Vec<retold_engine::session::PreparedDelivery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    pub delivery: ExportOptions,
    pub spend: String,
    pub max_requests: u64,
}

impl Default for ExportRequest {
    fn default() -> Self {
        Self {
            delivery: ExportOptions::default(),
            spend: "0".into(),
            max_requests: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeliveryEdition {
    pub id: String,
    pub label: String,
}

pub fn delivery_editions(specifications: &[String]) -> Result<Vec<DeliveryEdition>, Diagnostic> {
    let mut editions = std::collections::BTreeMap::new();
    for specification in specifications {
        let variant = specification
            .parse::<retold_engine::Variant>()
            .map_err(|_| {
                Diagnostic::new(
                    ErrorCode::DataInvalid,
                    "Saved edition specification is invalid.",
                )
            })?;
        let id = retold_engine::languages::edition_id(&variant);
        editions
            .entry(id.clone())
            .or_insert_with(|| DeliveryEdition {
                id,
                label: variant.describe(),
            });
    }
    Ok(editions.into_values().collect())
}

pub fn review_export(
    root: &Path,
    reference: &str,
    export: &ExportRequest,
) -> Result<ReviewedExport, Diagnostic> {
    let options = PlanOptions {
        spend: Some(export.spend.clone()),
        max_requests: Some(export.max_requests),
        export: Some(export.delivery.clone()),
        ..PlanOptions::default()
    };
    let request = compile_review(root, reference, &options)?;
    let available = retold_engine::session::prepared_deliveries(&request.plan.runtime.layout)
        .map_err(|_| {
            Diagnostic::new(
                ErrorCode::DataInvalid,
                "Prepared delivery records are invalid or unavailable.",
            )
        })?;
    Ok(ReviewedExport {
        request,
        input: export.clone(),
        available,
    })
}

impl ReviewedExport {
    #[must_use]
    pub fn matches_request(&self, input: &ExportRequest) -> bool {
        self.input == *input
    }
    #[must_use]
    pub fn output_directory(&self) -> PathBuf {
        self.request.plan.runtime.layout.deliverables()
    }
    #[must_use]
    pub fn available(&self) -> &[retold_engine::session::PreparedDelivery] {
        &self.available
    }
    #[must_use]
    pub const fn plan(&self) -> &ExecutablePlan {
        &self.request.plan
    }
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.request.reference
    }
    #[must_use]
    pub fn library(&self) -> &Path {
        &self.request.library
    }
    pub fn submit(&self) -> Result<JobRecord, Diagnostic> {
        self.request
            .submit(&format!("desktop-export-{}", self.request.plan.plan_hash))
    }
}

pub fn review_run(
    root: &Path,
    reference: &str,
    options: &PlanOptions,
) -> Result<ReviewedRun, Diagnostic> {
    let exporting = options.export.is_some();
    if exporting {
        return Err(Diagnostic::new(
            ErrorCode::InputRequired,
            "Run review stops at ready. Use a separate explicit delivery review for export.",
        ));
    }
    compile_review(root, reference, options)
}

fn compile_review(
    root: &Path,
    reference: &str,
    options: &PlanOptions,
) -> Result<ReviewedRun, Diagnostic> {
    let catalog = crate::discovery::scan(root)?;
    super::queue_state(&catalog.root)?;
    let (plan, _) = crate::plan::compile(&catalog, reference, options)?;
    Ok(ReviewedRun {
        library: catalog.root,
        reference: reference.into(),
        options: options.clone(),
        plan,
    })
}

impl ReviewedRun {
    pub fn submit(&self, idempotency_key: &str) -> Result<JobRecord, Diagnostic> {
        super::queue_state(&self.library)?;
        let options = SubmitOptions {
            execution: ContinueOptions {
                plan: self.options.clone(),
                plan_hash: Some(self.plan.plan_hash.clone()),
                yes: false,
            },
            idempotency_key: Some(idempotency_key.into()),
        };
        crate::studio::submit_job(&self.library, &self.reference, &options)
    }
}
