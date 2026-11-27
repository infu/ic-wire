use super::{grouped_messages, limits, run_typescript};
use crate::support::{decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BinaryHeap},
    num::NonZeroI64,
};
use wire::Encoding;
use wire::typescript::{Method, PayloadLimits, SchemaLimits, ValidatedRegistry};

macro_rules! record {
    ($new:ident,$ty:ty,$new_path:literal) => {
        #[derive(Debug, Serialize, Deserialize, wire::typescript::Wire)]
        #[wire(crate = "wire", fixed)]
        struct $new {
            #[serde(with=$new_path)]
            value: $ty,
        }
    };
}
record!(Bools, Vec<bool>, "wire::serde_packed::bools");
record!(F32, Vec<f32>, "wire::serde_packed::finite_f32");
record!(F64, Vec<f64>, "wire::serde_packed::finite_f64");
record!(Nonzero, Vec<NonZeroI64>, "wire::serde_packed::nonzero_i64");
record!(Heap, BinaryHeap<u64>, "wire::serde_packed::heap_u64");
fn native<T: Serialize + for<'a> Deserialize<'a>>(value: &T, bytes: &[u8]) {
    let mut message = bytes.to_vec();
    message.push(0);
    let bytes = message.as_slice();
    assert_eq!(encode(value, Encoding::Payload, limits()).unwrap(), bytes);
    let decoded = decode::<T>(bytes, Encoding::Payload, limits()).unwrap();
    assert_eq!(
        encode(&decoded, Encoding::Payload, limits()).unwrap(),
        bytes
    );
}
fn schema<T: wire::typescript::Wire>(name: &str) -> Value {
    serde_json::from_str::<Value>(
        &ValidatedRegistry::from_type::<T>(name, SchemaLimits::default())
            .unwrap()
            .metadata_json()
            .unwrap(),
    )
    .unwrap()[name]
        .clone()
}

#[test]
fn named_adapters_remain_inline_fields_without_synthetic_layout_types() {
    use wire::typescript::SchemaRegistry;
    let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    registry.register::<F32>("Floats").unwrap();
    let metadata: Value =
        serde_json::from_str(&registry.finish().unwrap().metadata_json().unwrap()).unwrap();
    let definitions = metadata.as_object().unwrap();
    assert_eq!(definitions.len(), 2); // Public root plus actual Rust record, no adapter identity.
    let name = metadata["Floats"]["name"].as_str().unwrap();
    assert_eq!(metadata[name], schema::<F32>("Floats"));
}
#[test]
fn domain_adapters_match_literal_bytes_and_browser_runtime() {
    let bools = vec![2, 0, 1];
    let f32 = vec![8, 0, 0, 0, 128, 1, 0, 0, 0];
    let f64 = vec![16, 0, 0, 0, 0, 0, 0, 0, 128, 1, 0, 0, 0, 0, 0, 0, 0];
    let nonzero = vec![
        16, 255, 255, 255, 255, 255, 255, 255, 255, 7, 0, 0, 0, 0, 0, 0, 0,
    ];
    let heap = vec![
        24, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0,
    ];
    native(
        &Bools {
            value: vec![false, true],
        },
        &bools,
    );
    native(
        &F32 {
            value: vec![-0.0, f32::from_bits(1)],
        },
        &f32,
    );
    native(
        &F64 {
            value: vec![-0.0, f64::from_bits(1)],
        },
        &f64,
    );
    let values = vec![NonZeroI64::new(-1).unwrap(), NonZeroI64::new(7).unwrap()];
    native(&Nonzero { value: values }, &nonzero);
    native(
        &Heap {
            value: BinaryHeap::from(vec![3, 1, 3]),
        },
        &heap,
    );
    let types = BTreeMap::from([
        ("Bools".into(), schema::<Bools>("Bools")),
        ("F32".into(), schema::<F32>("F32")),
        ("F64".into(), schema::<F64>("F64")),
        ("Nonzero".into(), schema::<Nonzero>("Nonzero")),
        ("Heap".into(), schema::<Heap>("Heap")),
    ]);
    for (name, word) in [
        ("Bools", "bool"),
        ("F32", "finite-f32"),
        ("F64", "finite-f64"),
        ("Heap", "u64"),
    ] {
        assert_eq!(
            types[name],
            json!({"k":"record","fixed":true,"fields":[{"name":"value","value":{"k":"packed","word":word}}]})
        );
    }
    assert_eq!(
        types["Nonzero"],
        json!({"k":"record","fixed":true,"fields":[{"name":"value","value":{"k":"packed","word":"i64","nonzero":true}}]})
    );
    let graph =
        ValidatedRegistry::from_metadata(&grouped_messages(types.clone()), SchemaLimits::default())
            .unwrap();
    let policy = PayloadLimits {
        bytes: 64,
        nodes: 3,
        depth: 3,
    };
    let names = [
        ("bools", "MessageBools"),
        ("floats32", "MessageF32"),
        ("floats64", "MessageF64"),
        ("nonzero", "MessageNonzero"),
        ("heap", "MessageHeap"),
    ];
    let methods: Vec<_> = names
        .iter()
        .enumerate()
        .map(|(id, (export, root))| Method {
            export,
            id: id as u64,
            request: root,
            response: root,
            request_limits: policy,
            response_limits: policy,
        })
        .collect();
    let frame = graph
        .method_module(
            &methods,
            "./runtime-adapter.ts",
            &BTreeMap::new(),
            |report| assert!(report.is_clean()),
        )
        .unwrap();
    let module = graph
        .payload_module(
            "floats",
            "MessageF32",
            "./runtime-adapter.ts",
            policy,
            &BTreeMap::new(),
            |report| assert!(report.is_clean()),
        )
        .unwrap();
    let expected = [bools, f32, f64, nonzero, heap].map(|mut bytes| {
        bytes.push(0);
        bytes
    });
    let output = run_typescript(
        &json!({"types":types,"module":module,"frameModule":frame,"bytes":expected}),
        include_str!("exchange.mjs"),
    );
    assert_eq!(output, json!(expected));
}
#[test]
fn invalid_domain_values_and_heap_normalization_remain_explicit() {
    for byte in 2..=255 {
        assert_eq!(
            decode::<Bools>(&[1, byte], Encoding::Payload, limits()).unwrap_err(),
            wire::Error::Malformed
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            encode(&F32 { value: vec![value] }, Encoding::Payload, limits()),
            Err(wire::Error::Malformed)
        );
        let mut bytes = vec![4];
        bytes.extend(value.to_le_bytes());
        assert_eq!(
            decode::<F32>(&bytes, Encoding::Payload, limits()).unwrap_err(),
            wire::Error::Malformed
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            encode(&F64 { value: vec![value] }, Encoding::Payload, limits()),
            Err(wire::Error::Malformed)
        );
        let mut bytes = vec![8];
        bytes.extend(value.to_le_bytes());
        assert_eq!(
            decode::<F64>(&bytes, Encoding::Payload, limits()).unwrap_err(),
            wire::Error::Malformed
        );
    }
    assert_eq!(
        decode::<F32>(&[3, 0, 0, 0], Encoding::Payload, limits()).unwrap_err(),
        wire::Error::Malformed
    );
    assert_eq!(
        decode::<F64>(&[4, 0, 0, 0, 0], Encoding::Payload, limits()).unwrap_err(),
        wire::Error::Malformed
    );
    assert_eq!(
        decode::<Nonzero>(&[8, 0, 0, 0, 0, 0, 0, 0, 0], Encoding::Payload, limits()).unwrap_err(),
        wire::Error::Malformed
    );
    let mut unordered = vec![24];
    for word in [1_u64, 3, 3] {
        unordered.extend(word.to_le_bytes());
    }
    unordered.push(0);
    let decoded = decode::<Heap>(&unordered, Encoding::Payload, limits()).unwrap();
    assert_eq!(decoded.value.clone().into_sorted_vec(), [1, 3, 3]);
    assert_ne!(
        encode(&decoded, Encoding::Payload, limits()).unwrap(),
        unordered
    );
    // Plain lists remain per-element Serde, without a bulk prefix or finite guard.
    assert_eq!(
        encode(&vec![1.0_f32], Encoding::Payload, limits()).unwrap(),
        [1, 0, 0, 128, 63, 0]
    );
    assert!(encode(&vec![f32::INFINITY], Encoding::Payload, limits()).is_ok());
}
