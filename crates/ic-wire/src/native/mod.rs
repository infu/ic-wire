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
    encode_prefixed_into(value, limits, prefix, &mut bytes, None)?;
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
    encode_prefixed_into(value, limits, &[], bytes, None)
}

/// Encode a payload into caller-owned storage without allocating codec output.
///
/// Returns only the completed encoding. A short buffer returns [`Error::Limit`]
/// before writing; on any other error storage contents are unspecified and must
/// not be published. The returned slice borrows storage, not the value. Admission,
/// two-pass sizing and trusted custom Serde rules match [`encode_into`]; storage
/// capacity does not replace or reduce the configured allocation allowance.
pub fn encode_to_slice<'a, T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    storage: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    encode_prefixed_to_slice(value, limits, &[], storage)
}

/// Encode a payload using scratch storage or owned fallback, then consume its bytes.
///
/// Preflight selects a destination once: scratch when the admitted payload fits,
/// otherwise an owned buffer. Both paths retain [`encode`]'s two-pass admission
/// and allocation allowance; custom serializers may still allocate independently.
/// The owned `value` is dropped before the synchronous callback. Passing a
/// reference instead keeps ownership with the caller.
///
/// Encoding failure never calls `consume`; scratch contents on error are
/// unspecified. Only successful bytes may be published. Copy bytes that must
/// survive the callback, including saved retries. No framing is added.
///
/// ```
/// let limits = ic_wire::Limits { bytes: 128, allocation: 2048, nodes: 64, depth: 8 };
/// let mut scratch = [0; 128];
/// let length = ic_wire::native::with_encoded(7_u64, limits, &mut scratch, |bytes| {
///     assert_eq!(bytes, [7]);
///     bytes.len()
/// })?;
/// assert_eq!(length, 1);
/// # Ok::<(), ic_wire::Error>(())
/// ```
/// Callback bytes cannot escape as a borrow or inside a future:
/// ```compile_fail
/// let limits = ic_wire::Limits { bytes: 128, allocation: 2048, nodes: 64, depth: 8 };
/// let mut scratch = [0; 128];
/// ic_wire::native::with_encoded(7_u64, limits, &mut scratch, |bytes| bytes);
/// ```
/// ```compile_fail
/// let limits = ic_wire::Limits { bytes: 128, allocation: 2048, nodes: 64, depth: 8 };
/// let mut scratch = [0; 128];
/// ic_wire::native::with_encoded(7_u64, limits, &mut scratch, |bytes| async move {
///     bytes.len()
/// });
/// ```
pub fn with_encoded<T: Serialize, R>(
    value: T,
    limits: Limits,
    scratch: &mut [u8],
    consume: impl FnOnce(&[u8]) -> R,
) -> Result<R, Error> {
    let size = encoded_len(&value, limits)?;
    // New destination selection only: both writers are capped at the preflight
    // size. Postcard bytes and admission match encode; e.g. u64 7 remains 07.
    // Existing endpoints/retries need no migration or alternate codec.
    if let Some(storage) = scratch.get_mut(..size) {
        let bytes = encode_sized_to_slice(&value, limits, storage)?;
        drop(value);
        Ok(consume(bytes))
    } else {
        let mut bytes = Vec::new();
        encode_prefixed_into(&value, limits, &[], &mut bytes, Some(size))?;
        drop(value);
        Ok(consume(&bytes))
    }
}

/// Encode a complete request into caller storage; follows [`encode_to_slice`].
pub fn encode_request_to_slice<'a, T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    storage: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    encode_prefixed_to_slice(value, limits, &contract::header(id, 0), storage)
}

/// Encode a complete response into caller storage; follows [`encode_to_slice`].
pub fn encode_response_to_slice<'a, T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    storage: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    encode_prefixed_to_slice(value, limits, &contract::header(id, 1), storage)
}

fn encode_prefixed_to_slice<'a, T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    prefix: &[u8],
    storage: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    let size = encoded_len(value, payload_limits(limits, prefix.len())?)?;
    let output = storage.get_mut(..size + prefix.len()).ok_or(Error::Limit)?;
    output[..prefix.len()].copy_from_slice(prefix);
    // New storage API, same Postcard bytes and admission as Vec output. The
    // slice flavor bounds every write, including inconsistent custom Serialize
    // passes. Only Ok publishes a completed prefix; no inline/spill policy or
    // framing change is introduced, and existing consumers need no migration.
    let payload = encode_sized_to_slice(value, limits, &mut output[prefix.len()..])?;
    let written = prefix.len() + payload.len();
    Ok(&mut output[..written])
}

#[cfg_attr(target_arch = "wasm32", inline)]
fn encode_sized_to_slice<'a, T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    storage: &'a mut [u8],
) -> Result<&'a mut [u8], Error> {
    // Both callers trimmed storage to the size admitted on this value. Keep a
    // fresh writing budget beside its serializer and share one slice pass.
    // Bytes, admission and partial-failure behavior remain unchanged.
    let budget = Budget::new(limits);
    postcard::to_slice(
        &encode::Bounded {
            value,
            budget: &budget,
            depth: 1,
        },
        storage,
    )
    .map_err(|_| budget.error())
}

fn encode_prefixed_into<T: Serialize + ?Sized>(
    value: &T,
    limits: Limits,
    prefix: &[u8],
    bytes: &mut Vec<u8>,
    admitted_size: Option<usize>,
) -> Result<(), Error> {
    bytes.clear();
    // Keep sizing beside writing for existing callers. Scoped heap output is
    // the sole Some caller: an empty prefix, and encoded_len on the same value
    // and limits. This reuses admission without adding a third Serialize visit.
    let size = match admitted_size {
        Some(size) => size,
        None => encoded_len(value, payload_limits(limits, prefix.len())?)?,
    };
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
    encode_prefixed_into(value, limits, &contract::header(id, 0), bytes, None)
}

/// Encode a response into reusable storage; follows [`encode_into`] failure semantics.
pub fn encode_response_into<T: Serialize + ?Sized>(
    id: u64,
    value: &T,
    limits: Limits,
    bytes: &mut Vec<u8>,
) -> Result<(), Error> {
    encode_prefixed_into(value, limits, &contract::header(id, 1), bytes, None)
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
