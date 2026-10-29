//! Evicted event pages must not leave an unbounded trail of cache tombstones.
use super::*;
use retold_app::protocol::Outcome;

const EVENT_PAGES: usize = 1_000;

#[test]
fn completed_event_pages_are_fully_evicted_after_their_cursor_advances() {
    let mut store = Store::default();
    for sequence in 0..EVENT_PAGES {
        let command = words(&format!("queue events example --after-sequence {sequence}"));
        store.mark_loading(std::slice::from_ref(&command));
        store.fill(
            command.clone(),
            Payload::Result {
                outcome: Outcome::Completed,
                contents: serde_json::json!({"events":[]}),
            },
            0,
        );
        store.remove(&command);
    }
    assert_eq!(store.slots.len(), 0);
}
