//! Explicit optional branches with a bounded, typed recovery scope.

use super::{Error, Field, OpaqueOpen, ReadContext, WriteContext, admit_scope};
use core::cell::Cell;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::Bytes;

/// A Postcard option containing a bounded, borrowed Postcard payload.
///
/// Absence is the byte zero. Presence is the byte one followed by the byte
/// length and contents of the payload. A present unit has a zero byte length.
/// Consumers explicitly project this value to enable recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recoverable<'a>(Option<&'a [u8]>);

impl<'a> Recoverable<'a> {
    /// Construct an absent branch.
    #[must_use]
    pub const fn absent() -> Self {
        Self(None)
    }

    /// Write a present branch payload into caller-owned scratch storage.
    ///
    /// # Errors
    ///
    /// Returns an allowance, output capacity, or payload serialization error.
    pub fn write<T: Serialize + ?Sized>(
        context: &WriteContext<'_>,
        value: &T,
        scratch: &'a mut [u8],
    ) -> Result<Self, Error> {
        let body = context.child();
        let size = body.write_once(value, scratch)?;
        Ok(Self(Some(&scratch[..size])))
    }

    /// Project a complete branch and map a marked incompatibility to absence.
    ///
    /// The projection must check the whole branch, including known siblings
    /// after an incompatible value, and propagate errors from those checks.
    ///
    /// # Errors
    ///
    /// Returns a malformed payload, allowance refusal, or projection error.
    /// Recovery requires a mark from [RecoveryContext::required_open] or
    /// [RecoveryContext::missing_required].
    pub fn project<T: Deserialize<'a>, R>(
        self,
        context: &ReadContext<'_>,
        project: impl FnOnce(T, &RecoveryContext<'_>) -> Result<R, Error>,
    ) -> Result<Option<R>, Error> {
        let branch = context.child();
        admit_scope(branch.budget, branch.depth)?;
        let Some(bytes) = self.0 else {
            return Ok(None);
        };
        let field = Field {
            bytes,
            context: branch,
        };
        field.project(|value, read| {
            let incompatible = Cell::new(false);
            let scope = RecoveryContext {
                read,
                incompatible: &incompatible,
            };
            let projected = project(value, &scope)?;
            Ok((!incompatible.get()).then_some(projected))
        })
    }
}

impl Serialize for Recoverable<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.map(Bytes::new).serialize(serializer)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Recoverable<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<&Bytes>::deserialize(deserializer).map(|bytes| Self(bytes.map(AsRef::as_ref)))
    }
}

/// Projection scope that records unknown required tags and missing required fields.
///
/// Projectors still validate each known descendant. The scope owns only the
/// compatibility result and borrows the same admission meter as its parent.
pub struct RecoveryContext<'a> {
    read: &'a ReadContext<'a>,
    incompatible: &'a Cell<bool>,
}

impl RecoveryContext<'_> {
    /// Check a projection with no enclosing option for a marked incompatibility.
    ///
    /// # Errors
    ///
    /// Returns a projection error first. Returns [Error::Incompatible] if the
    /// projection completes with a marked incompatibility outside a recovering
    /// branch.
    pub fn check<R>(
        context: &ReadContext<'_>,
        project: impl FnOnce(&RecoveryContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let incompatible = Cell::new(false);
        let scope = RecoveryContext {
            read: context,
            incompatible: &incompatible,
        };
        let value = project(&scope)?;
        context.budget.check()?;
        if incompatible.get() {
            return Err(Error::Incompatible);
        }
        Ok(value)
    }

    /// Borrow the current context for ordinary projection and nested branches.
    #[must_use]
    pub const fn read_context(&self) -> &ReadContext<'_> {
        self.read
    }

    /// Admit one additional structural level within this scope.
    ///
    /// # Errors
    ///
    /// Returns the depth or work refusal or a projection error.
    pub fn nested<R>(
        &self,
        project: impl FnOnce(&RecoveryContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        self.read.nested(|read| {
            project(&RecoveryContext {
                read,
                incompatible: self.incompatible,
            })
        })
    }

    /// Project a known field at its own admitted depth within this scope.
    ///
    /// # Errors
    ///
    /// Returns a typed field error or a projection error. A field from a
    /// different admission context is malformed.
    pub fn project_field<'de, T: Deserialize<'de>, R>(
        &self,
        field: Field<'_, 'de>,
        project: impl FnOnce(T, &RecoveryContext<'_>) -> Result<R, Error>,
    ) -> Result<R, Error> {
        if !core::ptr::eq(self.read.budget, field.context.budget) {
            return Err(Error::Malformed);
        }
        field.project(|value, read| {
            project(
                value,
                &RecoveryContext {
                    read,
                    incompatible: self.incompatible,
                },
            )
        })
    }

    /// Mark a missing required extension in the reader's schema.
    ///
    /// Call this only after a complete [extension visit](super::Extensions::visit)
    /// has validated every entry header and known value. Continue projecting
    /// the remaining known siblings and propagate their errors.
    pub fn missing_required(&self) {
        self.incompatible.set(true);
    }

    /// Map a required open variant and mark a well-formed unknown tag.
    ///
    /// Return [None] from the callback only for unknown tags. Known tags must
    /// check their typed payloads. The result is absent until the enclosing
    /// projection decides whether it can return the full value.
    ///
    /// # Errors
    ///
    /// Returns malformed for a zero tag, and propagates allowance and callback
    /// errors.
    pub fn required_open<'de, R>(
        &self,
        open: OpaqueOpen<'de>,
        visit: impl FnOnce(u32, Field<'_, 'de>) -> Result<Option<R>, Error>,
    ) -> Result<Option<R>, Error> {
        let value = open.visit(self.read, visit)?.ok_or(Error::Malformed)?;
        if value.is_none() {
            self.incompatible.set(true);
        }
        Ok(value)
    }
}
