//! Explicit resource admission for native Serde payloads in the same Postcard format.
//!
//! Types, IDs, deterministic serialization and domain rules are consumer-owned.
//! Resource admission follows Serde visits; no static schema is inferred.
//! Custom Serde implementations are trusted: allocations/work outside visited
//! values cannot be constrained here. See the runtime guide for budget semantics.
#[cfg(test)]
mod tests;

mod decode;
mod encode;
mod parser;

use crate::{Error, Limits, contract};
use alloc::vec::Vec;
use core::cell::Cell;
use serde::{Deserialize, Serialize};

struct Budget {
    nodes: Cell<usize>,
    heap_units: Cell<usize>,
    depth: usize,
    refused: Cell<bool>,
}
impl Budget {
    fn new(limits: Limits) -> Self {
        Self {
            nodes: Cell::new(limits.nodes),
            // All value charges are multiples of eight bytes. Store whole charge
            // units instead of bytes: the unused remainder cannot fund a charge.
            // This removes per-visit multiplication without changing admission,
            // overflow rejection, wire bytes or consumer limits.
            heap_units: Cell::new(limits.allocation.saturating_sub(limits.bytes) / 8),
            depth: limits.depth,
            refused: Cell::new(limits.allocation < limits.bytes),
        }
    }
    #[cfg_attr(target_arch = "wasm32", inline(always))]
    fn enter(&self, depth: usize, size: usize) -> bool {
        self.enter_many(depth, size, 1)
    }
    #[cfg_attr(target_arch = "wasm32", inline(always))]
    fn enter_many(&self, depth: usize, size: usize, count: usize) -> bool {
        if self.refused.get() {
            return false;
        }
        let Some(nodes) = self.nodes.get().checked_sub(count) else {
            self.refused.set(true);
            return false;
        };
        if depth > self.depth || !self.bytes(size) {
            self.refused.set(true);
            return false;
        }
        self.nodes.set(nodes);
        true
    }
    #[cfg_attr(target_arch = "wasm32", inline(always))]
    fn bytes(&self, bytes: usize) -> bool {
        let Some(heap) = self.heap_units.get().checked_sub(bytes) else {
            self.refused.set(true);
            return false;
        };
        self.heap_units.set(heap);
        true
    }
    fn error(&self) -> Error {
        if self.refused.get() {
            Error::Limit
        } else {
            Error::Malformed
        }
    }
}

/// Measure one native payload with no output allocation; `bytes` excludes a frame.
pub fn encoded_len<T: Serialize + ?Sized>(value: &T, limits: Limits) -> Result<usize, Error> {
    let budget = Budget::new(limits);
    let size = postcard::experimental::serialized_size(&encode::Bounded {
        value,
        budget: &budget,
        depth: 1,
    })
    .map_err(|_| budget.error())?;
    if size > limits.bytes {
        return Err(Error::Limit);
    }
    Ok(size)
}

/// Encode an owned or borrowed native payload under explicit traversal allowances.
pub fn encode<T: Serialize + ?Sized>(value: &T, limits: Limits) -> Result<Vec<u8>, Error> {
    encode_prefixed(value, limits, &[])
}
fn encode_prefixed<T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    prefix: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    encode_prefixed_into(value, limits, prefix, &mut bytes)?;
    Ok(bytes)
}

/// Encode into reusable caller storage. Previous contents are cleared, including on error.
/// Capacity may be lost on serialization failure; successful calls retain it.
/// Retain a separate immutable frame when bytes must survive subsequent calls for retries.
pub fn encode_into<T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    bytes: &mut Vec<u8>,
) -> Result<(), Error> {
    encode_prefixed_into(value, limits, &[], bytes)
}

/// Encode a payload into caller-owned storage without allocating an output buffer.
///
/// The returned slice borrows only `storage`, not `value`. Admission and sizing
/// retain the same two-pass traversal and conservative allocation accounting as
/// [`encode`]; custom serializers can still allocate independently.
/// Insufficient storage returns [`Error::Limit`] before modifying it. That error
/// also covers ordinary resource admission, so it does not by itself establish
/// that retrying with a larger buffer will succeed.
/// Serialization failure can leave partial bytes in storage; publish only the
/// slice returned by a successful call. Bytes outside that slice are not output.
pub fn encode_to_slice<'out, T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    storage: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    encode_prefixed_to_slice(value, limits, &[], storage)
}

/// Encode a payload and lend complete bytes to a synchronous consumer.
///
/// Sizing selects caller scratch or an owned buffer once, before serialization;
/// large values do not pass through a per-byte inline/spill adapter. Both paths
/// keep [`encode`]'s two-pass admission. `value` is dropped before `consume`,
/// which is never invoked after an encoding failure. Scratch contents after an
/// error are unspecified. Custom serializers retain their ordinary trust boundary.
///
/// The callback's bytes cannot escape, including in a returned future:
/// ```compile_fail
/// let limits = ic_wire::Limits { bytes: 128, allocation: 1024, nodes: 64, depth: 8 };
/// let mut scratch = [0; 128];
/// let borrowed = ic_wire::native::with_encoded(7u64, limits, &mut scratch, |bytes| bytes);
/// ```
pub fn with_encoded<T: Serialize, R>(
    value: T,
    limits: Limits,
    scratch: &mut [u8],
    consume: impl FnOnce(&[u8]) -> R,
) -> Result<R, Error> {
    let size = encoded_len(&value, limits)?;
    let budget = Budget::new(limits);
    if let Some(storage) = scratch.get_mut(..size) {
        let bytes = postcard::serialize_with_flavor(
            &encode::Bounded {
                value: &value,
                budget: &budget,
                depth: 1,
            },
            postcard::ser_flavors::Slice::new(storage),
        )
        .map_err(|_| budget.error())?;
        drop(value);
        Ok(consume(bytes))
    } else {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| Error::Allocation)?;
        let bytes = postcard::serialize_with_flavor(
            &encode::Bounded {
                value: &value,
                budget: &budget,
                depth: 1,
            },
            contract::Output { bytes, limit: size },
        )
        .map_err(|_| budget.error())?;
        drop(value);
        Ok(consume(&bytes))
    }
}

/// Encode a request into caller storage, including its 11-byte identity envelope.
///
/// Follows [`encode_to_slice`] admission, ownership and failure rules.
/// `limits.bytes` includes the header.
pub fn encode_request_to_slice<'out, T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    storage: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    encode_prefixed_to_slice(value, limits, &contract::header(id, 0), storage)
}

/// Encode a response into caller storage, including its 11-byte identity envelope.
///
/// Follows [`encode_to_slice`] admission, ownership and failure rules.
/// `limits.bytes` includes the header.
pub fn encode_response_to_slice<'out, T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    storage: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    encode_prefixed_to_slice(value, limits, &contract::header(id, 1), storage)
}

fn encode_prefixed_to_slice<'out, T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    prefix: &[u8],
    storage: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    let size = encoded_len(value, payload_limits(limits, prefix.len())?)?;
    let total = size.checked_add(prefix.len()).ok_or(Error::Limit)?;
    let output = storage.get_mut(..total).ok_or(Error::Limit)?;
    let (header, payload) = output.split_at_mut(prefix.len());
    header.copy_from_slice(prefix);
    let budget = Budget::new(limits);
    // This adds a storage destination, not an encoding or admission rule. Limit
    // the second pass to its admitted size even when the caller offers more:
    // a changing/custom Serialize implementation must not bypass preflight.
    let written = postcard::serialize_with_flavor(
        &encode::Bounded {
            value,
            budget: &budget,
            depth: 1,
        },
        postcard::ser_flavors::Slice::new(payload),
    )
    .map_err(|_| budget.error())?
    .len();
    Ok(&output[..prefix.len() + written])
}

fn encode_prefixed_into<T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    prefix: &[u8],
    bytes: &mut Vec<u8>,
) -> Result<(), Error> {
    bytes.clear();
    let size = encoded_len(value, payload_limits(limits, prefix.len())?)?;
    bytes
        .try_reserve_exact(size + prefix.len())
        .map_err(|_| Error::Allocation)?;
    bytes.extend_from_slice(prefix);
    let budget = Budget::new(limits);
    *bytes = postcard::serialize_with_flavor(
        &encode::Bounded {
            value,
            budget: &budget,
            depth: 1,
        },
        contract::Output {
            bytes: core::mem::take(bytes),
            limit: size + prefix.len(),
        },
    )
    .map_err(|_| budget.error())?;
    Ok(())
}

/// Encode a request into reusable storage; follows [`encode_into`] failure semantics.
pub fn encode_request_into<T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    bytes: &mut Vec<u8>,
) -> Result<(), Error> {
    encode_prefixed_into(value, limits, &contract::header(id, 0), bytes)
}

/// Encode a response into reusable storage; follows [`encode_into`] failure semantics.
pub fn encode_response_into<T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    bytes: &mut Vec<u8>,
) -> Result<(), Error> {
    encode_prefixed_into(value, limits, &contract::header(id, 1), bytes)
}

/// Decode a complete Postcard payload; allocation admission precedes visitor calls.
/// Borrowed bytes and strings may refer directly to the input for its lifetime.
/// Primitive representations are checked while reading; Serde normalization is consumer-owned.
pub fn decode<'de, T: Deserialize<'de>>(bytes: &'de [u8], limits: Limits) -> Result<T, Error> {
    if bytes.len() > limits.bytes {
        return Err(Error::Limit);
    }
    let budget = Budget::new(limits);
    let mut input = parser::Deserializer::from_bytes(bytes);
    let value = T::deserialize(decode::Bounded {
        inner: &mut input,
        budget: &budget,
        depth: 1,
    })
    .map_err(|_| budget.error())?;
    if !input.finalize().map_err(|_| Error::Malformed)?.is_empty() {
        return Err(Error::Trailing);
    }
    // Hard cut: decoding no longer reserializes or charges a serialization pass.
    // Primitive parsing stays strict, but maps/sets and custom Serde may normalize:
    // e.g. map bytes 02 02 14 01 0a now decode and re-encode as 02 01 0a 02 14.
    // Consumers own normalization and must retain original bytes for exact retries.
    Ok(value)
}

/// Encode a complete request; `limits.bytes` includes the canonical frame header.
pub fn encode_request<T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    encode_prefixed(value, limits, &contract::header(id, 0))
}
/// Encode a complete response, including borrowed response projections.
pub fn encode_response<T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    encode_prefixed(value, limits, &contract::header(id, 1))
}
/// Measure a request or response frame; both directions have equal header length.
pub fn encoded_frame_len<T: Serialize + ?Sized>(value: &T, limits: Limits) -> Result<usize, Error> {
    encoded_len(value, payload_limits(limits, contract::HEADER)?)
        .map(|size| size + contract::HEADER)
}
/// Check request identity and direction before decoding its bounded native value.
pub fn decode_request<'de, T: Deserialize<'de>>(
    id: u64,
    bytes: &'de [u8],
    limits: Limits,
) -> Result<T, Error> {
    decode_frame(id, 0, bytes, limits)
}
/// Check response identity and direction before decoding its bounded native value.
pub fn decode_response<'de, T: Deserialize<'de>>(
    id: u64,
    bytes: &'de [u8],
    limits: Limits,
) -> Result<T, Error> {
    decode_frame(id, 1, bytes, limits)
}
fn decode_frame<'de, T: Deserialize<'de>>(
    id: u64,
    direction: u8,
    bytes: &'de [u8],
    limits: Limits,
) -> Result<T, Error> {
    if bytes.len() > limits.bytes {
        return Err(Error::Limit);
    }
    let payload = contract::payload(id, direction, bytes)?;
    decode(payload, payload_limits(limits, contract::HEADER)?)
}

fn payload_limits(limits: Limits, prefix: usize) -> Result<Limits, Error> {
    Ok(Limits {
        bytes: limits.bytes.checked_sub(prefix).ok_or(Error::Limit)?,
        allocation: limits.allocation.checked_sub(prefix).ok_or(Error::Limit)?,
        ..limits
    })
}
