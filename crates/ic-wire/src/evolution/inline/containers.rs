//! Stock Serde leaves and structural projections for nested Wire values.

use super::{Fixed, Inline, preserve_incompatible};
use crate::{
    Bytes, BytesRef, Error, FiniteF32, FiniteF64, List, Packed, PackedElement, Text, TextRef,
    evolution::{ReadContext, WriteContext},
};
use alloc::{borrow::Cow, boxed::Box, string::String, vec::Vec};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeTuple};

macro_rules! fixed {
    ($($ty:ty),* $(,)?) => { $(impl Fixed for $ty {})* };
}

fixed!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    FiniteF32,
    FiniteF64,
    String,
    str,
    Cow<'_, str>,
    serde_bytes::ByteBuf,
    serde_bytes::Bytes,
);

impl<T: Fixed + ?Sized> Fixed for &T {}
impl<T: Fixed + ?Sized> Fixed for Box<T> {}
impl<T: Fixed> Fixed for [T] {}
impl<const N: usize> Fixed for Bytes<N> {}
impl<const N: usize> Fixed for BytesRef<'_, N> {}
impl<const N: usize> Fixed for Text<N> {}
impl<const N: usize> Fixed for TextRef<'_, N> {}
impl<const N: usize> Fixed for serde_bytes::ByteArray<N> {}
impl<T: PackedElement, const N: usize> Fixed for Packed<T, N> {}

#[cfg(feature = "platform-types")]
fixed!(ic_platform_types::Principal, ic_platform_types::Account);

type Prepared<'s, T> = Result<(T, &'s mut [u8]), Error>;

impl<'de, T: Inline<'de>> Inline<'de> for Option<T> {
    const MAY_STAGE: bool = T::MAY_STAGE;

    type Encoded<'s>
        = Option<T::Encoded<'s>>
    where
        Self: 's;
    type Decoded = Option<T::Decoded>;

    fn prepare<'s>(
        &'s self,
        context: &WriteContext<'_>,
        scratch: &'s mut [u8],
    ) -> Prepared<'s, Self::Encoded<'s>> {
        context.admit_preparation()?;
        match self {
            Some(value) => {
                let (value, scratch) = value.prepare(&context.child(), scratch)?;
                Ok((Some(value), scratch))
            }
            None => Ok((None, scratch)),
        }
    }

    fn project(input: Self::Decoded, context: &ReadContext<'_>) -> Result<Self, Error> {
        input
            .map(|value| T::project(value, &context.child()))
            .transpose()
    }
}

impl<'de, T: Inline<'de>, E: Inline<'de>> Inline<'de> for Result<T, E> {
    const MAY_STAGE: bool = T::MAY_STAGE || E::MAY_STAGE;

    type Encoded<'s>
        = Result<T::Encoded<'s>, E::Encoded<'s>>
    where
        Self: 's;
    type Decoded = Result<T::Decoded, E::Decoded>;

    fn prepare<'s>(
        &'s self,
        context: &WriteContext<'_>,
        scratch: &'s mut [u8],
    ) -> Prepared<'s, Self::Encoded<'s>> {
        context.admit_preparation()?;
        let child = context.child();
        match self {
            Ok(value) => {
                let (value, scratch) = value.prepare(&child, scratch)?;
                Ok((Ok(value), scratch))
            }
            Err(value) => {
                let (value, scratch) = value.prepare(&child, scratch)?;
                Ok((Err(value), scratch))
            }
        }
    }

    fn project(input: Self::Decoded, context: &ReadContext<'_>) -> Result<Self, Error> {
        let child = context.child();
        match input {
            Ok(value) => T::project(value, &child).map(Ok),
            Err(value) => E::project(value, &child).map(Err),
        }
    }
}

fn reserve<T>(count: usize) -> Result<Vec<T>, Error> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::Allocation)?;
    Ok(values)
}

fn prepare_sequence<'de, 's, T: Inline<'de>>(
    input: &'s [T],
    context: &WriteContext<'_>,
    mut scratch: &'s mut [u8],
) -> Prepared<'s, Vec<T::Encoded<'s>>> {
    context.admit_allocation::<T::Encoded<'s>>(input.len())?;
    let mut values = reserve(input.len())?;
    let child = context.child();
    for value in input {
        let (value, tail) = value.prepare(&child, scratch)?;
        values.push(value);
        scratch = tail;
    }
    Ok((values, scratch))
}

fn project_sequence<'de, T: Inline<'de>>(
    input: impl ExactSizeIterator<Item = T::Decoded>,
    context: &ReadContext<'_>,
) -> Result<Vec<T>, Error> {
    context.admit_allocation::<T>(input.len())?;
    let mut values = reserve(input.len())?;
    let child = context.child();
    let mut incompatible = false;
    for value in input {
        if let Some(value) = preserve_incompatible(T::project(value, &child), &mut incompatible)? {
            values.push(value);
        }
    }
    if incompatible {
        return Err(Error::Incompatible);
    }
    Ok(values)
}

impl<'de, T: Inline<'de>> Inline<'de> for Vec<T> {
    const MAY_STAGE: bool = T::MAY_STAGE;

    type Encoded<'s>
        = Vec<T::Encoded<'s>>
    where
        Self: 's;
    type Decoded = Vec<T::Decoded>;

    fn prepare<'s>(
        &'s self,
        context: &WriteContext<'_>,
        scratch: &'s mut [u8],
    ) -> Prepared<'s, Self::Encoded<'s>> {
        prepare_sequence(self, context, scratch)
    }

    fn project(input: Self::Decoded, context: &ReadContext<'_>) -> Result<Self, Error> {
        project_sequence::<T>(input.into_iter(), context)
    }
}

impl<'de, T: Inline<'de>, const N: usize> Inline<'de> for List<T, N> {
    const MAY_STAGE: bool = T::MAY_STAGE;

    type Encoded<'s>
        = Vec<T::Encoded<'s>>
    where
        Self: 's;
    type Decoded = List<T::Decoded, N>;

    fn prepare<'s>(
        &'s self,
        context: &WriteContext<'_>,
        scratch: &'s mut [u8],
    ) -> Prepared<'s, Self::Encoded<'s>> {
        prepare_sequence(self.as_slice(), context, scratch)
    }

    fn project(input: Self::Decoded, context: &ReadContext<'_>) -> Result<Self, Error> {
        Self::new(project_sequence::<T>(
            input.into_vec().into_iter(),
            context,
        )?)
    }
}

/// An admitted array view serialized without a sequence length prefix.
pub struct EncodedArray<T>(Vec<T>);

impl<T: Serialize> Serialize for EncodedArray<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut tuple = serializer.serialize_tuple(self.0.len())?;
        for value in &self.0 {
            tuple.serialize_element(value)?;
        }
        tuple.end()
    }
}

impl<'de, T: Inline<'de>, const N: usize> Inline<'de> for [T; N]
where
    [T::Decoded; N]: Deserialize<'de>,
{
    const MAY_STAGE: bool = T::MAY_STAGE;

    type Encoded<'s>
        = EncodedArray<T::Encoded<'s>>
    where
        Self: 's;
    type Decoded = [T::Decoded; N];

    fn prepare<'s>(
        &'s self,
        context: &WriteContext<'_>,
        scratch: &'s mut [u8],
    ) -> Prepared<'s, Self::Encoded<'s>> {
        let (values, scratch) = prepare_sequence(self, context, scratch)?;
        Ok((EncodedArray(values), scratch))
    }

    fn project(input: Self::Decoded, context: &ReadContext<'_>) -> Result<Self, Error> {
        project_sequence::<T>(input.into_iter(), context)?
            .try_into()
            .map_err(|_| Error::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::{Inline, Prepared};
    use crate::{Encoding, Error, LimitKind, Limits, List, evolution::inline};
    use alloc::{boxed::Box, vec, vec::Vec};

    const LIMITS: Limits = Limits {
        bytes: 64,
        allocation: 8 * 1024,
        nodes: 256,
        depth: 24,
    };

    #[test]
    fn standard_containers_preserve_their_serde_bytes() {
        type Rows = List<Option<Result<[u16; 2], u32>>, 4>;
        let rows = Rows::new(vec![Some(Ok([500, 12])), None, Some(Err(99))]).unwrap();
        let mut output = Vec::new();
        inline::encode_inline(&rows, &mut output, Encoding::Payload, LIMITS, &mut []).unwrap();

        assert_eq!(output, [3, 1, 0, 244, 3, 12, 0, 1, 1, 99]);
        assert_eq!(
            inline::decode_inline::<Rows>(&output, Encoding::Payload, LIMITS),
            Ok(rows)
        );

        let text = Box::<str>::from("row");
        inline::encode_inline(&text, &mut output, Encoding::Payload, LIMITS, &mut []).unwrap();
        assert_eq!(output, [3, b'r', b'o', b'w']);
        assert_eq!(
            inline::decode_inline::<Box<str>>(&output, Encoding::Payload, LIMITS),
            Ok(text)
        );
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Checked(u8);

    impl<'de> Inline<'de> for Checked {
        type Encoded<'s> = u8;
        type Decoded = u8;

        fn prepare<'s>(
            &'s self,
            _: &crate::evolution::WriteContext<'_>,
            scratch: &'s mut [u8],
        ) -> Prepared<'s, Self::Encoded<'s>> {
            Ok((self.0, scratch))
        }

        fn project(
            input: Self::Decoded,
            _: &crate::evolution::ReadContext<'_>,
        ) -> Result<Self, Error> {
            match input {
                0 => Err(Error::Incompatible),
                1 => Err(Error::Malformed),
                value => Ok(Self(value)),
            }
        }
    }

    #[test]
    fn collection_incompatibility_still_checks_later_elements() {
        assert_eq!(
            inline::decode_inline::<List<Checked, 2>>(&[2, 0, 1], Encoding::Payload, LIMITS),
            Err(Error::Malformed)
        );
        assert_eq!(
            inline::decode_inline::<Vec<Checked>>(&[2, 0, 2], Encoding::Payload, LIMITS),
            Err(Error::Incompatible)
        );
        assert_eq!(
            inline::decode_inline::<[Checked; 2]>(&[0, 1], Encoding::Payload, LIMITS),
            Err(Error::Malformed)
        );
    }

    #[test]
    fn staging_allocations_are_admitted_before_serializing() {
        let mut output = vec![255];
        let limits = Limits {
            allocation: LIMITS.bytes,
            ..LIMITS
        };
        assert_eq!(
            inline::encode_inline(
                &vec![8_u16; 2],
                &mut output,
                Encoding::Payload,
                limits,
                &mut []
            ),
            Err(Error::Limit(LimitKind::Allocation))
        );
        assert!(output.is_empty());
    }
}
