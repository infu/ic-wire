//! Shared contract for the raw counter endpoints and their generated clients.

/// The largest u32 needs five Postcard bytes. The empty extension block adds one.
pub(crate) const PAYLOAD_BYTES: usize = 6;

#[derive(wire::Wire)]
#[wire(crate = "wire")]
pub(crate) struct Word {
    pub(crate) value: u32,
}
