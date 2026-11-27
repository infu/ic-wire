use super::{grouped_messages, run_typescript};
use serde_json::json;
use std::collections::BTreeMap;
use wire::typescript::{GenerationError, Method, PayloadLimits, SchemaLimits, ValidatedRegistry};
#[test]
fn scalar_bounds_and_checks_survive_graph_export_and_generated_clients() {
    let types = BTreeMap::from([
        (
            "Root".into(),
            json!({"k":"ref","name":"Alias","check":"root"}),
        ),
        (
            "Alias".into(),
            json!({"k":"newtype","check":"alias","item":{"k":"ref","name":"Value","check":"item"}}),
        ),
        (
            "Value".into(),
            json!({"k":"u","bits":16,"min":2,"max":5,"check":"leaf"}),
        ),
    ]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&graph.metadata_json().unwrap()).unwrap(),
        json!(types)
    );
    let graph =
        ValidatedRegistry::from_metadata(&grouped_messages(types.clone()), SchemaLimits::default())
            .unwrap();
    let policy = PayloadLimits {
        bytes: 16,
        nodes: 3,
        depth: 3,
    };
    let module = graph
        .payload_module(
            "checked",
            "MessageRoot",
            "./runtime-adapter.ts",
            policy,
            &BTreeMap::new(),
            |report| assert!(report.is_clean()),
        )
        .unwrap();
    let frame = graph
        .method_module(
            &[Method {
                export: "method",
                id: 7,
                request: "MessageRoot",
                response: "MessageRoot",
                request_limits: policy,
                response_limits: policy,
            }],
            "./runtime-adapter.ts",
            &BTreeMap::new(),
            |report| assert!(report.is_clean()),
        )
        .unwrap();
    let output = run_typescript(
        &json!({"types":types,"module":module,"frameModule":frame,"runtimeAdapter":include_str!("adapter.ts")}),
        include_str!("exchange.mjs"),
    );
    assert_eq!(output, json!(true));
}
#[test]
fn scalar_metadata_refuses_unsafe_or_inconsistent_constraints() {
    for (value, error) in [
        (
            json!({"k":"u","bits":64,"min":9007199254740992_u64}),
            GenerationError::Complexity,
        ),
        (
            json!({"k":"i","bits":128,"max":-9007199254740992_i64}),
            GenerationError::Complexity,
        ),
        (
            json!({"k":"i","bits":16,"min":1.5}),
            GenerationError::Schema,
        ),
        (
            json!({"k":"float","bits":32,"min":2,"max":1}),
            GenerationError::Schema,
        ),
        (
            json!({"k":"u","bits":8,"min":null}),
            GenerationError::Schema,
        ),
        (json!({"k":"bool","check":3}), GenerationError::Schema),
        (
            json!({"k":"ref","name":"Root","max":4}),
            GenerationError::Schema,
        ),
    ] {
        assert_eq!(
            ValidatedRegistry::from_metadata(
                &BTreeMap::from([("Root".into(), value)]),
                SchemaLimits::default()
            )
            .unwrap_err(),
            error
        );
    }
    let types = BTreeMap::from([("Root".into(), json!({"k":"bool","check":"a/\"~\n"}))]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&graph.metadata_json().unwrap()).unwrap(),
        json!(types)
    );
    assert_eq!(
        ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                names_bytes: 8,
                ..SchemaLimits::default()
            }
        )
        .unwrap_err(),
        GenerationError::Complexity
    );
}
