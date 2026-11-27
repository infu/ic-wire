/// The first allowance that refused an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    /// Complete encoded or supplied input bytes.
    Bytes,
    /// Conservative traversal allocation allowance.
    Allocation,
    /// Value and seed visits.
    Nodes,
    /// Nested traversal depth.
    Depth,
    /// Caller-provided destination capacity.
    Output,
    /// Caller-provided temporary storage for preparing extensions.
    Scratch,
    /// Declared field bound in a fallible constructor.
    Field,
}

/// Wire failure categories, independent of transport and application errors.
///
/// Allocation is reserved for fallible codec-owned output allocation; consumer
/// deserialization can allocate outside the codec's control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// A wire, collection, traversal or output allowance was exceeded.
    Limit(LimitKind),
    /// Allocation failed within an admitted allowance.
    Allocation,
    /// The representation is invalid or truncated.
    Malformed,
    /// Input remains after the expected value.
    Trailing,
    /// The envelope marker, method identity or direction differs.
    Identity,
    /// A well-formed evolved value cannot satisfy the required projection.
    Incompatible,
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Limit(LimitKind::Bytes) => "wire bytes limit",
            Self::Limit(LimitKind::Allocation) => "wire allocation limit",
            Self::Limit(LimitKind::Nodes) => "wire nodes limit",
            Self::Limit(LimitKind::Depth) => "wire depth limit",
            Self::Limit(LimitKind::Output) => "wire output limit",
            Self::Limit(LimitKind::Scratch) => "wire scratch limit",
            Self::Limit(LimitKind::Field) => "wire field limit",
            Self::Allocation => "wire allocation",
            Self::Malformed => "wire malformed",
            Self::Trailing => "wire trailing",
            Self::Identity => "wire identity",
            Self::Incompatible => "wire incompatible",
        })
    }
}

impl core::error::Error for Error {}
