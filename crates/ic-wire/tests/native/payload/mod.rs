//! Payload metadata uses the same schema validation and advice as framed methods.
use ic_wire::{Limits, typescript};
use serde_json::{Value, json};
use std::{cell::Cell, collections::BTreeMap};

fn method(schema: Value) -> Value {
    json!({"name":"echo","mode":"update","request":schema,"response":schema,
        "requestBytes":4096,"responseBytes":4096,"requestNodes":4096,
        "responseNodes":4096,"requestDepth":64,"responseDepth":64})
}

#[test]
fn payload_generation_requires_no_method_identity_and_keeps_bulk_advice() {
    let delivered = Cell::new(false);
    let source = typescript::generate_payload_registry(
        &BTreeMap::new(),
        &[method(json!({"k":"list","item":{"k":"u","bits":64}}))],
        "./consumer-runtime.js",
        &BTreeMap::new(),
        |report| {
            delivered.set(true);
            assert!(!report.is_clean());
        },
    )
    .unwrap();
    assert!(delivered.get());
    assert!(source.contains("from \"./consumer-runtime.js\""));
    assert!(source.contains("createPayloadCodec"));
    let mut framed = method(json!({"k":"unit"}));
    assert!(
        typescript::generate_registry(
            &BTreeMap::new(),
            &[framed.clone()],
            "./r.js",
            &BTreeMap::new(),
            |_| {}
        )
        .is_err()
    );
    framed["id"] = json!("7");
    assert!(
        typescript::generate_registry(
            &BTreeMap::new(),
            &[framed],
            "./r.js",
            &BTreeMap::new(),
            |_| {}
        )
        .is_ok()
    );
}

#[test]
fn payloads_admit_zero_byte_unit_limits_and_validate_all_other_allowances() {
    let limits = Limits {
        bytes: 0,
        allocation: 0,
        nodes: 4,
        depth: 4,
    };
    let policy = typescript::payload_method_limits(limits, limits).unwrap();
    assert_eq!(policy["requestBytes"], 0);
    assert!(typescript::method_limits(limits, limits).is_err());
    let mut value = method(json!({"k":"unit"}));
    value["requestBytes"] = json!(0);
    value["responseBytes"] = json!(0);
    let generate = |value| {
        typescript::generate_payload_registry(
            &BTreeMap::new(),
            &[value],
            "./r.js",
            &BTreeMap::new(),
            |_| {},
        )
    };
    assert!(generate(value.clone()).is_ok());
    for (key, invalid) in [
        ("requestBytes", json!(-1)),
        ("requestNodes", json!(0)),
        ("responseDepth", json!(0)),
        ("responseBytes", json!(9007199254740992u64)),
    ] {
        let mut invalid_value = value.clone();
        invalid_value[key] = invalid;
        assert!(generate(invalid_value).is_err());
    }
}

#[test]
fn standalone_validation_rejects_bad_references_and_alias_cycles() {
    let root = json!({"k":"ref","name":"Node"});
    let recursive = BTreeMap::from([("Node".into(), json!({"k":"option","item":root}))]);
    assert!(typescript::validate_payload_schema(&recursive, &root).is_ok());
    assert!(typescript::validate_payload_schema(&BTreeMap::new(), &root).is_err());
    let aliases = BTreeMap::from([("Node".into(), root.clone())]);
    assert!(typescript::validate_payload_schema(&aliases, &root).is_err());
    let missing = BTreeMap::from([("Node".into(), json!({"k":"ref","name":"Missing"}))]);
    assert!(typescript::validate_payload_schema(&missing, &root).is_err());
    for schema in [
        json!({"k":"u","bits":7}),
        json!({"k":"bytes","min":2,"max":1}),
        json!({"k":"text","check":false}),
    ] {
        assert!(typescript::validate_payload_schema(&BTreeMap::new(), &schema).is_err());
    }
}
