//! Frozen public schema and a golden envelope guard the supported machine contract.

use retold_app::protocol::{Envelope, Outcome, Payload};
use retold_app::{Request, respond};

#[test]
fn schema_matches_the_published_v1_contract() {
    let Payload::Result { contents, .. } = respond(Request::Schema) else {
        unreachable!("schema generation failed");
    };
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/schemas/retold-output-v1.json")).unwrap();
    assert_eq!(contents, fixture);
}

#[test]
fn result_envelope_has_the_expected_wire_shape_and_round_trips() {
    let envelope = Envelope::new(
        "req-fixture".into(),
        Payload::Result {
            outcome: Outcome::Completed,
            contents: serde_json::json!({"items": []}),
        },
    );
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "schema_version": 1,
            "request_id": "req-fixture",
            "type": "result",
            "payload": {"outcome": "completed", "data": {"items": []}}
        })
    );
    let decoded: Envelope = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn job_progress_keeps_durable_identity_in_the_envelope() {
    const PROGRESS_SEQUENCE: u64 = 42;
    let mut envelope = Envelope::new(
        "req-watch".into(),
        Payload::Event(retold_app::protocol::Detail::JobProgress {
            timestamp: "2026-10-20T00:00:00Z".into(),
            stage: "research".into(),
            step: None,
            message: "Saved source".into(),
        }),
    );
    envelope.job_id = Some("job-fixture".into());
    envelope.sequence = Some(PROGRESS_SEQUENCE);
    let value = serde_json::to_value(&envelope).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"schema_version": 1, "request_id": "req-watch",
        "job_id": "job-fixture", "sequence": 42, "type": "event",
        "payload": {"event": "job_progress", "timestamp": "2026-10-20T00:00:00Z",
            "stage": "research", "step": null, "message": "Saved source"}})
    );
    let decoded: Envelope = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}
