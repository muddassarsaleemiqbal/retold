//! Version 1 NDJSON envelope. Breaking changes require a new schema version.

pub use retold_engine::events::Detail;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const EXIT_BLOCKED: u8 = 3;
pub const EXIT_EXECUTION_FAILURE: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ErrorCode {
    #[serde(rename = "RG-ACCOUNT-REQUIRED")]
    AccountRequired,
    #[serde(rename = "RG-ACCOUNTING-UNAVAILABLE")]
    AccountingUnavailable,
    #[serde(rename = "RG-INPUT-REQUIRED")]
    InputRequired,
    #[serde(rename = "RG-NOT-AVAILABLE")]
    NotAvailable,
    #[serde(rename = "RG-EXECUTION-FAILED")]
    ExecutionFailed,
    #[serde(rename = "RG-REF-NOT-FOUND")]
    RefNotFound,
    #[serde(rename = "RG-REF-AMBIGUOUS")]
    RefAmbiguous,
    #[serde(rename = "RG-CURSOR-INVALID")]
    CursorInvalid,
    #[serde(rename = "RG-DATA-INVALID")]
    DataInvalid,
    #[serde(rename = "RG-LIBRARY-INCOMPLETE")]
    LibraryIncomplete,
    #[serde(rename = "RG-REVIEW-REQUIRED")]
    ReviewRequired,
    #[serde(rename = "RG-PLAN-CHANGED")]
    PlanChanged,
    #[serde(rename = "RG-NOT-READY")]
    NotReady,
    #[serde(rename = "RG-ACCOUNT-BUSY")]
    AccountBusy,
    #[serde(rename = "RG-STUDIO-REQUIRED")]
    StudioRequired,
}

const CODE_CATALOG: &[(ErrorCode, &str, &str)] = &[
    (
        ErrorCode::AccountRequired,
        "RG-ACCOUNT-REQUIRED",
        "Inspect account show and restore finite authority or resolve existing attempts before dispatching.",
    ),
    (
        ErrorCode::AccountingUnavailable,
        "RG-ACCOUNTING-UNAVAILABLE",
        "Restore authoritative accounting. Never reinitialize over lost or corrupt history.",
    ),
    (
        ErrorCode::InputRequired,
        "RG-INPUT-REQUIRED",
        "Supply the required command inputs or correct conflicting flags. See --help.",
    ),
    (
        ErrorCode::NotAvailable,
        "RG-NOT-AVAILABLE",
        "This action has not been implemented. No work was dispatched.",
    ),
    (
        ErrorCode::ExecutionFailed,
        "RG-EXECUTION-FAILED",
        "Inspect the saved run logs before retrying. Independent work may still be running; a failure never grants permission to dispatch again.",
    ),
    (
        ErrorCode::RefNotFound,
        "RG-REF-NOT-FOUND",
        "Run video list and use a full reference or immutable ID.",
    ),
    (
        ErrorCode::RefAmbiguous,
        "RG-REF-AMBIGUOUS",
        "Run video list and use a full reference or immutable ID.",
    ),
    (
        ErrorCode::CursorInvalid,
        "RG-CURSOR-INVALID",
        "Restart the affected list without --cursor to obtain a fresh snapshot.",
    ),
    (
        ErrorCode::DataInvalid,
        "RG-DATA-INVALID",
        "Inspect the library files and restore valid metadata or cache evidence. Queries never repair files.",
    ),
    (
        ErrorCode::LibraryIncomplete,
        "RG-LIBRARY-INCOMPLETE",
        "Run video list to locate invalid manifests; resolve them before selecting a video.",
    ),
    (
        ErrorCode::ReviewRequired,
        "RG-REVIEW-REQUIRED",
        "Saved work waits for review. Inspect it; continuing never approves content.",
    ),
    (
        ErrorCode::PlanChanged,
        "RG-PLAN-CHANGED",
        "The video or its inputs changed after planning. Run video plan again and continue with the new plan hash.",
    ),
    (
        ErrorCode::NotReady,
        "RG-NOT-READY",
        "Export uses only deliveries saved at ready. Continue the video to ready, then export a listed edition and format.",
    ),
    (
        ErrorCode::AccountBusy,
        "RG-ACCOUNT-BUSY",
        "Running jobs or account changes hold the account. Let them finish, or stop or pause them, then retry. Nothing was changed.",
    ),
    (
        ErrorCode::StudioRequired,
        "RG-STUDIO-REQUIRED",
        "Queue commands require a running studio. Start it with retold studio start.",
    ),
];

impl ErrorCode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        CODE_CATALOG
            .iter()
            .find(|entry| entry.0 == self)
            .map_or("RG-EXECUTION-FAILED", |entry| entry.1)
    }

    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        CODE_CATALOG
            .iter()
            .find(|entry| entry.1 == code)
            .map(|entry| entry.0)
    }

    #[must_use]
    pub fn explanation(self) -> &'static str {
        CODE_CATALOG
            .iter()
            .find(|entry| entry.0 == self)
            .map_or("Inspect saved run logs.", |entry| entry.2)
    }

    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::InputRequired | Self::RefNotFound | Self::RefAmbiguous | Self::CursorInvalid => 2,
            Self::NotAvailable
            | Self::LibraryIncomplete
            | Self::AccountRequired
            | Self::ReviewRequired
            | Self::PlanChanged
            | Self::NotReady
            | Self::AccountBusy
            | Self::StudioRequired => EXIT_BLOCKED,
            Self::ExecutionFailed | Self::DataInvalid | Self::AccountingUnavailable => {
                EXIT_EXECUTION_FAILURE
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    pub code: ErrorCode,
    pub message: String,
    pub context: serde_json::Value,
    pub suggested_actions: Vec<Vec<String>>,
    pub redacted: bool,
}

impl Diagnostic {
    #[must_use]
    pub const fn into_payload(self) -> Payload {
        let exit = self.code.exit_code();
        let outcome = if exit == EXIT_BLOCKED {
            Outcome::Blocked
        } else {
            Outcome::Failed
        };
        Payload::Error {
            outcome,
            diagnostic: self,
        }
    }

    #[must_use]
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
            context: serde_json::Value::Null,
            suggested_actions: vec![vec!["retold".into(), "--help".into()]],
            redacted: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Queued,
    Detached,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Payload {
    Event(Detail),
    Result {
        outcome: Outcome,
        #[serde(rename = "data")]
        contents: serde_json::Value,
    },
    Error {
        outcome: Outcome,
        #[serde(flatten)]
        diagnostic: Diagnostic,
    },
}

impl Payload {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Error { diagnostic, .. } => diagnostic.code.exit_code(),
            Self::Event(_) | Self::Result { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    pub schema_version: u32,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(flatten)]
    pub body: Payload,
}

impl Envelope {
    #[must_use]
    pub const fn new(request_id: String, body: Payload) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            request_id,
            job_id: None,
            sequence: None,
            body,
        }
    }
}

/// The diagnostic browser and CLI share the authoritative code catalog.
#[must_use]
pub fn diagnostics() -> Payload {
    let items = CODE_CATALOG.iter().map(|(_,code,explanation)| serde_json::json!({"id":code,"name":code,"summary":explanation})).collect::<Vec<_>>();
    Payload::Result {
        outcome: Outcome::Completed,
        contents: serde_json::json!({"items":items}),
    }
}
