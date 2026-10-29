//! Worker and CLI fingerprints agree independently of `serde_json` object-order features.
use retold_engine::store;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct Record {
    zeta: serde_json::Value,
    alpha: String,
}

#[test]
fn nested_json_maps_match_legacy_sorted_output() {
    let value: serde_json::Value = serde_json::from_str(r#"{"z":2,"a":{"y":3,"b":1}}"#).unwrap();
    let expected = br#"{"a":{"b":1,"y":3},"z":2}"#;
    assert_eq!(
        store::hash(&value).unwrap(),
        blake3::hash(expected).to_hex().as_str()
    );
}

#[test]
fn declared_struct_fields_and_sequences_retain_their_existing_fingerprints() {
    let record = Record {
        zeta: serde_json::json!({"z":2,"a":1}),
        alpha: "kept".into(),
    };
    let expected = br#"{"zeta":{"a":1,"z":2},"alpha":"kept"}"#;
    assert_eq!(
        store::hash(&record).unwrap(),
        blake3::hash(expected).to_hex().as_str()
    );
    let records = (record, ["z", "a"]);
    let expected = br#"[{"zeta":{"a":1,"z":2},"alpha":"kept"},["z","a"]]"#;
    assert_eq!(
        store::hash(&records).unwrap(),
        blake3::hash(expected).to_hex().as_str()
    );
}

#[test]
fn integer_map_keys_keep_numeric_order() {
    const TWO_DIGIT_KEY: i32 = 10;
    let values: BTreeMap<_, _> = [(2, "two"), (TWO_DIGIT_KEY, "ten")].into();
    let expected = br#"{"2":"two","10":"ten"}"#;
    assert_eq!(
        store::hash(&values).unwrap(),
        blake3::hash(expected).to_hex().as_str()
    );
}
