//! Generated open-variant recovery with independently constructed payloads.

use proptest::{arbitrary::any, collection::vec, prop_assert_eq, proptest};
use serde::Deserialize;
use wire::{
    Encoding, Error, Limits,
    evolution::{OpaqueOpen, Projected, RecoverSlot, decode_projected},
};

const LIMITS: Limits = Limits {
    bytes: 128,
    allocation: 8192,
    nodes: 256,
    depth: 16,
};

///
/// Packet
///
/// Holds an optional outer branch and a witness outside it.
///

#[derive(Deserialize)]
struct Packet<'a> {
    #[serde(borrow)]
    outer: RecoverSlot<'a, Outer<'a>>,
    outside: u8,
}

///
/// Outer
///
/// Holds the nearer recovery boundary and a witness outside that boundary.
///

#[derive(Deserialize)]
struct Outer<'a> {
    #[serde(borrow)]
    inner: RecoverSlot<'a, Inner<'a>>,
    witness: u8,
}

///
/// Inner
///
/// Holds the evolving case and a known case whose body is projected afterward.
///

#[derive(Deserialize)]
struct Inner<'a> {
    #[serde(borrow)]
    state: OpaqueOpen<'a>,
    #[serde(borrow)]
    sibling: OpaqueOpen<'a>,
}

type Projection = (Option<(Option<bool>, u8)>, u8);

fn read_old(bytes: &[u8]) -> Result<Projection, Error> {
    decode_projected(
        bytes,
        Encoding::Payload,
        LIMITS,
        |root: Projected<'_, Packet<'_>>, ctx| {
            root.extensions.visit(ctx, |_, _| Ok(()))?;
            let outer = root.core.outer.project(ctx, |outer, scope| {
                let inner = outer.inner.project(scope.read_context(), |inner, scope| {
                    scope.required_open(inner.state, |id, field| match id {
                        1 => field.decode::<()>().map(Some),
                        _ => Ok(None),
                    })?;

                    inner
                        .sibling
                        .visit(scope.read_context(), |id, field| match id {
                            2 => field.decode::<bool>(),
                            _ => Err(Error::Malformed),
                        })?
                        .ok_or(Error::Malformed)
                })?;

                Ok((inner, outer.witness))
            })?;

            Ok((outer, root.core.outside))
        },
    )
}

fn recovery_payload(state: &[u8], sibling: u8, witness: u8, outside: u8) -> Vec<u8> {
    // Inputs stay below 128 bytes, so all length prefixes fit in one byte.
    let inner_len = u8::try_from(state.len() + 3).unwrap();
    let outer_len = inner_len + 3;
    let mut payload = vec![1, outer_len, 1, inner_len];
    payload.extend_from_slice(state);
    // The known case carries one boolean byte. Zero ends the root extension block.
    payload.extend_from_slice(&[5, 1, sibling, witness, outside, 0]);

    payload
}

proptest! {
    #[test]
    fn unknown_case_recovers_nearest_branch_and_checks_known_sibling(
        future_id in 2_u8..=63,
        future_body in vec(any::<u8>(), 0..=16),
        sibling in any::<bool>(),
        witness in any::<u8>(),
        outside in any::<u8>(),
        malformed_bool in 2_u8..=255,
    ) {
        let mut future_case = vec![future_id << 1];
        if !future_body.is_empty() {
            future_case[0] |= 1;
            future_case.push(u8::try_from(future_body.len()).unwrap());
            future_case.extend_from_slice(&future_body);
        }

        let known = recovery_payload(&[2], u8::from(sibling), witness, outside);
        prop_assert_eq!(read_old(&known), Ok((Some((Some(sibling), witness)), outside)));

        let future = recovery_payload(&future_case, u8::from(sibling), witness, outside);
        prop_assert_eq!(read_old(&future), Ok((Some((None, witness)), outside)));

        let malformed = recovery_payload(&future_case, malformed_bool, witness, outside);
        prop_assert_eq!(read_old(&malformed), Err(Error::Malformed));
    }
}
