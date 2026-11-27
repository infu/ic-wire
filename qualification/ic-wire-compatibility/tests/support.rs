//! Exercise arbitrary fixed field layouts through a sparse message root.
use serde::{Deserialize, Serialize};
use wire::{
    Encoding, Error, Limits,
    evolution::{Projected, decode_projected, encode_projected_into},
};

pub(crate) fn encode<T: Serialize + ?Sized>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    encode_projected_into(&mut output, encoding, limits, |_| {
        Ok(Projected::empty(value))
    })?;
    Ok(output)
}

pub(crate) fn decode<'de, T: Deserialize<'de>>(
    bytes: &'de [u8],
    encoding: Encoding,
    limits: Limits,
) -> Result<T, Error> {
    decode_projected(
        bytes,
        encoding,
        limits,
        |root: Projected<'de, T>, context| {
            root.extensions.visit(context, |_, _| Ok(()))?;
            Ok(root.core)
        },
    )
}
