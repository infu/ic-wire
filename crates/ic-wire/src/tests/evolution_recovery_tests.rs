//! Bounded optional recovery across explicit projected DTOs.

use crate::{
    Encoding, Error, LimitKind, Limits, List, TextRef,
    evolution::{
        Extensions, OpaqueOpen, Recoverable, RecoveryContext, decode_value, encode_value_into,
    },
};
use serde::Deserialize;
use std::vec::Vec;

const LIMITS: Limits = Limits {
    bytes: 96,
    allocation: 16_384,
    nodes: 1_024,
    depth: 16,
};

#[derive(Deserialize)]
struct Packet<'a> {
    #[serde(borrow)]
    branch: Recoverable<'a>,
    #[serde(borrow)]
    outside: OpaqueOpen<'a>,
}

#[derive(Deserialize)]
struct Row<'a> {
    #[serde(borrow)]
    state: OpaqueOpen<'a>,
    #[serde(borrow)]
    note: TextRef<'a, 8>,
}

#[derive(Deserialize)]
struct Body<'a> {
    #[serde(borrow)]
    rows: List<Row<'a>, 4>,
    #[serde(borrow)]
    outcome: Result<OpaqueOpen<'a>, OpaqueOpen<'a>>,
}

fn known(open: OpaqueOpen<'_>, scope: &RecoveryContext<'_>) -> Result<Option<()>, Error> {
    scope.required_open(open, |tag, field| match tag {
        1 => field.decode::<()>().map(Some),
        _ => Ok(None),
    })
}

fn decode_list_result(bytes: &[u8], limits: Limits) -> Result<Option<usize>, Error> {
    decode_value(
        bytes,
        Encoding::Payload,
        limits,
        |packet: Packet<'_>, read| {
            RecoveryContext::check(read, |scope| {
                let branch = packet.branch.project(read, |body: Body<'_>, scope| {
                    let mut note_bytes = 0;
                    scope.nested(|rows| {
                        for row in body.rows.as_slice() {
                            rows.nested(|row_scope| {
                                known(row.state, row_scope)?;
                                note_bytes += row.note.as_str().len();
                                Ok(())
                            })?;
                        }
                        Ok(())
                    })?;
                    scope.nested(|result| {
                        let selected = match body.outcome {
                            Ok(state) | Err(state) => state,
                        };
                        known(selected, result)?;
                        Ok(())
                    })?;
                    Ok(note_bytes)
                })?;
                known(packet.outside, scope)?;
                Ok(branch)
            })
        },
    )
}

#[test]
fn unknown_required_tags_in_lists_and_results_recover_the_enclosing_branch() {
    // Two rows. The first carries an unfamiliar extension with an opaque body.
    // The second carries a known unit and a known text sibling. The selected
    // Result::Err payload is a separate required open variant.
    let unknown_row = [1, 12, 2, 11, 2, 0xff, 0, 1, b'x', 2, 1, b'y', 1, 2, 2];
    assert_eq!(decode_list_result(&unknown_row, LIMITS), Ok(None));

    let unknown_result = [1, 9, 2, 2, 1, b'x', 2, 1, b'y', 1, 10, 2];
    assert_eq!(decode_list_result(&unknown_result, LIMITS), Ok(None));

    let known = [1, 9, 2, 2, 1, b'x', 2, 1, b'y', 1, 2, 2];
    assert_eq!(decode_list_result(&known, LIMITS), Ok(Some(2)));
    assert_eq!(decode_list_result(&[0, 2], LIMITS), Ok(None));
}

#[test]
fn hard_failures_in_known_siblings_and_bounded_extents_survive_recovery() {
    let malformed_note = [1, 9, 2, 10, 1, b'x', 2, 1, 0xff, 1, 2, 2];
    let absent_required = [1, 9, 2, 10, 1, b'x', 0, 1, b'y', 1, 2, 2];
    let invalid_known_variant = [1, 9, 2, 10, 1, b'x', 2, 1, b'y', 1, 2, 3, 1, 0];
    let trailing_in_branch = [1, 10, 2, 10, 1, b'x', 2, 1, b'y', 1, 2, 0, 2];
    for input in [
        malformed_note.as_slice(),
        &absent_required,
        &invalid_known_variant,
        &trailing_in_branch,
    ] {
        assert_eq!(decode_list_result(input, LIMITS), Err(Error::Malformed));
    }

    let mut bytes = [1, 9, 2, 10, 1, b'x', 2, 1, b'y', 1, 2, 2, 0];
    assert_eq!(decode_list_result(&bytes, LIMITS), Err(Error::Trailing));
    bytes[1] = 20;
    assert_eq!(decode_list_result(&bytes, LIMITS), Err(Error::Malformed));
    assert_eq!(
        decode_list_result(
            &bytes,
            Limits {
                bytes: bytes.len() - 1,
                ..LIMITS
            }
        ),
        Err(Error::Limit(LimitKind::Bytes))
    );
    let unknown_branch = [1, 9, 2, 10, 1, b'x', 2, 1, b'y', 1, 2, 2];
    let exact = Limits {
        nodes: 51,
        ..LIMITS
    };
    assert_eq!(decode_list_result(&unknown_branch, exact), Ok(None));
    assert_eq!(
        decode_list_result(&unknown_branch[..unknown_branch.len() - 1], exact),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode_list_result(
            &unknown_branch,
            Limits {
                nodes: exact.nodes - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Nodes))
    );
}

#[derive(Deserialize)]
struct Nested<'a> {
    prefix: u32,
    #[serde(borrow)]
    branch: Recoverable<'a>,
    #[serde(borrow)]
    suffix: OpaqueOpen<'a>,
}

#[test]
fn the_nearest_explicit_branch_recovers_without_marking_its_parent() {
    let bytes = [1, 6, 42, 1, 2, 10, 1, 2, 2];
    let outcome = decode_value(
        &bytes,
        Encoding::Payload,
        LIMITS,
        |packet: Packet<'_>, read| {
            RecoveryContext::check(read, |scope| {
                let branch = packet.branch.project(read, |outer: Nested<'_>, scope| {
                    let inner = outer.branch.project(
                        scope.read_context(),
                        |(state, flag): (OpaqueOpen<'_>, bool), child| {
                            known(state, child)?;
                            Ok(flag)
                        },
                    )?;
                    let suffix = known(outer.suffix, scope)?;
                    Ok((outer.prefix, inner, suffix))
                })?;
                let outside = known(packet.outside, scope)?;
                Ok((branch, outside))
            })
        },
    );
    assert_eq!(outcome, Ok((Some((42, None, Some(()))), Some(()))));
}

#[test]
fn marks_cross_known_variant_payloads_and_scope_absence_is_distinct_from_unit() {
    // The outer branch holds tag 1 with a typed open variant payload. The
    // selected inner tag 5 is unfamiliar to this schema.
    let bytes = [1, 3, 3, 1, 10, 2];
    let outcome = decode_value(
        &bytes,
        Encoding::Payload,
        LIMITS,
        |packet: Packet<'_>, read| {
            packet.branch.project(read, |outer: OpaqueOpen<'_>, scope| {
                scope.required_open(outer, |tag, field| {
                    assert_eq!(tag, 1);
                    scope.project_field(field, |inner: OpaqueOpen<'_>, child| {
                        known(inner, child)?;
                        Ok(Some(()))
                    })
                })
            })
        },
    );
    assert_eq!(outcome, Ok(None));

    let mut scratch = [];
    let mut output = Vec::new();
    let written = encode_value_into(&mut output, Encoding::Payload, LIMITS, |write| {
        Recoverable::write(write, &(), &mut scratch)
    })
    .unwrap();
    assert_eq!(written, &[1, 0]);
    for (bytes, expected) in [(written, Some(())), (&[0], None)] {
        assert_eq!(
            decode_value(
                bytes,
                Encoding::Payload,
                LIMITS,
                |branch: Recoverable<'_>, read| branch.project(read, |(): (), _| Ok(())),
            ),
            Ok(expected)
        );
    }
    assert_eq!(
        encode_value_into(&mut output, Encoding::Payload, LIMITS, |_| {
            Ok(Recoverable::absent())
        }),
        Ok(&[0][..])
    );

    let mut scratch = [0; 2];
    let written = encode_value_into(&mut output, Encoding::Payload, LIMITS, |write| {
        Recoverable::write(write, &(7_u32, true), &mut scratch)
    })
    .unwrap();
    assert_eq!(written, &[1, 2, 7, 1]);
}

#[test]
fn strict_scopes_finish_validation_before_reporting_incompatibility() {
    let unknown_outside = [0, 10];
    assert_eq!(
        decode_list_result(&unknown_outside, LIMITS),
        Err(Error::Incompatible)
    );
    assert_eq!(
        decode_value(
            &[10, 3, 1, 0],
            Encoding::Payload,
            LIMITS,
            |(first, second): (OpaqueOpen<'_>, OpaqueOpen<'_>), read| {
                RecoveryContext::check(read, |scope| {
                    known(first, scope)?;
                    known(second, scope)?;
                    Ok(())
                })
            },
        ),
        Err(Error::Malformed)
    );

    assert_eq!(
        decode_value(
            &[1, 0],
            Encoding::Payload,
            LIMITS,
            |branch: Recoverable<'_>, read| {
                branch.project(read, |(): (), _| Err::<(), _>(Error::Incompatible))
            },
        ),
        Err(Error::Incompatible)
    );
}

fn require_epoch(
    fields: Extensions<'_>,
    scope: &RecoveryContext<'_>,
) -> Result<Option<u32>, Error> {
    let mut epoch = None;
    fields.visit(scope.read_context(), |id, field| {
        match id {
            2 => epoch = Some(field.decode::<u32>()?),
            3 => {
                field.decode::<bool>()?;
            }
            _ => {}
        }
        Ok(())
    })?;
    if epoch.is_none() {
        scope.missing_required();
    }
    Ok(epoch)
}

#[test]
fn missing_required_extension_recovers_only_after_complete_validation() {
    let decode = |bytes: &[u8]| {
        decode_value(
            bytes,
            Encoding::Payload,
            LIMITS,
            |packet: Packet<'_>, read| {
                RecoveryContext::check(read, |scope| {
                    let branch = packet.branch.project(
                        read,
                        |(fields, state): (Extensions<'_>, OpaqueOpen<'_>), branch| {
                            let epoch = require_epoch(fields, branch)?;
                            known(state, branch)?;
                            Ok(epoch)
                        },
                    )?;
                    known(packet.outside, scope)?;
                    Ok(branch.flatten())
                })
            },
        )
    };

    // Epoch ID 2 is missing. The greater known ID 3 and both subsequent
    // known variants still need validation.
    assert_eq!(decode(&[1, 5, 3, 3, 1, 1, 2, 2]), Ok(None));
    assert_eq!(decode(&[1, 8, 6, 2, 1, 7, 3, 1, 1, 2, 2]), Ok(Some(7)));
    for malformed in [
        &[1, 5, 3, 3, 1, 0xff, 2, 2][..], // Known bool late in the block.
        &[1, 6, 4, 3, 1, 1, 4, 2, 2],     // Unknown field header is incomplete.
        &[1, 7, 3, 3, 1, 1, 3, 1, 0, 2],  // Known sibling within the branch.
        &[1, 5, 3, 3, 1, 1, 2, 3, 1, 0],  // Known sibling outside the branch.
    ] {
        assert_eq!(decode(malformed), Err(Error::Malformed));
    }

    assert_eq!(
        decode_value(
            &[3, 3, 1, 1, 2],
            Encoding::Payload,
            LIMITS,
            |(fields, state): (Extensions<'_>, OpaqueOpen<'_>), read| {
                RecoveryContext::check(read, |scope| {
                    require_epoch(fields, scope)?;
                    known(state, scope)?;
                    Ok(())
                })
            },
        ),
        Err(Error::Incompatible)
    );
}
