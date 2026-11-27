use super::{limits, message_type, run_typescript};
use crate::support::{decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wire::Encoding;
use wire::typescript::{PayloadLimits, SchemaLimits, ValidatedRegistry};

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire")]
enum Choice {
    #[serde(rename = "ready-now")]
    Ready,
    #[serde(rename = "has-value")]
    Value(u16),
}
#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire", fixed)]
struct Record {
    #[serde(rename = "__proto__")]
    flag: bool,
    #[serde(rename = "choice-value")]
    choice: Choice,
}

#[test]
fn explicit_serde_names_reach_generated_clients_without_changing_bytes() {
    let graph = ValidatedRegistry::from_type::<Record>("Record", SchemaLimits::default()).unwrap();
    let metadata: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(
        metadata["Record"],
        json!({"k":"record","fixed":true,"fields":[
            {"name":"__proto__","value":{"k":"bool"}},
            {"name":"choice-value","value":{"k":"enum","variants":[
                {"name":"ready-now","id":0,"value":{"k":"unit"}},
                {"name":"has-value","id":1,"value":{"k":"u","bits":16}}
            ]}}
        ]})
    );
    let records = [
        Record {
            flag: false,
            choice: Choice::Ready,
        },
        Record {
            flag: true,
            choice: Choice::Value(128),
        },
    ];
    let literals = [vec![0, 0, 0], vec![1, 1, 128, 1, 0]];
    for (record, literal) in records.iter().zip(&literals) {
        assert_eq!(
            encode(record, Encoding::Payload, limits()).unwrap(),
            *literal
        );
    }
    let module = message_type::<Record>("Record")
        .payload_module(
            "codec",
            "MessageRecord",
            "./runtime-adapter.ts",
            PayloadLimits {
                bytes: 128,
                nodes: 512,
                depth: 32,
            },
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap();
    let output = run_typescript(
        &json!({"module":module,"literals":literals}),
        include_str!("exchange.mjs"),
    );
    let output: Vec<Vec<u8>> = serde_json::from_value(output).unwrap();
    assert_eq!(output, literals);
    for (bytes, record) in output.iter().zip(&records) {
        assert_eq!(
            &decode::<Record>(bytes, Encoding::Payload, limits()).unwrap(),
            record
        );
    }
}
