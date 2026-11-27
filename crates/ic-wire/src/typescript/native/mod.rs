//! Bounded inline/named metadata production for the shared admission boundary.
mod adapters;
#[cfg(feature = "platform-types")]
mod platform;
mod registry;

use super::{GenerationError, SchemaLimits, ValidatedRegistry};
pub use adapters::PackedField;
use alloc::{borrow::Cow, boxed::Box, collections::BTreeMap, string::String, vec::Vec};
pub use registry::SchemaRegistry;
use serde_json::{Value, json};

type Projection = fn(&mut SchemaContext) -> Result<Value, GenerationError>;

/// A trusted description of a type's Serde representation.
///
/// This does not implement Serialize/Deserialize. Manual implementations must
/// independently qualify that their projection agrees with their Serde code.
pub trait Wire {
    /// Produce metadata using `context.describe` for every nested Rust type.
    ///
    /// # Errors
    /// Return Schema for unsupported layouts or Complexity for exhausted work.
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError>;
}

/// Construction allowance for Rust metadata, separate from graph admission.
///
/// Created by `ValidatedRegistry::from_type` or `SchemaRegistry`. Calls and active
/// type depth are bounded; inline recursion rejects and named recursion emits
/// references rather than expanding. Manual projection
/// code is trusted and may allocate/work outside this accounting, like custom
/// Serde code. Final JSON admission independently checks names and exact nodes.
#[derive(Debug)]
pub struct SchemaContext {
    remaining: usize,
    depth: usize,
    active: Vec<&'static str>,
    failed: bool,
    names: Option<Box<registry::Names>>,
}

impl SchemaContext {
    /// Project the sealed standard serde_bytes adapter without guessing an
    /// arbitrary custom serializer's representation.
    /// # Errors
    /// Returns the same admission errors as describe.
    #[doc(hidden)]
    pub fn describe_bytes<T: SerdeBytesField + ?Sized>(
        &mut self,
    ) -> Result<Value, GenerationError> {
        self.describe::<T::Layout>()
    }

    /// Describe a nested type without bypassing construction admission.
    ///
    /// # Errors
    /// Schema rejects recursive inline projection or a previously failed
    /// context; Complexity rejects exhausted construction calls or depth.
    pub fn describe<T: Wire + ?Sized>(&mut self) -> Result<Value, GenerationError> {
        self.describe_projection::<T>(T::schema)
    }

    fn describe_projection<T: ?Sized>(
        &mut self,
        projection: Projection,
    ) -> Result<Value, GenerationError> {
        if self.failed {
            return Err(GenerationError::Schema);
        }
        self.failed = true;
        let name = core::any::type_name::<T>();
        if self.names.is_none() && self.active.contains(&name) {
            return Err(GenerationError::Schema);
        }
        if self.active.len() > self.depth {
            return Err(GenerationError::Complexity);
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(GenerationError::Complexity)?;
        self.active.push(name);
        self.failed = false;
        let result = if self.names.is_some() {
            self.named::<T>(projection)
        } else {
            projection(self)
        };
        self.active.pop();
        // A manual projection cannot hide a failed nested registration.
        if self.failed && result.is_ok() {
            return Err(GenerationError::Schema);
        }
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

impl ValidatedRegistry {
    /// Project one Rust root and admit its inline metadata into the shared graph.
    ///
    /// Nested declarations are inline; no generated-name algorithm is substituted
    /// for the named collector. Recursive automatic projection rejects;
    /// use SchemaRegistry for shared or recursive named collection.
    ///
    /// # Errors
    /// Returns Schema for invalid/recursive projections and Complexity for
    /// exceeded construction or graph limits. No partial registry is returned.
    pub fn from_type<T: Wire + ?Sized>(
        root: &str,
        limits: SchemaLimits,
    ) -> Result<Self, GenerationError> {
        super::identifier(root)?;
        if limits.depth > 64 || root.len() > limits.names_bytes {
            return Err(GenerationError::Complexity);
        }
        let mut context = SchemaContext {
            remaining: limits.nodes,
            depth: limits.depth,
            active: Vec::new(),
            failed: false,
            names: None,
        };
        let schema = context.describe::<T>()?;
        Self::from_metadata(&BTreeMap::from([(String::from(root), schema)]), limits)
    }
}

macro_rules! scalar {
    ($($ty:ty => $value:expr),* $(,)?) => { $(
        impl Wire for $ty {
            fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> { Ok($value) }
        }
    )* };
}
scalar! {
    crate::FiniteF32 => json!({"k":"float","bits":32}),
    crate::FiniteF64 => json!({"k":"float","bits":64}),
    () => json!({"k":"unit"}), bool => json!({"k":"bool"}),
    u8 => json!({"k":"u","bits":8}), u16 => json!({"k":"u","bits":16}),
    u32 => json!({"k":"u","bits":32}), u64 => json!({"k":"u","bits":64}), u128 => json!({"k":"u","bits":128}),
    i8 => json!({"k":"i","bits":8}), i16 => json!({"k":"i","bits":16}),
    i32 => json!({"k":"i","bits":32}), i64 => json!({"k":"i","bits":64}), i128 => json!({"k":"i","bits":128}),
    str => json!({"k":"text"}), String => json!({"k":"text"}),
    serde_bytes::ByteBuf => json!({"k":"bytes"}), serde_bytes::Bytes => json!({"k":"bytes"}),
}
impl<const N: usize> Wire for serde_bytes::ByteArray<N> {
    fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"bytes","min":N,"max":N}))
    }
}
impl<T: crate::PackedElement, const N: usize> Wire for crate::Packed<T, N> {
    fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
        let mut value = json!({"k":"packed","word":T::SCHEMA});
        if N != usize::MAX {
            value["max"] = json!(N);
        }
        Ok(value)
    }
}

mod byte_field {
    pub trait Sealed {
        type Layout: super::Wire;
    }
    impl Sealed for alloc::vec::Vec<u8> {
        type Layout = serde_bytes::ByteBuf;
    }
    impl Sealed for [u8] {
        type Layout = serde_bytes::ByteBuf;
    }
    impl Sealed for serde_bytes::ByteBuf {
        type Layout = Self;
    }
    impl Sealed for serde_bytes::Bytes {
        type Layout = serde_bytes::ByteBuf;
    }
    impl<const N: usize> Sealed for [u8; N] {
        type Layout = serde_bytes::ByteArray<N>;
    }
    impl<const N: usize> Sealed for serde_bytes::ByteArray<N> {
        type Layout = Self;
    }
    impl<T: Sealed + ?Sized> Sealed for &T {
        type Layout = T::Layout;
    }
    impl<T: Sealed> Sealed for Option<T> {
        type Layout = Option<T::Layout>;
    }
}
/// Sealed inputs for the standard Serde bulk-byte adapter.
///
/// ```compile_fail
/// fn requires_bytes<T: ic_wire::typescript::SerdeBytesField>() {}
/// requires_bytes::<Vec<u16>>();
/// ```
#[doc(hidden)]
pub trait SerdeBytesField: byte_field::Sealed {}
impl<T: byte_field::Sealed + ?Sized> SerdeBytesField for T {}
impl<T: Wire> Wire for Option<T> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"option","item":context.describe::<T>()?}))
    }
}
impl<T: Wire, E: Wire> Wire for Result<T, E> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"result","ok":context.describe::<T>()?,"err":context.describe::<E>()?}))
    }
}
impl<T: Wire, const N: usize> Wire for [T; N]
where
    [(); N]: serde::Serialize,
{
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"array","max":N,"item":context.describe::<T>()?}))
    }
}
impl<T: Wire> Wire for [T] {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"list","item":context.describe::<T>()?}))
    }
}
impl Wire for Cow<'_, str> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        context.describe::<str>()
    }
}
macro_rules! tuple {
    ($($ty:ident),+) => {
        impl<$($ty: Wire),+> Wire for ($($ty,)+) {
            fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
                Ok(json!({"k":"tuple","items":[$(context.describe::<$ty>()?),+]}))
            }
        }
    };
}
tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
impl<T: Wire> Wire for Vec<T> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"list","item":context.describe::<T>()?}))
    }
}
impl<T: Wire + ?Sized> Wire for Box<T> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        context.describe::<T>()
    }
}
impl<T: Wire + ?Sized> Wire for &T {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        context.describe::<T>()
    }
}
impl<T: Wire, const N: usize> Wire for crate::List<T, N> {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"list","max":N,"item":context.describe::<T>()?}))
    }
}
macro_rules! bounded {
    ($($ty:ident => $kind:literal),* $(,)?) => { $(
        impl<const N: usize> Wire for crate::$ty<N> {
            fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> { Ok(json!({"k":$kind,"max":N})) }
        }
    )* };
}
bounded! { Bytes => "bytes", Text => "text" }
impl<const N: usize> Wire for crate::BytesRef<'_, N> {
    fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"bytes","max":N}))
    }
}
impl<const N: usize> Wire for crate::TextRef<'_, N> {
    fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
        Ok(json!({"k":"text","max":N}))
    }
}
