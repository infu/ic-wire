//! Typed request and reply framing over the evolutionary codec.

use crate::{Direction, Encoding, Error, Limits, method_id};
use alloc::vec::Vec;
use core::marker::PhantomData;

use super::inline::InlineMessage;

/// A value with typed encoding and projection through the evolutionary codec.
///
/// Decoded values may borrow from the frame or payload for `'de`.
/// Both directions must use a sparse message root.
pub trait Evolving<'de>: Sized {
    /// Encode one complete value into reusable output.
    ///
    /// # Errors
    /// Returns an encoding or admission error and leaves output empty.
    fn encode_evolving(
        &self,
        output: &mut Vec<u8>,
        encoding: Encoding,
        limits: Limits,
    ) -> Result<(), Error>;

    /// Decode and project one complete frame or payload from the given input.
    ///
    /// # Errors
    /// Returns a framing, decoding, incompatibility, or admission error.
    fn decode_evolving(input: &'de [u8], encoding: Encoding, limits: Limits)
    -> Result<Self, Error>;
}

/// Encode a complete Wire message into owned output.
///
/// # Errors
/// Returns the first preparation, encoding or allowance failure.
pub fn encode<'de, T: Evolving<'de>>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    encode_into(value, encoding, limits, &mut output)?;
    Ok(output)
}

/// Encode a complete Wire message into reusable output.
///
/// # Errors
/// Returns the first preparation, encoding or allowance failure. Output is
/// empty on error.
pub fn encode_into<'out, 'de, T: Evolving<'de>>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
    output: &'out mut Vec<u8>,
) -> Result<&'out [u8], Error> {
    output.clear();
    if let Err(error) = value.encode_evolving(output, encoding, limits) {
        output.clear();
        return Err(error);
    }
    Ok(output)
}

/// Encode a derived Wire message directly into caller-owned output.
///
/// Use [`crate::evolution::encode_projected_to_slice`] for a manually
/// projected message. A fixed nested layout can implement `Evolving` with
/// a different root layout, so those bounds alone cannot select the slice
/// representation.
///
/// ```compile_fail,E0277
/// use ic_wire::{Encoding, Evolving, Limits, encode_to_slice};
/// use ic_wire::evolution::inline::Fixed;
/// use serde::{Deserialize, Serialize};
///
/// fn write_slice<'de, T>(value: &T, limits: Limits, output: &mut [u8])
/// where
///     T: Evolving<'de> + Fixed + Serialize + Deserialize<'de>,
/// {
///     encode_to_slice(value, Encoding::Payload, limits, output).unwrap();
/// }
/// ```
///
/// # Errors
/// Returns a preparation, encoding, allowance or destination capacity error.
/// A failed write may modify the destination. Only the prefix returned on
/// success contains a complete message.
pub fn encode_to_slice<'out, 'de, T: InlineMessage<'de>>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
    output: &'out mut [u8],
) -> Result<&'out [u8], Error> {
    super::encode_value_to_slice_with(output, encoding, limits, |mut destination| {
        let context = destination.context();
        super::inline::with_scratch(T::MAY_STAGE, context.budget, |scratch| {
            let (prepared, _) = value.prepare(&context, scratch)?;
            destination.write_value(&prepared)
        })
    })
}

/// Admit and decode one complete Wire message with its typed projection.
///
/// # Errors
/// Returns a framing, decoding, compatibility or allowance error.
pub fn decode<'de, T: Evolving<'de>>(
    input: &'de [u8],
    encoding: Encoding,
    limits: Limits,
) -> Result<T, Error> {
    T::decode_evolving(input, encoding, limits)
}

/// Bind an export name, direction limits and Rust DTOs to Wire frames.
///
/// An application can infer the lifetimes of borrowing DTOs locally with a type
/// such as `Method<Request<'_>, Reply<'_>>`.
pub struct Method<Request, Reply> {
    name: &'static str,
    id: u64,
    request_limits: Limits,
    reply_limits: Limits,
    types: PhantomData<fn() -> (Request, Reply)>,
}

impl<Request, Reply> Method<Request, Reply> {
    /// Define the Wire method for the exact exported name.
    ///
    /// A `const` descriptor computes the routing ID at compile time. The
    /// configured byte limits include the frame header in each direction.
    #[must_use]
    pub const fn new(name: &'static str, request_limits: Limits, reply_limits: Limits) -> Self {
        Self {
            name,
            id: method_id(name),
            request_limits,
            reply_limits,
            types: PhantomData,
        }
    }

    /// Get the exact exported name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Get the routing ID derived from the exported name.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Get the request limits.
    #[must_use]
    pub const fn request_limits(&self) -> Limits {
        self.request_limits
    }

    /// Get the reply limits.
    #[must_use]
    pub const fn reply_limits(&self) -> Limits {
        self.reply_limits
    }

    /// Encode a request frame into reusable output.
    ///
    /// # Errors
    /// Returns an encoding or request admission error.
    pub fn encode_request<'de>(&self, value: &Request, output: &mut Vec<u8>) -> Result<(), Error>
    where
        Request: Evolving<'de>,
    {
        encode_into(
            value,
            self.encoding(Direction::Request),
            self.request_limits,
            output,
        )
        .map(|_| ())
    }

    /// Decode a request frame for this method.
    ///
    /// # Errors
    /// Returns a framing, decoding, incompatibility, or request admission error.
    pub fn decode_request<'de>(&self, input: &'de [u8]) -> Result<Request, Error>
    where
        Request: Evolving<'de>,
    {
        decode(
            input,
            self.encoding(Direction::Request),
            self.request_limits,
        )
    }

    /// Encode a reply frame into reusable output.
    ///
    /// # Errors
    /// Returns an encoding or reply admission error.
    pub fn encode_reply<'de>(&self, value: &Reply, output: &mut Vec<u8>) -> Result<(), Error>
    where
        Reply: Evolving<'de>,
    {
        encode_into(
            value,
            self.encoding(Direction::Response),
            self.reply_limits,
            output,
        )
        .map(|_| ())
    }

    /// Decode a reply frame for this method.
    ///
    /// # Errors
    /// Returns a framing, decoding, incompatibility, or reply admission error.
    pub fn decode_reply<'de>(&self, input: &'de [u8]) -> Result<Reply, Error>
    where
        Reply: Evolving<'de>,
    {
        decode(input, self.encoding(Direction::Response), self.reply_limits)
    }

    const fn encoding(&self, direction: Direction) -> Encoding {
        Encoding::Frame {
            method: self.id,
            direction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evolution::{
        Projected, decode_projected, encode_projected_into, encode_projected_to_slice,
        inline::Fixed,
    };
    use crate::{LimitKind, TextRef};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Request<'a>(#[serde(borrow)] TextRef<'a, 16>);

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Reply(u16);

    impl Fixed for Reply {}

    impl<'de: 'a, 'a> Evolving<'de> for Request<'a> {
        fn encode_evolving(
            &self,
            output: &mut Vec<u8>,
            encoding: Encoding,
            limits: Limits,
        ) -> Result<(), Error> {
            encode_projected_into(output, encoding, limits, |_| Ok(Projected::empty(self)))
                .map(|_| ())
        }

        fn decode_evolving(
            input: &'de [u8],
            encoding: Encoding,
            limits: Limits,
        ) -> Result<Self, Error> {
            decode_projected(
                input,
                encoding,
                limits,
                |root: Projected<'_, Self>, context| {
                    root.extensions.visit(context, |_, _| Ok(()))?;
                    Ok(root.core)
                },
            )
        }
    }

    impl<'de> Evolving<'de> for Reply {
        fn encode_evolving(
            &self,
            output: &mut Vec<u8>,
            encoding: Encoding,
            limits: Limits,
        ) -> Result<(), Error> {
            encode_projected_into(output, encoding, limits, |_| Ok(Projected::empty(self)))
                .map(|_| ())
        }

        fn decode_evolving(
            input: &'de [u8],
            encoding: Encoding,
            limits: Limits,
        ) -> Result<Self, Error> {
            decode_projected(
                input,
                encoding,
                limits,
                |root: Projected<'_, Self>, context| {
                    root.extensions.visit(context, |_, _| Ok(()))?;
                    Ok(root.core)
                },
            )
        }
    }

    const REQUEST_LIMITS: Limits = Limits {
        bytes: 64,
        allocation: 4096,
        nodes: 64,
        depth: 16,
    };
    const REPLY_LIMITS: Limits = Limits {
        bytes: 13,
        ..REQUEST_LIMITS
    };
    const LOOKUP: Method<Request<'static>, Reply> =
        Method::new("lookup", REQUEST_LIMITS, REPLY_LIMITS);

    #[test]
    fn typed_methods_round_trip_borrowed_requests_and_check_identity() {
        assert_eq!(LOOKUP.name(), "lookup");
        assert_eq!(LOOKUP.id(), method_id("lookup"));
        assert_eq!(LOOKUP.request_limits().bytes, 64);
        assert_eq!(LOOKUP.reply_limits().bytes, 13);

        let value = alloc::string::String::from("hello");
        let mut input = Vec::new();
        let method: Method<Request<'_>, Reply> = LOOKUP;
        method
            .encode_request(&Request(TextRef::new(&value).unwrap()), &mut input)
            .unwrap();
        let decoded = method.decode_request(&input).unwrap();
        assert_eq!(decoded.0.as_str(), "hello");
        assert_eq!(decoded.0.as_str().as_bytes().as_ptr(), input[12..].as_ptr());

        let other = Method::<Request<'_>, Reply>::new("other", REQUEST_LIMITS, REPLY_LIMITS);
        assert_eq!(other.decode_request(&input), Err(Error::Identity));
        let mut reply = Vec::new();
        method.encode_reply(&Reply(7), &mut reply).unwrap();
        assert_eq!(reply[10..], [1, 7, 0]);
        assert_eq!(method.decode_reply(&reply), Ok(Reply(7)));
        assert_eq!(method.decode_request(&reply), Err(Error::Identity));
    }

    #[test]
    fn reply_limits_apply_to_both_encoding_and_decoding() {
        let mut reply = alloc::vec![1, 2, 3];
        assert_eq!(
            LOOKUP.encode_reply(&Reply(128), &mut reply),
            Err(Error::Limit(LimitKind::Bytes))
        );
        assert!(reply.is_empty());
        let large = crate::encode(
            &Reply(128),
            Encoding::Frame {
                method: LOOKUP.id(),
                direction: Direction::Response,
            },
            REQUEST_LIMITS,
        )
        .unwrap();
        assert_eq!(
            LOOKUP.decode_reply(&large),
            Err(Error::Limit(LimitKind::Bytes))
        );
    }

    #[test]
    fn fixed_nested_reply_uses_its_explicit_projection_at_the_root() {
        let value = Reply(42);
        let encoding = Encoding::Payload;
        let owned = encode(&value, encoding, REPLY_LIMITS).unwrap();
        let mut output = [0; 2];
        let slice = encode_projected_to_slice(&mut output, encoding, REPLY_LIMITS, |_| {
            Ok(Projected::empty(&value))
        })
        .unwrap();
        assert_eq!(slice, [42, 0]);
        assert_eq!(slice, owned);
        assert_eq!(decode::<Reply>(slice, encoding, REPLY_LIMITS), Ok(value));
    }
}
