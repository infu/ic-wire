//! Shared preflight and destination-specific bounded Postcard output.

mod admitted;

use crate::{Direction, Encoding, Error, Limits, framing::HEADER_BYTES, meter::Budget};
use alloc::vec::Vec;
use postcard::ser_flavors::Flavor;
use serde::Serialize;

fn preflight<T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> Result<(Option<[u8; HEADER_BYTES]>, usize), Error> {
    let header = frame_header(encoding);
    let prefix = if header.is_some() { HEADER_BYTES } else { 0 };
    let payload_limits = limits.without_prefix(prefix)?;
    let budget = Budget::new(payload_limits);
    let size = measure_payload(value, &budget, 1)?;
    if size > payload_limits.bytes {
        return Err(Error::Limit(crate::LimitKind::Bytes));
    }

    // The byte allowance was reduced by prefix before sizing, so addition fits.
    Ok((header, size + prefix))
}

pub(super) fn frame_header(encoding: Encoding) -> Option<[u8; HEADER_BYTES]> {
    match encoding {
        Encoding::Payload => None,
        Encoding::Frame { method, direction } => {
            let mut header = [0; HEADER_BYTES];
            header[..2].copy_from_slice(b"IW");
            header[2..10].copy_from_slice(&method.to_le_bytes());
            header[10] = match direction {
                Direction::Request => 0,
                Direction::Response => 1,
            };
            Some(header)
        }
    }
}

pub(super) fn measure_payload<T: Serialize + ?Sized>(
    value: &T,
    budget: &Budget,
    depth: usize,
) -> Result<usize, Error> {
    postcard::experimental::serialized_size(&admitted::Bounded {
        value,
        budget,
        depth,
    })
    .map_err(|_| budget.error())
}

/// Measure the complete selected encoding without allocating codec output.
///
/// Custom Serialize code is trusted and must be deterministic across calls.
/// This does not return a reusable admission token.
///
/// # Errors
/// Returns Limit for exhausted allowances, otherwise Malformed for serializer
/// failure. Framed lengths include the 11-byte header.
pub fn encoded_len<T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> Result<usize, Error> {
    preflight(value, encoding, limits).map(|(_, size)| size)
}

/// Encode into owned storage after a complete sizing/admission pass.
///
/// # Errors
/// Returns the same errors as [`encode_into`]. No partial output is returned.
#[cfg(test)]
pub fn encode<T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    encode_into(value, encoding, limits, &mut output)?;

    Ok(output)
}

/// Encode into reusable storage, returning only the completed prefix.
///
/// Prior contents are cleared, including on error; capacity retention on failure
/// is not promised. Keep retained retry bytes elsewhere. Preflight completes
/// before codec output reservation. Custom serializers must be deterministic;
/// writes remain bounded by preflight length even if a second pass grows.
///
/// # Errors
/// Returns Limit for exhausted allowances, Allocation for failed output
/// reservation, otherwise Malformed for serialization failure.
#[cfg(test)]
pub fn encode_into<'out, T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
    output: &'out mut Vec<u8>,
) -> Result<&'out [u8], Error> {
    output.clear();
    let (header, size) = preflight(value, encoding, limits)?;
    write_vec(value, &Budget::new(limits), header, size, output)?;
    Ok(output)
}

pub(super) fn write_vec<T: Serialize + ?Sized>(
    value: &T,
    budget: &Budget,
    header: Option<[u8; HEADER_BYTES]>,
    size: usize,
    output: &mut Vec<u8>,
) -> Result<(), Error> {
    output
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    if let Some(header) = header {
        output.extend_from_slice(&header);
    }
    *output = postcard::serialize_with_flavor(
        &admitted::Bounded {
            value,
            budget,
            depth: 1,
        },
        Output {
            bytes: core::mem::take(output),
            limit: size,
        },
    )
    .map_err(|_| budget.error())?;
    Ok(())
}

/// Encode into caller storage without allocating codec output.
///
/// A short destination is rejected before writing. Other failures may modify
/// storage; only a successful returned slice may be published. Custom Serialize
/// work is trusted and may allocate independently. Caller capacity does not
/// reduce the configured allocation allowance.
///
/// # Errors
/// Returns Limit for exhausted allowances or a short destination, otherwise
/// Malformed for serialization failure (including growth between passes).
#[cfg(test)]
pub fn encode_to_slice<'out, T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
    output: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    let (header, size) = preflight(value, encoding, limits)?;
    let output = output
        .get_mut(..size)
        .ok_or(Error::Limit(crate::LimitKind::Output))?;
    let prefix = if let Some(header) = header {
        output[..HEADER_BYTES].copy_from_slice(&header);

        HEADER_BYTES
    } else {
        0
    };
    let written = prefix + write_payload(value, &Budget::new(limits), 1, &mut output[prefix..])?;

    Ok(&output[..written])
}

#[cfg(test)]
pub(super) fn write_payload<T: Serialize + ?Sized>(
    value: &T,
    budget: &Budget,
    depth: usize,
    output: &mut [u8],
) -> Result<usize, Error> {
    postcard::to_slice(
        &admitted::Bounded {
            value,
            budget,
            depth,
        },
        output,
    )
    .map(|payload| payload.len())
    .map_err(|_| budget.error())
}

pub(super) fn write_scratch<T: Serialize + ?Sized>(
    value: &T,
    budget: &Budget,
    depth: usize,
    output: &mut [u8],
    capacity_kind: crate::LimitKind,
) -> Result<usize, Error> {
    write_bounded_slice::<T, true>(value, budget, depth, output, capacity_kind)
}

pub(super) fn write_bounded_slice<T: Serialize + ?Sized, const CHARGE_SCRATCH: bool>(
    value: &T,
    budget: &Budget,
    depth: usize,
    output: &mut [u8],
    capacity_kind: crate::LimitKind,
) -> Result<usize, Error> {
    let initial_len = output.len();
    let result = postcard::serialize_with_flavor(
        &admitted::Bounded {
            value,
            budget,
            depth,
        },
        BoundedSlice::<CHARGE_SCRATCH> {
            output,
            budget,
            capacity_kind,
            initial_len,
        },
    );
    budget.check()?;
    result.map_err(|_| Error::Malformed)
}

// Final output uses the meter's configured byte reservation. Additional
// staging writes debit the remaining allocation allowance.
struct BoundedSlice<'a, 'budget, const CHARGE_SCRATCH: bool> {
    output: &'a mut [u8],
    budget: &'budget Budget,
    capacity_kind: crate::LimitKind,
    initial_len: usize,
}

impl<const CHARGE_SCRATCH: bool> BoundedSlice<'_, '_, CHARGE_SCRATCH> {
    #[inline]
    fn take(&mut self, len: usize) -> postcard::Result<&mut [u8]> {
        let Some((head, rest)) = core::mem::take(&mut self.output).split_at_mut_checked(len) else {
            self.budget.refuse_with(self.capacity_kind);
            return Err(postcard::Error::SerializeBufferFull);
        };
        if CHARGE_SCRATCH && !self.budget.charge_buffer(len) {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.output = rest;
        Ok(head)
    }
}

impl<const CHARGE_SCRATCH: bool> Flavor for BoundedSlice<'_, '_, CHARGE_SCRATCH> {
    type Output = usize;

    #[inline]
    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        self.take(1)?[0] = byte;
        Ok(())
    }

    #[inline]
    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        self.take(bytes.len())?.copy_from_slice(bytes);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<Self::Output> {
        Ok(self.initial_len - self.output.len())
    }
}

// Allocation capacity is not admission: every write is capped at preflight size.
struct Output {
    bytes: Vec<u8>,
    limit: usize,
}

impl Flavor for Output {
    type Output = Vec<u8>;

    fn try_push(&mut self, byte: u8) -> postcard::Result<()> {
        if self.bytes.len() == self.limit {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.bytes.push(byte);

        Ok(())
    }

    fn try_extend(&mut self, bytes: &[u8]) -> postcard::Result<()> {
        if bytes.len() > self.limit - self.bytes.len() {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.bytes.extend_from_slice(bytes);

        Ok(())
    }

    fn finalize(self) -> postcard::Result<Self::Output> {
        Ok(self.bytes)
    }
}
