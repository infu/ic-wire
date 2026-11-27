//! Typed carriers for the explicit extension declarations.

use super::{Extensions, OpaqueOpen, ReadContext, Recoverable, RecoveryContext, WriteContext};
use crate::Error;
use core::marker::PhantomData;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The final extension block of an evolvable record, with the associated set
/// of field IDs retained in the Rust type.
pub struct ExtensionSlot<'a, T>(Extensions<'a>, PhantomData<fn() -> T>);

impl<T> Copy for ExtensionSlot<'_, T> {}

impl<T> Clone for ExtensionSlot<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> ExtensionSlot<'a, T> {
    /// Wrap an already prepared or decoded extension block.
    #[doc(hidden)]
    #[must_use]
    pub const fn from_raw(block: Extensions<'a>) -> Self {
        Self(block, PhantomData)
    }

    /// Access the low-level block for an explicit projection.
    #[doc(hidden)]
    #[must_use]
    pub const fn into_raw(self) -> Extensions<'a> {
        self.0
    }
}

impl<T> Serialize for ExtensionSlot<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de: 'a, 'a, T> Deserialize<'de> for ExtensionSlot<'a, T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Extensions::deserialize(deserializer).map(Self::from_raw)
    }
}

/// An open variant carrying the declaration of the cases the reader knows.
/// Set `REQUIRED` to propagate unknown tags to the nearest recovering scope.
pub struct OpenSlot<'a, T, const REQUIRED: bool = false>(OpaqueOpen<'a>, PhantomData<fn() -> T>);

impl<T, const REQUIRED: bool> Copy for OpenSlot<'_, T, REQUIRED> {}

impl<T, const REQUIRED: bool> Clone for OpenSlot<'_, T, REQUIRED> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T, const REQUIRED: bool> OpenSlot<'a, T, REQUIRED> {
    /// Wrap an already prepared or decoded open variant.
    #[doc(hidden)]
    #[must_use]
    pub const fn from_raw(open: OpaqueOpen<'a>) -> Self {
        Self(open, PhantomData)
    }

    /// Access the low-level variant for an explicit projection.
    #[doc(hidden)]
    #[must_use]
    pub const fn into_raw(self) -> OpaqueOpen<'a> {
        self.0
    }
}

impl<T, const REQUIRED: bool> Serialize for OpenSlot<'_, T, REQUIRED> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de: 'a, 'a, T, const REQUIRED: bool> Deserialize<'de> for OpenSlot<'a, T, REQUIRED> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OpaqueOpen::deserialize(deserializer).map(Self::from_raw)
    }
}

/// An optional, length-bounded payload that can recover an unknown required case.
pub struct RecoverSlot<'a, T>(Recoverable<'a>, PhantomData<fn() -> T>);

impl<T> Copy for RecoverSlot<'_, T> {}

impl<T> Clone for RecoverSlot<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> RecoverSlot<'a, T> {
    /// Construct an absent branch.
    #[must_use]
    pub const fn absent() -> Self {
        Self(Recoverable::absent(), PhantomData)
    }

    /// Prepare a present branch in the supplied scratch.
    /// # Errors
    /// Returns the first resource, encoding or scratch refusal.
    pub fn write(
        context: &WriteContext<'_>,
        value: &T,
        scratch: &'a mut [u8],
    ) -> Result<Self, Error>
    where
        T: Serialize,
    {
        Recoverable::write(context, value, scratch).map(|branch| Self(branch, PhantomData))
    }

    /// Project the branch through its nearest recovery scope.
    /// # Errors
    /// Returns a resource or payload failure. A marked unknown required tag
    /// yields absence after the projection checks the remaining known parts.
    pub fn project<R>(
        self,
        context: &ReadContext<'_>,
        project: impl FnOnce(T, &RecoveryContext<'_>) -> Result<R, Error>,
    ) -> Result<Option<R>, Error>
    where
        T: Deserialize<'a>,
    {
        self.0.project(context, project)
    }
}

impl<T> Serialize for RecoverSlot<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de: 'a, 'a, T> Deserialize<'de> for RecoverSlot<'a, T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Recoverable::deserialize(deserializer).map(|branch| Self(branch, PhantomData))
    }
}

#[cfg(feature = "derive")]
pub use ic_wire_derive::Evolve;

#[doc(hidden)]
pub mod __private {
    pub use alloc::vec::Vec;

    /// Describe the root projected by a generated typed method adapter.
    #[cfg(feature = "typescript")]
    pub trait EvolvingSchema {
        fn describe(
            context: &mut crate::typescript::SchemaContext,
        ) -> Result<serde_json::Value, crate::typescript::GenerationError>;
    }
}

#[cfg(feature = "typescript")]
mod schema {
    use super::{ExtensionSlot, OpenSlot, RecoverSlot};
    use crate::typescript::{
        __private::{Value, json},
        GenerationError, SchemaContext, Wire,
    };
    /// Return metadata for the fields held in an extension declaration.
    #[doc(hidden)]
    pub trait ExtensionLayout {
        fn extension_fields(context: &mut SchemaContext) -> Result<Value, GenerationError>;
    }

    /// Return metadata for the cases held in an open variant declaration.
    #[doc(hidden)]
    pub trait OpenLayout {
        fn open_cases(context: &mut SchemaContext) -> Result<Value, GenerationError>;
    }

    /// Confirm that the final member of an extension record is a typed carrier.
    #[doc(hidden)]
    pub trait SchemaFields {
        fn extension_fields(context: &mut SchemaContext) -> Result<Value, GenerationError>;
    }

    impl<T: ExtensionLayout> SchemaFields for ExtensionSlot<'_, T> {
        fn extension_fields(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            T::extension_fields(context)
        }
    }

    impl<T: OpenLayout, const REQUIRED: bool> Wire for OpenSlot<'_, T, REQUIRED> {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            let cases = T::open_cases(context)?;
            if REQUIRED {
                Ok(json!({"k":"open-variant","variants":cases,"required":true}))
            } else {
                Ok(json!({"k":"open-variant","variants":cases}))
            }
        }
    }

    impl<T: Wire> Wire for RecoverSlot<'_, T> {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"recover","item":context.describe::<T>()?}))
        }
    }
}

#[cfg(feature = "typescript")]
pub use schema::{ExtensionLayout, OpenLayout, SchemaFields};

/// Emit a generated schema implementation when the runtime supplies schema tooling.
#[cfg(feature = "typescript")]
#[doc(hidden)]
#[macro_export]
macro_rules! __wire_evolution_schema {
    ($schema:item) => {
        $schema
    };
}

/// Drop generated schema metadata from codec-only consumers.
#[cfg(not(feature = "typescript"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __wire_evolution_schema {
    ($schema:item) => {};
}
