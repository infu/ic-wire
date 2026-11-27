//! Regression for projection of known nested fields before optional recovery.
#![cfg(feature = "derive")]

use ic_wire::{
    Encoding, Error, LimitKind, Limits,
    evolution::{Evolve, ExtensionSlot, Projected, RecoverSlot, RecoveryContext, decode_projected},
};
use serde::Deserialize;

const LIMITS: Limits = Limits {
    bytes: 32,
    allocation: 8192,
    nodes: 64,
    depth: 12,
};

#[derive(Evolve)]
struct DetailFields {
    #[wire(id = 1)]
    active: Option<bool>,
}

#[derive(Deserialize, serde::Serialize)]
#[cfg_attr(feature = "typescript", derive(ic_wire_derive::Wire))]
struct Detail<'a>(
    #[serde(borrow)]
    #[cfg_attr(feature = "typescript", wire(extensions))]
    ExtensionSlot<'a, DetailFields>,
);

#[derive(Evolve)]
struct BranchFields<'a> {
    #[wire(id = 2)]
    detail: Option<Detail<'a>>,
    #[wire(id = 1, required)]
    generation: u8,
}

#[derive(Deserialize)]
struct Message<'a>(
    #[serde(borrow)] RecoverSlot<'a, ExtensionSlot<'a, BranchFields<'a>>>,
    u8,
);

type Branch = Option<(u8, Option<bool>)>;

fn read(bytes: &[u8], limits: Limits) -> Result<(Option<Branch>, u8), Error> {
    decode_projected(
        bytes,
        Encoding::Payload,
        limits,
        |root: Projected<'_, Message<'_>>, ctx| {
            root.extensions.visit(ctx, |_, _| Ok(()))?;
            let message = root.core;
            let branch = ctx.nested(|ctx| {
                message.0.project(ctx, |slot, scope| {
                    let mut active = None;
                    let fields = BranchFields::decode_extensions_in(
                        slot,
                        scope,
                        (
                            |value, _| Ok(value),
                            |detail: Detail<'_>, field_scope: &RecoveryContext<'_>| {
                                active = DetailFields::decode_extensions(
                                    detail.0,
                                    field_scope.read_context(),
                                )?
                                .active;
                                Ok(detail)
                            },
                        ),
                    )?;
                    Ok(fields.map(|fields| (fields.generation, active)))
                })
            })?;
            Ok((branch, message.1))
        },
    )
}

#[test]
fn missing_required_field_cannot_hide_a_known_nested_error() {
    let missing_generation = [1, 7, 6, 2, 4, 3, 1, 1, 1, 23, 0];
    assert_eq!(read(&missing_generation, LIMITS), Ok((None, 23)));

    let complete = [1, 10, 9, 1, 1, 7, 2, 4, 3, 1, 1, 1, 23, 0];
    assert_eq!(
        read(&complete, LIMITS),
        Ok((Some(Some((7, Some(true)))), 23))
    );

    let mut malformed = missing_generation;
    malformed[8] = 2; // Inside the recognized nested boolean's exact span.
    assert_eq!(read(&malformed, LIMITS), Err(Error::Malformed));
    assert_eq!(
        read(&missing_generation, Limits { depth: 6, ..LIMITS }),
        Err(Error::Limit(LimitKind::Depth))
    );
}
