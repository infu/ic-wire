//! One bounded Postcard decoding path for payloads and frames.

mod admitted;
#[cfg(test)]
mod tests;

use crate::{
    Encoding, Error, Limits,
    framing::{HEADER_BYTES, validate_header},
    meter::Budget,
};
use serde::Deserialize;

/// Decode exactly one payload or frame under explicit traversal allowances.
///
/// Borrowed results may reference the original input. Postcard owns primitive
/// acceptance; this does not enforce canonical bytes or reserialize results.
/// Consumer collections and custom Serde may normalize values. Retain original
/// bytes for signatures, hashes and retries. Custom Serde work outside admitted
/// visits is trusted; a valid prefix can allocate before a later failure.
///
/// # Errors
///
/// Oversized input returns [`Error::Limit`] before header checks. Short frames
/// return [`Error::Malformed`], then header mismatches return [`Error::Identity`].
/// During payload decoding exhausted allowances return [`Error::Limit`], other
/// deserialization failures return [`Error::Malformed`]. A successful value
/// followed by unused input returns [`Error::Trailing`].
#[cfg(test)]
pub fn decode<'de, T: Deserialize<'de>>(
    input: &'de [u8],
    encoding: Encoding,
    limits: Limits,
) -> Result<T, Error> {
    let (payload, limits) = admit_input(input, encoding, limits)?;
    decode_payload(payload, &Budget::new(limits), 1)
}

pub(super) fn admit_input(
    input: &[u8],
    encoding: Encoding,
    mut limits: Limits,
) -> Result<(&[u8], Limits), Error> {
    if input.len() > limits.bytes {
        return Err(Error::Limit(crate::LimitKind::Bytes));
    }
    let payload = match encoding {
        Encoding::Payload => input,
        Encoding::Frame { method, direction } => {
            let (header, payload) = input
                .split_first_chunk::<HEADER_BYTES>()
                .ok_or(Error::Malformed)?;
            validate_header(method, direction, header)?;
            limits = limits.without_prefix(HEADER_BYTES)?;

            payload
        }
    };
    Ok((payload, limits))
}

pub(super) fn decode_payload<'de, T: Deserialize<'de>>(
    input: &'de [u8],
    budget: &Budget,
    depth: usize,
) -> Result<T, Error> {
    let (value, rest) = take_payload(input, budget, depth)?;
    if !rest.is_empty() {
        return Err(Error::Trailing);
    }
    Ok(value)
}

pub(super) fn take_payload<'de, T: Deserialize<'de>>(
    payload: &'de [u8],
    budget: &Budget,
    depth: usize,
) -> Result<(T, &'de [u8]), Error> {
    let mut deserializer = postcard::Deserializer::from_bytes(payload);
    let value = T::deserialize(admitted::Bounded {
        inner: &mut deserializer,
        budget,
        depth,
    })
    .map_err(|_| budget.error())?;
    let rest = deserializer.finalize().map_err(|_| Error::Malformed)?;
    Ok((value, rest))
}
