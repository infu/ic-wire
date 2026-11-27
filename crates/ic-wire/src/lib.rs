//! Evolvable Wire messages over bounded Serde/Postcard.
//!
//! Bounded encoding and decoding, method identity and request-header
//! admission. Header admission is
//! neither authentication nor payload/schema/resource validation.
#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate std;

mod decode;
mod encode;
mod error;
mod fields;
mod finite;
mod framing;
mod limits;
mod meter;
mod packed;
pub mod serde_packed;

/// Sparse extensions, open variants, and explicit message projections.
pub mod evolution;

/// Optional schema admission and TypeScript declarations.
#[cfg(feature = "typescript")]
pub mod typescript;

pub use error::{Error, LimitKind};
pub use evolution::method::{decode, encode, encode_into, encode_to_slice};
pub use evolution::{Evolving, Method};
pub use fields::{Bytes, BytesRef, List, Text, TextRef};
pub use finite::{FiniteF32, FiniteF64};
pub use framing::{Direction, Encoding, method_id, validate_request_header};
#[cfg(feature = "derive")]
pub use ic_wire_derive::Wire;
pub use limits::Limits;
pub use packed::{Packed, PackedElement};

/// Published Serde bulk byte types; ByteArray includes a length prefix, unlike
/// an ordinary `[u8; N]`. Changing field representation requires consumer review.
pub use serde_bytes::{ByteArray, ByteBuf};

#[cfg(test)]
mod tests;
