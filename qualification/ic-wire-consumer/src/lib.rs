//! Portable application-owned contract for a bounded sample upload.
//! No transport, persistence, SDK, authentication or endpoint execution.
#![no_std]
extern crate alloc;
use alloc::{string::String, vec::Vec};
pub use wire::{Bytes, Error, Limits, List, Text};

/// Bounded packed application sample field.
pub type Samples = wire::Packed<i16, 64>;

/// Frozen upload identity, independent of crate version.
pub const METHOD: u64 = 7;
/// Complete request frame and native traversal allowances.
pub const REQUEST_LIMITS: Limits = Limits {
    bytes: 1400,
    allocation: 65536,
    nodes: 1024,
    depth: 16,
};
/// Complete response frame and native traversal allowances.
pub const RESPONSE_LIMITS: Limits = Limits {
    bytes: 32,
    allocation: 2112,
    nodes: 36,
    depth: 8,
};

/// Application's explicit field representations; ordinary tags stay a list.
#[derive(Debug, PartialEq, wire::Wire)]
#[wire(crate = "wire")]
pub struct Upload {
    /// Expected application revision.
    pub revision: u64,
    /// UTF-8 application key.
    pub key: Text<32>,
    /// Opaque content, capped at 1 KiB.
    pub content: Bytes<1024>,
    /// Explicit packed little-endian signed samples.
    pub samples: Samples,
    /// Ordinary Postcard integer tags.
    pub tags: List<u16, 8>,
}
/// Application reply, distinct from the upload shape.
#[derive(Debug, PartialEq, wire::Wire)]
#[wire(crate = "wire")]
pub struct Receipt {
    /// Committed revision reported by the application.
    pub revision: u64,
    /// Application result; Wire supplies no commit semantics.
    pub accepted: bool,
}

/// Representative request for native, browser and Wasm qualification.
/// # Errors
/// Returns a field-bound error if the fixed fixture ceases to fit its contract.
pub fn sample_upload() -> Result<Upload, Error> {
    Ok(Upload {
        revision: 128,
        key: Text::new(String::from("sensor"))?,
        content: Bytes::new(alloc::vec![0, 255])?,
        samples: Samples::new(alloc::vec![-32768, 0, 32767])?,
        tags: List::new(alloc::vec![1, 128])?,
    })
}

/// Encode this contract's request, including identity and request direction.
/// # Errors
/// Returns Wire's typed admission or serialization refusal.
pub fn encode_upload(value: &Upload) -> Result<Vec<u8>, Error> {
    wire::encode(
        value,
        wire::Encoding::Frame {
            method: METHOD,
            direction: wire::Direction::Request,
        },
        REQUEST_LIMITS,
    )
}
/// Decode a complete request using the fixed application policy.
/// # Errors
/// Returns identity, budget or malformed-input refusal.
pub fn decode_upload(bytes: &[u8]) -> Result<Upload, Error> {
    wire::decode(
        bytes,
        wire::Encoding::Frame {
            method: METHOD,
            direction: wire::Direction::Request,
        },
        REQUEST_LIMITS,
    )
}
/// Encode this contract's response with its independent budget.
/// # Errors
/// Returns Wire's typed admission or serialization refusal.
pub fn encode_receipt(value: &Receipt) -> Result<Vec<u8>, Error> {
    wire::encode(
        value,
        wire::Encoding::Frame {
            method: METHOD,
            direction: wire::Direction::Response,
        },
        RESPONSE_LIMITS,
    )
}
/// Decode a complete response; no retry or state interpretation is supplied.
/// # Errors
/// Returns identity, budget or malformed-input refusal.
pub fn decode_receipt(bytes: &[u8]) -> Result<Receipt, Error> {
    wire::decode(
        bytes,
        wire::Encoding::Frame {
            method: METHOD,
            direction: wire::Direction::Response,
        },
        RESPONSE_LIMITS,
    )
}

/// Collect the application types and describe the upload method for client generation
/// and API baseline checks.
///
/// # Errors
/// Returns a schema error if the application types exceed the registry limits.
#[cfg(feature = "schema")]
pub fn upload_contract() -> Result<
    (
        wire::typescript::ValidatedRegistry,
        wire::typescript::Method<'static>,
    ),
    wire::typescript::GenerationError,
> {
    use wire::typescript::{Method, PayloadLimits, SchemaLimits, SchemaRegistry};

    let method = Method {
        export: "upload",
        id: METHOD,
        request: "Upload",
        response: "Receipt",
        request_limits: PayloadLimits {
            bytes: 1389,
            nodes: 32,
            depth: 8,
        },
        response_limits: PayloadLimits {
            bytes: 21,
            nodes: 3,
            depth: 2,
        },
    };

    let mut registry = SchemaRegistry::new(SchemaLimits::default())?;
    registry.register::<Upload>(method.request)?;
    registry.register::<Receipt>(method.response)?;

    Ok((registry.finish()?, method))
}

/// Generate typed payload and method clients from the application types.
/// # Errors
/// Returns schema or bounded-output admission errors.
#[cfg(feature = "schema")]
pub fn clients(
    runtime: &str,
    mut diagnostics: impl FnMut(wire::typescript::BulkLintReport),
) -> Result<(String, String), wire::typescript::GenerationError> {
    let (graph, method) = upload_contract()?;

    Ok((
        graph.payload_module(
            "uploadPayload",
            method.request,
            runtime,
            method.request_limits,
            &Default::default(),
            &mut diagnostics,
        )?,
        graph.method_module(&[method], runtime, &Default::default(), &mut diagnostics)?,
    ))
}

/// Run the upload probe and return seven little-endian counter deltas and the two frames.
///
/// # Panics
/// Panics if the supplied counter goes backwards or the fixed codec fixture fails.
#[cfg(feature = "ic")]
pub fn instruction_reply(mut counter: impl FnMut() -> u64) -> Vec<u8> {
    use core::hint::black_box;
    const REPETITIONS: usize = 8;

    let request = sample_upload().unwrap();
    let receipt = Receipt {
        revision: 129,
        accepted: true,
    };
    let request_bytes = encode_upload(&request).unwrap();
    let receipt_bytes = encode_receipt(&receipt).unwrap();
    let mut bad = request_bytes.clone();
    bad[0] ^= 1;
    let mut counts = [0_u64; 7];
    for (operation, count) in counts.iter_mut().enumerate() {
        let run = || match operation {
            0 => {
                black_box(encode_upload(black_box(&request)).unwrap());
            }
            1 => {
                black_box(decode_upload(black_box(&request_bytes)).unwrap());
            }
            2 => {
                black_box(encode_receipt(black_box(&receipt)).unwrap());
            }
            3 => {
                black_box(decode_receipt(black_box(&receipt_bytes)).unwrap());
            }
            4 => {
                assert!(decode_upload(black_box(&bad)).is_err());
            }
            5 => {
                assert!(
                    decode_upload(black_box(&request_bytes[..request_bytes.len() - 1])).is_err()
                );
            }
            6 => {
                black_box(&request_bytes);
            }
            _ => unreachable!(),
        };
        run();
        let start = counter();
        for _ in 0..REPETITIONS {
            run();
        }
        *count = counter() - start;
    }
    let mut reply: Vec<u8> = counts.into_iter().flat_map(u64::to_le_bytes).collect();
    reply.extend(request_bytes);
    reply.extend(receipt_bytes);

    reply
}
