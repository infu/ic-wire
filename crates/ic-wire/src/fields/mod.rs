//! Explicit field bounds without changing ordinary Serde layouts.
// Adapted from pinned IC Wire bounded adapters; see docs/design/fields.md.
mod borrowed;
pub use borrowed::{BytesRef, TextRef};

use crate::Error;
use alloc::{string::String, vec::Vec};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A sequence with a compile-time element bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct List<T, const N: usize>(Vec<T>);
impl<T, const N: usize> List<T, N> {
    /// Admit an owned sequence without copying.
    pub fn new(values: Vec<T>) -> Result<Self, Error> {
        if values.len() > N {
            Err(Error::Limit(crate::LimitKind::Field))
        } else {
            Ok(Self(values))
        }
    }

    /// Borrow the elements.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Mutably borrow the elements without changing the admitted length.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.0
    }

    /// Recover the admitted sequence.
    #[must_use]
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }
}
impl<T: Serialize, const N: usize> Serialize for List<T, N> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for List<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_seq(ListVisitor::<T, N>(core::marker::PhantomData))
            .map(Self)
    }
}

struct ListVisitor<T, const N: usize>(core::marker::PhantomData<T>);
impl<'de, T: Deserialize<'de>, const N: usize> de::Visitor<'de> for ListVisitor<T, N> {
    type Value = Vec<T>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded sequence")
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        read_list(seq, N)
    }
}

fn read_list<'de, T: Deserialize<'de>, A: de::SeqAccess<'de>>(
    mut seq: A,
    limit: usize,
) -> Result<Vec<T>, A::Error> {
    // Postcard hides its count when remaining bytes are fewer than
    // elements (including legitimate unit values). Bound both paths.
    let count = seq.size_hint();
    if count.is_some_and(|count| count > limit) {
        return Err(de::Error::custom("element limit"));
    }
    let mut values = Vec::new();
    values
        .try_reserve_exact(count.unwrap_or(0))
        .map_err(|_| de::Error::custom("allocation"))?;
    for _ in 0..limit {
        let Some(value) = seq.next_element()? else {
            return Ok(values);
        };
        if values.len() == values.capacity() {
            // Hidden lengths need amortized growth, capped by the
            // declared element bound rather than Vec's growth policy.
            let next = values.len().saturating_mul(2).max(1).min(limit);
            values
                .try_reserve_exact(next - values.len())
                .map_err(|_| de::Error::custom("allocation"))?;
        }
        values.push(value);
    }
    // Detect an excess element without invoking its decoder.
    struct Reject;
    impl<'de> de::DeserializeSeed<'de> for Reject {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
            Err(de::Error::custom("element limit"))
        }
    }
    seq.next_element_seed(Reject)?;
    Ok(values)
}

/// Bounded opaque bytes; consumers validate principal bytes using their existing
/// principal type. IC Wire defines no new identity type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bytes<const N: usize>(Vec<u8>);
impl<const N: usize> Bytes<N> {
    /// Admit bytes without copying.
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() > N {
            Err(Error::Limit(crate::LimitKind::Field))
        } else {
            Ok(Self(bytes))
        }
    }

    /// Borrow the bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Borrow this bounded value without copying or repeating admission.
    #[must_use]
    pub fn as_borrowed(&self) -> BytesRef<'_, N> {
        BytesRef::from_owned(self)
    }

    /// Recover the admitted bytes without copying or allocating.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}
impl<const N: usize> Serialize for Bytes<N> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}
impl<'de, const N: usize> Deserialize<'de> for Bytes<N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_bytes(BytesVisitor { limit: N }).map(Self)
    }
}

// Share the allocation/copy body across byte bounds.
// Vec<u8> and Bytes<N> have the same visited size. Check the same limit before
// allocation; bytes, traversal charges and malformed-input errors stay unchanged.
struct BytesVisitor {
    limit: usize,
}
impl de::Visitor<'_> for BytesVisitor {
    type Value = Vec<u8>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded bytes")
    }

    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
        if bytes.len() > self.limit {
            return Err(E::custom("byte limit"));
        }
        let mut out = Vec::new();
        out.try_reserve_exact(bytes.len())
            .map_err(|_| E::custom("allocation"))?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
}

/// UTF-8 text bounded in bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Text<const N: usize>(String);
impl<const N: usize> Text<N> {
    /// Admit owned text without copying.
    pub fn new(text: String) -> Result<Self, Error> {
        if text.len() > N {
            Err(Error::Limit(crate::LimitKind::Field))
        } else {
            Ok(Self(text))
        }
    }

    /// Borrow the text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrow this bounded value without copying or repeating admission.
    #[must_use]
    pub fn as_borrowed(&self) -> TextRef<'_, N> {
        TextRef::from_owned(self)
    }

    /// Recover the admitted text without copying or allocating.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}
impl<const N: usize> Serialize for Text<N> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de, const N: usize> Deserialize<'de> for Text<N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = Bytes::<N>::deserialize(d)?;
        let text = String::from_utf8(bytes.0).map_err(|_| de::Error::custom("invalid UTF-8"))?;
        Ok(Self(text))
    }
}
