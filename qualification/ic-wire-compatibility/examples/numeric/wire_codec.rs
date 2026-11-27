use super::Value;
use wire::evolution::{Projected, decode_projected, encode_projected_into};

const LIMITS: wire::Limits = wire::Limits {
    bytes: 65536,
    allocation: 4194304,
    nodes: 65536,
    depth: 32,
};

pub(super) fn encode(value: &Value) -> Vec<u8> {
    let mut output = Vec::new();
    encode_projected_into(&mut output, wire::Encoding::Payload, LIMITS, |_| {
        Ok(Projected::empty(value))
    })
    .unwrap();
    output
}

pub(super) fn decode(bytes: &[u8]) -> Value {
    decode_projected(
        bytes,
        wire::Encoding::Payload,
        LIMITS,
        |root: Projected<'_, Value>, context| {
            root.extensions.visit(context, |_, _| Ok(()))?;
            Ok(root.core)
        },
    )
    .unwrap()
}
