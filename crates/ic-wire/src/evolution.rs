//! Sparse extension fields, open variants and bounded message projections.
//!
//! The default `Wire` derive stages and projects typed extension records and
//! tuples through supported containers. Applications can also project open
//! cases and recovery envelopes explicitly. Projected calls share Wire's
//! admission across the complete Serde value and typed envelope contents.

mod declaration;
#[doc(hidden)]
pub use declaration::__private;
pub(crate) mod method;
#[cfg(feature = "derive")]
pub use declaration::Evolve;
#[cfg(feature = "typescript")]
pub use declaration::{ExtensionLayout, OpenLayout, SchemaFields};
pub use declaration::{ExtensionSlot, OpenSlot, RecoverSlot};
pub use method::{Evolving, Method};
#[doc(hidden)]
pub mod inline;
#[cfg(feature = "derive")]
#[doc(hidden)]
pub use serde as __serde;

use crate::{
    Encoding, Error, LimitKind, Limits, decode, encode, framing::HEADER_BYTES, meter::Budget,
};
use alloc::vec::Vec;
use core::fmt;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, SeqAccess, Visitor},
    ser::SerializeTuple,
};
use serde_bytes::Bytes;

mod recovery;
pub use recovery::{Recoverable, RecoveryContext};

/// A manually projected message with a positional core and a sparse root block.
pub struct Projected<'a, T> {
    /// Positional fields in their declared order.
    pub core: T,
    /// Staged sparse fields at the root of the message.
    pub extensions: Extensions<'a>,
}

impl<'a, T> Projected<'a, T> {
    /// Assemble a message with its staged root fields.
    pub const fn new(core: T, extensions: Extensions<'a>) -> Self {
        Self { core, extensions }
    }

    /// Assemble a message whose writer supplies no root extensions.
    pub const fn empty(core: T) -> Self {
        Self::new(core, Extensions::empty())
    }
}

impl<T: Serialize> Serialize for Projected<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (&self.core, self.extensions).serialize(serializer)
    }
}

impl<'de: 'a, 'a, T: Deserialize<'de>> Deserialize<'de> for Projected<'a, T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (core, extensions) = <(T, Extensions<'a>)>::deserialize(deserializer)?;
        Ok(Self::new(core, extensions))
    }
}

/// Decode the outer Serde DTO and project its envelopes under one budget.
///
/// The application owns the projection, including required-field checks and
/// visiting the root extension block and any nested envelopes. Keep custom
/// Serde and application work bounded independently.
///
/// # Errors
/// Returns an error from frame admission, DTO decoding, projection, or any
/// refused operation even if the application ignores that refusal.
pub fn decode_projected<'de, T: Deserialize<'de>, R>(
    input: &'de [u8],
    encoding: Encoding,
    limits: Limits,
    project: impl FnOnce(Projected<'de, T>, &ReadContext<'_>) -> Result<R, Error>,
) -> Result<R, Error> {
    decode_value(input, encoding, limits, project)
}

pub(crate) fn decode_value<'de, T: Deserialize<'de>, R>(
    input: &'de [u8],
    encoding: Encoding,
    limits: Limits,
    project: impl FnOnce(T, &ReadContext<'_>) -> Result<R, Error>,
) -> Result<R, Error> {
    let (payload, limits) = decode::admit_input(input, encoding, limits)?;
    let budget = Budget::new(limits);
    let dto = decode::decode_payload(payload, &budget, 1);
    budget.check()?;
    let result = project(
        dto?,
        &ReadContext {
            budget: &budget,
            depth: 1,
        },
    );
    budget.check()?;
    result
}

/// Prepare envelopes in caller-owned scratch space and encode the resulting DTO.
///
/// Envelope preparation, DTO sizing and DTO writing each debit the same meter.
/// Returns the final slice in reusable output storage.
///
/// # Errors
/// Clears the output on any failure. A failed preparation may modify its own
/// scratch storage. Custom serializers must agree on their size across passes.
pub fn encode_projected_into<'out, 'value, T: Serialize>(
    output: &'out mut Vec<u8>,
    encoding: Encoding,
    limits: Limits,
    prepare: impl FnOnce(&WriteContext<'_>) -> Result<Projected<'value, T>, Error>,
) -> Result<&'out [u8], Error> {
    encode_value_into(output, encoding, limits, prepare)
}

pub(crate) fn encode_value_into<T: Serialize>(
    output: &mut Vec<u8>,
    encoding: Encoding,
    limits: Limits,
    prepare: impl FnOnce(&WriteContext<'_>) -> Result<T, Error>,
) -> Result<&[u8], Error> {
    encode_value_into_with(output, encoding, limits, |output, header, context| {
        let value = prepare(context);
        context.budget.check()?;
        write_value_into(output, header, context, &value?)
    })
}

fn encode_value_into_with(
    output: &mut Vec<u8>,
    encoding: Encoding,
    limits: Limits,
    write: impl FnOnce(&mut Vec<u8>, Option<[u8; HEADER_BYTES]>, &WriteContext<'_>) -> Result<(), Error>,
) -> Result<&[u8], Error> {
    output.clear();
    let header = encode::frame_header(encoding);
    let prefix = if header.is_some() { HEADER_BYTES } else { 0 };
    let limits = limits.without_prefix(prefix)?;
    let budget = Budget::new(limits);
    budget.check()?;
    let context = WriteContext {
        budget: &budget,
        depth: 1,
        byte_limit: limits.bytes,
    };
    let result = write(output, header, &context);
    if let Err(error) = budget.check().and(result) {
        output.clear();
        return Err(error);
    }
    Ok(output)
}

fn write_value_into<T: Serialize>(
    output: &mut Vec<u8>,
    header: Option<[u8; HEADER_BYTES]>,
    context: &WriteContext<'_>,
    value: &T,
) -> Result<(), Error> {
    let budget = context.budget;
    let size = encode::measure_payload(value, budget, 1)?;
    budget.check()?;
    if size > context.byte_limit {
        return Err(Error::Limit(LimitKind::Bytes));
    }
    let prefix = if header.is_some() { HEADER_BYTES } else { 0 };
    let len = size + prefix;
    encode::write_vec(value, budget, header, len, output)?;
    if output.len() != len {
        return Err(Error::Malformed);
    }
    Ok(())
}

/// Measure an already projected message, including its root block and any frame header.
/// Preparation of the projected fields uses the caller's staging allowance.
///
/// # Errors
/// Returns a sizing, serialization or allowance error.
pub fn projected_len<T: Serialize>(
    value: &Projected<'_, T>,
    encoding: Encoding,
    limits: Limits,
) -> Result<usize, Error> {
    encode::encoded_len(value, encoding, limits)
}

/// Prepare envelopes and encode the DTO once into caller-owned output.
///
/// The destination is already allocated. Envelope preparation and DTO
/// serialization share one resource meter. Only the successful returned
/// prefix contains a complete encoding.
///
/// # Errors
/// Returns a preparation, serialization or allowance error. A short
/// destination returns `Limit(Output)`. A failed operation may modify the
/// caller's output and scratch.
pub fn encode_projected_to_slice<'out, 'value, T: Serialize>(
    output: &'out mut [u8],
    encoding: Encoding,
    limits: Limits,
    prepare: impl FnOnce(&WriteContext<'_>) -> Result<Projected<'value, T>, Error>,
) -> Result<&'out [u8], Error> {
    encode_value_to_slice(output, encoding, limits, prepare)
}

pub(crate) fn encode_value_to_slice<T: Serialize>(
    output: &mut [u8],
    encoding: Encoding,
    limits: Limits,
    prepare: impl FnOnce(&WriteContext<'_>) -> Result<T, Error>,
) -> Result<&[u8], Error> {
    encode_value_to_slice_with(output, encoding, limits, |mut destination| {
        let context = destination.context();
        let dto = prepare(&context);
        context.budget.check()?;
        destination.write_value(&dto?)
    })
}

/// Encode a DTO that borrows its projection context into caller-owned output.
///
/// The callback receives the destination for one final write. Use its context
/// to prepare envelopes while Serde visits the DTO, for example when staging
/// one sequence element at a time. Its output becomes visible only after the
/// callback and the shared resource checks succeed.
///
/// # Errors
/// Returns a callback, serialization or allowance error. Success requires
/// one completed write. A failed callback or serialization may modify the
/// caller's output and scratch.
pub fn encode_projected_to_slice_with(
    output: &mut [u8],
    encoding: Encoding,
    limits: Limits,
    write: impl FnOnce(ProjectedOutput<'_>) -> Result<(), Error>,
) -> Result<&[u8], Error> {
    encode_value_to_slice_with(output, encoding, limits, write)
}

pub(crate) fn encode_value_to_slice_with(
    output: &mut [u8],
    encoding: Encoding,
    limits: Limits,
    write: impl FnOnce(ProjectedOutput<'_>) -> Result<(), Error>,
) -> Result<&[u8], Error> {
    let header = encode::frame_header(encoding);
    let prefix = if header.is_some() { HEADER_BYTES } else { 0 };
    let limits = limits.without_prefix(prefix)?;
    let budget = Budget::new(limits);
    budget.check()?;
    let capacity = output
        .get_mut(prefix..)
        .ok_or(Error::Limit(LimitKind::Output))?;
    let capacity_kind = if capacity.len() < limits.bytes {
        LimitKind::Output
    } else {
        LimitKind::Bytes
    };
    let cap = capacity.len().min(limits.bytes);
    let mut written = None;
    let result = write(ProjectedOutput {
        output: &mut capacity[..cap],
        context: WriteContext {
            budget: &budget,
            depth: 1,
            byte_limit: limits.bytes,
        },
        capacity_kind,
        written: &mut written,
    });
    budget.check()?;
    result?;
    let size = written.ok_or(Error::Malformed)?;
    if let Some(header) = header {
        output[..prefix].copy_from_slice(&header);
    }
    Ok(&output[..prefix + size])
}

/// The caller's destination for one projected Serde write.
pub struct ProjectedOutput<'scope> {
    output: &'scope mut [u8],
    context: WriteContext<'scope>,
    capacity_kind: LimitKind,
    written: &'scope mut Option<usize>,
}

impl<'scope> ProjectedOutput<'scope> {
    /// Borrow the meter for envelope preparation, including during traversal.
    #[must_use]
    pub const fn context(&self) -> WriteContext<'scope> {
        WriteContext {
            budget: self.context.budget,
            depth: self.context.depth,
            byte_limit: self.context.byte_limit,
        }
    }

    /// Consume this destination and serialize the DTO in the current scope.
    ///
    /// # Errors
    /// Returns a serialization or allowance error. The destination's bytes
    /// are available only from the successful outer operation.
    pub fn write<T: Serialize>(mut self, value: &Projected<'_, T>) -> Result<(), Error> {
        self.write_value(value)
    }

    pub(crate) fn write_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let size = encode::write_bounded_slice::<T, false>(
            value,
            self.context.budget,
            self.context.depth,
            self.output,
            self.capacity_kind,
        )?;
        *self.written = Some(size);
        Ok(())
    }
}

/// Admission context for one projection. It borrows the outer decode's meter.
pub struct ReadContext<'a> {
    budget: &'a Budget,
    depth: usize,
}

impl ReadContext<'_> {
    /// Run application projection at one greater depth on the same meter.
    ///
    /// # Errors
    /// Returns an allowance refusal or the callback error.
    pub fn nested<R>(
        &self,
        project: impl FnOnce(&ReadContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let child = self.child();
        admit_scope(child.budget, child.depth)?;
        project(&child)
    }

    /// Charge storage for values the application will materialize.
    ///
    /// # Errors
    /// Returns an allocation or traversal allowance refusal.
    pub fn admit_allocation<T>(&self, count: usize) -> Result<(), Error> {
        admit_materialization::<T>(self.budget, self.depth, count)
    }

    /// Context for an already admitted structural field.
    #[doc(hidden)]
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            budget: self.budget,
            depth: self.depth.saturating_add(1),
        }
    }
}

/// Admission context for envelope preparation on the outer encoder's meter.
pub struct WriteContext<'a> {
    budget: &'a Budget,
    depth: usize,
    byte_limit: usize,
}

impl WriteContext<'_> {
    /// Run application preparation at one greater depth on the same meter.
    ///
    /// # Errors
    /// Returns an allowance refusal or the callback error.
    pub fn nested<R>(
        &self,
        prepare: impl FnOnce(&WriteContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let child = self.child();
        admit_scope(child.budget, child.depth)?;
        prepare(&child)
    }

    /// Debit a generated preparation step before recursively staging a value.
    ///
    /// # Errors
    /// Returns the first depth or work refusal.
    #[doc(hidden)]
    pub fn admit_preparation(&self) -> Result<(), Error> {
        admit_scope(self.budget, self.depth)
    }

    /// Debit storage before constructing a temporary Serde view.
    ///
    /// # Errors
    /// Returns the first allocation or traversal refusal.
    #[doc(hidden)]
    pub fn admit_allocation<T>(&self, count: usize) -> Result<(), Error> {
        admit_materialization::<T>(self.budget, self.depth, count)
    }

    /// Context for recursively preparing a structural field.
    #[doc(hidden)]
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            budget: self.budget,
            depth: self.depth.saturating_add(1),
            byte_limit: self.byte_limit,
        }
    }

    fn write_once<T: Serialize + ?Sized>(
        &self,
        value: &T,
        scratch: &mut [u8],
    ) -> Result<usize, Error> {
        let byte_limit = self.byte_limit.min(u32::MAX as usize);
        let capacity_kind = if scratch.len() < byte_limit {
            LimitKind::Scratch
        } else {
            LimitKind::Bytes
        };
        let cap = scratch.len().min(byte_limit);
        encode::write_scratch(
            value,
            self.budget,
            self.depth,
            &mut scratch[..cap],
            capacity_kind,
        )
    }
}

fn varint_width(value: u32) -> usize {
    (u32::BITS - value.leading_zeros()).max(1).div_ceil(7) as usize
}

fn admit(budget: &Budget, depth: usize, units: usize) -> Result<(), Error> {
    if budget.enter(depth, units) {
        Ok(())
    } else {
        Err(budget.error())
    }
}

fn admit_materialization<T>(budget: &Budget, depth: usize, count: usize) -> Result<(), Error> {
    let units = size_of::<T>().checked_mul(count).ok_or_else(|| {
        budget.refuse();
        budget.error()
    })?;
    admit(budget, depth, units)
}

fn admit_scope(budget: &Budget, depth: usize) -> Result<(), Error> {
    if budget.enter_scope(depth) {
        Ok(())
    } else {
        Err(budget.error())
    }
}

/// A bounded field or variant payload borrowed from an admitted envelope.
pub struct Field<'meter, 'de> {
    bytes: &'de [u8],
    context: ReadContext<'meter>,
}

impl<'de> Field<'_, 'de> {
    /// Decode the complete payload using stock Postcard on the shared meter.
    ///
    /// # Errors
    /// Returns a limit refusal or malformed payload error. Trailing bytes
    /// within this known field are malformed.
    pub fn decode<T: Deserialize<'de>>(self) -> Result<T, Error> {
        self.project(|value, _| Ok(value))
    }

    /// Decode the typed composition of an extension body.
    ///
    /// # Errors
    /// Returns the first decode, projection or resource failure.
    #[doc(hidden)]
    pub fn project_inline<T: inline::Inline<'de>>(self) -> Result<T, Error> {
        self.project(T::project)
    }

    /// Decode this field and project any nested envelopes at its depth.
    ///
    /// # Errors
    /// Returns a field decode error or an error from projection.
    pub fn project<T: Deserialize<'de>, R>(
        self,
        project: impl FnOnce(T, &ReadContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let value = decode::decode_payload(self.bytes, self.context.budget, self.context.depth)
            .map_err(|error| {
                if error == Error::Trailing {
                    Error::Malformed
                } else {
                    error
                }
            });
        self.context.budget.check()?;
        let result = project(value?, &self.context);
        self.context.budget.check()?;
        result
    }
}

/// An opaque ordered collection of extension fields, represented as Postcard bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extensions<'a>(&'a [u8]);

impl<'a> Extensions<'a> {
    /// Construct the empty extension block.
    #[must_use]
    pub const fn empty() -> Self {
        Self(&[])
    }

    /// Visit all IDs and length-bounded fields in strictly increasing order.
    ///
    /// A callback can ignore the field or consume it with [Field::decode].
    /// Successful visits check even the headers of fields the caller ignores.
    ///
    /// # Errors
    /// Returns malformed for zero, duplicate or descending IDs and invalid
    /// extents. Callback errors and allowance refusals propagate.
    pub fn visit(
        self,
        context: &ReadContext<'_>,
        mut visit: impl FnMut(u32, Field<'_, 'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let block = context.child();
        admit_scope(block.budget, block.depth)?;
        let mut rest = self.0;
        let mut previous = 0;
        while !rest.is_empty() {
            let field_context = block.child();
            admit_scope(field_context.budget, field_context.depth)?;
            let (id, bytes, remaining) = extension_header(rest)?;
            if id <= previous {
                return Err(Error::Malformed);
            }
            visit(
                id,
                Field {
                    bytes,
                    context: field_context,
                },
            )?;
            block.budget.check()?;
            rest = remaining;
            previous = id;
        }
        Ok(())
    }
}

#[inline]
fn extension_header(rest: &[u8]) -> Result<(u32, &[u8], &[u8]), Error> {
    // Single-byte Postcard headers need one bounds check for the field span.
    if let Some(([id, length], tail)) = rest.split_first_chunk::<2>()
        && (id | length) < 128
    {
        let (bytes, remaining) = tail
            .split_at_checked(usize::from(*length))
            .ok_or(Error::Malformed)?;
        return Ok((u32::from(*id), bytes, remaining));
    }
    postcard_extension_header(rest)
}

#[inline(never)]
fn postcard_extension_header(rest: &[u8]) -> Result<(u32, &[u8], &[u8]), Error> {
    // Preserve Postcard's acceptance of multi-byte and redundant encodings.
    let ((id, bytes), remaining) =
        postcard::take_from_bytes::<(u32, &Bytes)>(rest).map_err(|_| Error::Malformed)?;
    Ok((id, bytes.as_ref(), remaining))
}

impl Serialize for Extensions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Extensions<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <&Bytes>::deserialize(deserializer).map(|bytes| Self(bytes.as_ref()))
    }
}

/// Write sorted extension IDs into caller-owned scratch storage.
pub struct ExtensionsWriter<'a> {
    output: &'a mut [u8],
    used: usize,
    previous: u32,
}

impl<'a> ExtensionsWriter<'a> {
    /// Start an empty block in a slice distinct from the final frame output.
    #[must_use]
    pub const fn new(output: &'a mut [u8]) -> Self {
        Self {
            output,
            used: 0,
            previous: 0,
        }
    }

    /// Append a field with a greater, nonzero ID and a typed Postcard payload.
    ///
    /// # Errors
    /// Returns an ordering, serialization, allowance or scratch capacity error.
    /// A failed append may change bytes beyond the last successfully written field.
    pub fn push<T: Serialize + ?Sized>(
        &mut self,
        context: &WriteContext<'_>,
        id: u32,
        value: &T,
    ) -> Result<(), Error> {
        self.push_with(context, id, |leaf, available| {
            leaf.write_once(value, available)
        })
    }

    /// Append a field after preparing its nested values in the free scratch.
    ///
    /// The preparation bytes are reusable after the field is committed. The
    /// scratch must fit both the child view and the resulting field while the
    /// writer is staging it.
    ///
    /// # Errors
    /// Returns the first ordering, projection, serialization or limit refusal.
    #[doc(hidden)]
    pub fn push_projected<'de, T: inline::Inline<'de>>(
        &mut self,
        context: &WriteContext<'_>,
        id: u32,
        value: &T,
    ) -> Result<(), Error> {
        self.push_with(context, id, |leaf, available| {
            let capacity = available.len();
            let (view, tail) = value.prepare(leaf, available)?;
            let staged = capacity.checked_sub(tail.len()).ok_or(Error::Malformed)?;
            let len = leaf.write_once(&view, tail)?;
            drop(view);
            if staged != 0 {
                available.copy_within(staged..staged + len, 0);
            }
            Ok(len)
        })
    }

    fn push_with(
        &mut self,
        context: &WriteContext<'_>,
        id: u32,
        prepare: impl FnOnce(&WriteContext<'_>, &mut [u8]) -> Result<usize, Error>,
    ) -> Result<(), Error> {
        if id <= self.previous {
            return Err(Error::Malformed);
        }
        let mut leaf = context.child().child();
        admit_scope(leaf.budget, leaf.depth)?;
        let bound = leaf.byte_limit.min(u32::MAX as usize);
        let min_prefix = if id < 128 { 2 } else { varint_width(id) + 1 };
        let body_at = self
            .used
            .checked_add(min_prefix)
            .filter(|start| *start <= bound)
            .ok_or_else(|| leaf.budget.refuse_with(LimitKind::Bytes))?;
        let available = self
            .output
            .get_mut(body_at..)
            .ok_or_else(|| leaf.budget.refuse_with(LimitKind::Scratch))?;
        if !leaf.budget.charge_buffer(min_prefix) {
            return Err(leaf.budget.error());
        }
        // Most fields fit a one-byte length. Materialize the payload once
        // after that slot, shifting it only if the length consumes more bytes.
        leaf.byte_limit = bound - body_at;
        let len = prepare(&leaf, available)?;
        self.used = if min_prefix == 2 && len < 128 {
            // The reserved two bytes already fit both single-byte varints.
            self.output[self.used..body_at].copy_from_slice(&[id as u8, len as u8]);
            body_at + len
        } else {
            self.finish_wide_prefix(leaf.budget, bound, id, body_at, len)?
        };
        self.previous = id;
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn finish_wide_prefix(
        &mut self,
        budget: &Budget,
        bound: usize,
        id: u32,
        body_at: usize,
        len: usize,
    ) -> Result<usize, Error> {
        let mut end = body_at + len;
        if len >= 128 {
            let extra = varint_width(len as u32) - 1;
            end = end
                .checked_add(extra)
                .filter(|end| *end <= bound)
                .ok_or_else(|| budget.refuse_with(LimitKind::Bytes))?;
            if end > self.output.len() {
                return Err(budget.refuse_with(LimitKind::Scratch));
            }
            if !budget.charge_buffer(extra) {
                return Err(budget.error());
            }
            self.output
                .copy_within(body_at..body_at + len, body_at + extra);
        }
        let prefix = &mut self.output[self.used..end - len];
        postcard::to_slice(&(id, len as u32), prefix).map_err(|_| Error::Malformed)?;
        Ok(end)
    }

    /// Borrow the completed extension bytes from the caller's scratch storage.
    #[must_use]
    pub fn finish(self) -> Extensions<'a> {
        self.finish_and_tail().0
    }

    /// Borrow the completed block and return the remaining scratch space.
    ///
    /// The returned slice can stage the next field or record while the
    /// completed block remains borrowed.
    #[must_use]
    pub fn finish_and_tail(self) -> (Extensions<'a>, &'a mut [u8]) {
        let (bytes, tail) = self.output.split_at_mut(self.used);
        (Extensions(bytes), tail)
    }
}

/// An optional open variant with a stable numeric ID and opaque payload.
///
/// The first component is a Postcard u32. Its low bit selects a following
/// nonempty Postcard byte span. The upper bits hold the ID, with zero reserved
/// for absence. Unit variants have no payload length or payload bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueOpen<'a> {
    id: u32,
    bytes: &'a [u8],
}

impl<'a> OpaqueOpen<'a> {
    /// Construct an absent open variant.
    #[must_use]
    pub const fn absent() -> Self {
        Self { id: 0, bytes: &[] }
    }

    /// Construct a present unit variant without using scratch storage.
    ///
    /// # Errors
    /// Returns malformed if the ID is zero or greater than 2³¹ minus one.
    pub const fn unit(id: u32) -> Result<Self, Error> {
        if id == 0 || id > (u32::MAX >> 1) {
            return Err(Error::Malformed);
        }
        Ok(Self { id, bytes: &[] })
    }

    /// Write a typed open variant payload into scratch and borrow its extent.
    ///
    /// # Errors
    /// Returns an invalid ID, serialization, allowance or scratch capacity error.
    pub fn write<T: Serialize + ?Sized>(
        context: &WriteContext<'_>,
        id: u32,
        value: &T,
        scratch: &'a mut [u8],
    ) -> Result<Self, Error> {
        Self::write_in(context, id, value, scratch).map(|(open, _)| open)
    }

    /// Write a typed payload and return the open variant and unused scratch.
    ///
    /// # Errors
    /// Returns an invalid ID, serialization, allowance or scratch capacity error.
    pub fn write_in<T: Serialize + ?Sized>(
        context: &WriteContext<'_>,
        id: u32,
        value: &T,
        scratch: &'a mut [u8],
    ) -> Result<(Self, &'a mut [u8]), Error> {
        Self::unit(id)?;
        let leaf = context.child().child();
        let size = leaf.write_once(value, scratch)?;
        let (bytes, tail) = scratch.split_at_mut(size);
        Ok((Self { id, bytes }, tail))
    }

    /// Map a present selected tag, or keep absence.
    ///
    /// The callback can ignore any tag its schema does not understand and
    /// decode a known tag's entire payload. Known unit tags should decode the
    /// Rust unit value to reject a nonempty body.
    ///
    /// # Errors
    /// Returns any allowance or callback error.
    pub fn visit<R>(
        self,
        context: &ReadContext<'_>,
        visit: impl FnOnce(u32, Field<'_, 'a>) -> Result<R, Error>,
    ) -> Result<Option<R>, Error> {
        let variant = context.child();
        if self.id == 0 {
            admit_scope(variant.budget, variant.depth)?;
            return Ok(None);
        }
        let field_context = variant.child();
        // A present case visits the variant and its payload. Batch the debit
        // when both fit, and let the meter preserve their order on refusal.
        if !variant
            .budget
            .enter_nested_scopes(variant.depth, field_context.depth)
        {
            return Err(variant.budget.error());
        }
        let result = visit(
            self.id,
            Field {
                bytes: self.bytes,
                context: field_context,
            },
        );
        variant.budget.check()?;
        result.map(Some)
    }
}

impl Serialize for OpaqueOpen<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let has_payload = !self.bytes.is_empty();
        let mut tuple = serializer.serialize_tuple(1 + usize::from(has_payload))?;
        tuple.serialize_element(&((self.id << 1) | u32::from(has_payload)))?;
        if has_payload {
            tuple.serialize_element(Bytes::new(self.bytes))?;
        }
        tuple.end()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for OpaqueOpen<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OpenVisitor;
        impl<'de> Visitor<'de> for OpenVisitor {
            type Value = OpaqueOpen<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("open variant tag and optional nonempty payload")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let header: u32 = access
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let id = header >> 1;
                let bytes = if header & 1 == 0 {
                    &[]
                } else {
                    if id == 0 {
                        return Err(de::Error::custom("absent open variant has a payload"));
                    }
                    let bytes: &Bytes = access
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                    if bytes.is_empty() {
                        return Err(de::Error::custom("empty open variant payload extent"));
                    }
                    bytes.as_ref()
                };
                Ok(OpaqueOpen { id, bytes })
            }
        }
        deserializer.deserialize_tuple(2, OpenVisitor)
    }
}
