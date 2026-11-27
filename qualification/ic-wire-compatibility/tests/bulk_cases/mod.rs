//! Independent workload patterns; source provenance is in qualification/README.md.
mod packed;
use super::{message_type, run_typescript};
use crate::support::{decode, encode};
use serde_json::json;
use wire::typescript::{Method, PayloadLimits};
use wire::{Bytes, Encoding, Limits};

const MAX: usize = 1_048_576;

#[test]
fn large_bytes_cross_growth_and_length_prefix_boundaries() {
    let graph = message_type::<Bytes<MAX>>("Blob");
    let limits = PayloadLimits {
        bytes: (MAX + 4) as u64,
        nodes: 2,
        depth: 2,
    };
    let module = graph
        .payload_module(
            "blob",
            "MessageBlob",
            "./runtime-adapter.ts",
            limits,
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap();
    let frame = graph
        .method_module(
            &[Method {
                export: "blobFrame",
                id: 7,
                request: "MessageBlob",
                response: "MessageBlob",
                request_limits: limits,
                response_limits: limits,
            }],
            "./runtime-adapter.ts",
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap();
    let sizes = [0, 63, 64, 127, 128, 16_383, 16_384, MAX];
    let output = run_typescript(
        &json!({"module":module,"frameModule":frame,"sizes":sizes}),
        include_str!("exchange.mjs"),
    );
    let payloads: Vec<Vec<u8>> = serde_json::from_value(output).unwrap();
    let policy = Limits {
        bytes: MAX + 4,
        // The projection uses one allocation ledger for sizing and writing.
        allocation: 18 * MAX,
        nodes: 32,
        depth: 8,
    };
    assert_eq!(payloads.len(), sizes.len());
    for (payload, size) in payloads.iter().zip(sizes) {
        let value = Bytes::<MAX>::new((0..size).map(|i| (i % 251) as u8).collect()).unwrap();
        assert_eq!(*payload, encode(&value, Encoding::Payload, policy).unwrap());
        assert_eq!(
            decode::<Bytes<MAX>>(payload, Encoding::Payload, policy).unwrap(),
            value
        );
    }
}
