//! Literal decoding and admission boundary cases.

use crate::{Direction, Encoding, Error, LimitKind, Limits, decode::decode};
use serde::{
    Deserialize, Deserializer,
    de::{SeqAccess, Visitor},
};
use std::{collections::BTreeMap, fmt, vec, vec::Vec};

struct NoHint;

#[derive(Debug, PartialEq)]
struct Swallowed;

impl<'de> Deserialize<'de> for Swallowed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _caught = bool::deserialize(deserializer);
        Ok(Self)
    }
}

#[test]
fn trusted_custom_serde_can_catch_refusal_as_in_the_reference() {
    assert_eq!(
        decode::<Swallowed>(
            &[],
            Encoding::Payload,
            Limits {
                nodes: 0,
                ..limits()
            }
        ),
        Ok(Swallowed)
    );
    assert_eq!(
        decode::<Swallowed>(
            &[1],
            Encoding::Payload,
            Limits {
                nodes: 0,
                ..limits()
            }
        ),
        Err(Error::Trailing)
    );
}

impl<'de> Deserialize<'de> for NoHint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Probe;
        impl<'de> Visitor<'de> for Probe {
            type Value = NoHint;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a sequence without a size hint")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<NoHint, A::Error> {
                assert_eq!(access.size_hint(), None);
                while access.next_element::<()>()?.is_some() {}
                Ok(NoHint)
            }
        }
        deserializer.deserialize_seq(Probe)
    }
}

#[test]
fn attacker_count_is_hidden_and_work_stops_at_node_budget() {
    // A declared count of 16383 zero-width elements, with only three nodes.
    assert!(matches!(
        decode::<NoHint>(
            &[0xff, 0x7f],
            Encoding::Payload,
            Limits {
                nodes: 3,
                ..limits()
            }
        ),
        Err(Error::Limit(LimitKind::Nodes))
    ));
}

#[test]
fn map_keys_and_values_each_pay_seed_and_visitor_charges() {
    let exact = Limits {
        bytes: 3,
        allocation: 3 + 8 * (size_of::<BTreeMap<u8, u8>>() + 4),
        nodes: 5,
        depth: 2,
    };
    assert_eq!(
        decode::<BTreeMap<u8, u8>>(&[1, 2, 3], Encoding::Payload, exact),
        Ok(BTreeMap::from([(2, 3)]))
    );
    assert_eq!(
        decode::<BTreeMap<u8, u8>>(&[1, 2, 3], Encoding::Payload, Limits { nodes: 4, ..exact }),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert_eq!(
        decode::<BTreeMap<u8, u8>>(
            &[1, 2, 3],
            Encoding::Payload,
            Limits {
                allocation: exact.allocation - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

fn limits() -> Limits {
    Limits {
        bytes: 64,
        allocation: 4096,
        nodes: 64,
        depth: 16,
    }
}

#[test]
fn literal_payloads_preserve_supplier_acceptance() {
    assert_eq!(
        decode::<u16>(&[0x80, 1], Encoding::Payload, limits()),
        Ok(128)
    );
    assert_eq!(
        decode::<u16>(&[0x80, 0], Encoding::Payload, limits()),
        Ok(0)
    );
    assert_eq!(
        decode::<char>(&[2, b'a', b'b'], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<char>(&[2, 0xc3, 0xa9], Encoding::Payload, limits()),
        Ok('é')
    );
    assert_eq!(
        decode::<(u8, Option<u16>, Vec<u8>)>(
            &[7, 1, 0x80, 1, 2, 9, 10],
            Encoding::Payload,
            limits()
        ),
        Ok((7, Some(128), vec![9, 10]))
    );
}

#[test]
fn scalar_admission_has_exact_boundaries() {
    let exact = Limits {
        bytes: 1,
        allocation: 17,
        nodes: 1,
        depth: 1,
    };
    assert_eq!(decode::<u16>(&[7], Encoding::Payload, exact), Ok(7));
    for (insufficient, reason) in [
        Limits { bytes: 0, ..exact },
        Limits {
            allocation: 16,
            ..exact
        },
        Limits { nodes: 0, ..exact },
        Limits { depth: 0, ..exact },
    ]
    .into_iter()
    .zip([
        LimitKind::Bytes,
        LimitKind::Allocation,
        LimitKind::Nodes,
        LimitKind::Depth,
    ]) {
        assert_eq!(
            decode::<u16>(&[7], Encoding::Payload, insufficient),
            Err(Error::Limit(reason))
        );
    }
    // Reserve the configured bytes, not actual input length.
    assert_eq!(
        decode::<u16>(&[7], Encoding::Payload, Limits { bytes: 2, ..exact }),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

#[test]
fn borrowed_spans_keep_input_custody_and_pay_for_the_span() {
    let input = [3, b'a', b'b', b'c'];
    let exact = Limits {
        bytes: 4,
        allocation: 4 + 8 * (size_of::<&str>() + 3),
        nodes: 1,
        depth: 1,
    };
    let text: &str = decode(&input, Encoding::Payload, exact).unwrap();
    assert_eq!(text, "abc");
    assert_eq!(text.as_ptr(), input[1..].as_ptr());
    let bytes: &[u8] = decode(&input, Encoding::Payload, exact).unwrap();
    assert_eq!(bytes.as_ptr(), input[1..].as_ptr());
    assert_eq!(
        decode::<&str>(
            &input,
            Encoding::Payload,
            Limits {
                allocation: exact.allocation - 1,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}

#[test]
fn zero_sized_elements_still_pay_seed_and_visitor_nodes() {
    let exact = Limits {
        bytes: 1,
        allocation: 1 + 8 * size_of::<Vec<()>>(),
        nodes: 7,
        depth: 2,
    };
    assert_eq!(
        decode::<Vec<()>>(&[3], Encoding::Payload, exact),
        Ok(vec![(); 3])
    );
    for (insufficient, reason) in [Limits { nodes: 6, ..exact }, Limits { depth: 1, ..exact }]
        .into_iter()
        .zip([LimitKind::Nodes, LimitKind::Depth])
    {
        assert_eq!(
            decode::<Vec<()>>(&[3], Encoding::Payload, insufficient),
            Err(Error::Limit(reason))
        );
    }
}

#[test]
fn malformed_trailing_and_budget_precedence() {
    for input in [&[2][..], &[][..]] {
        assert_eq!(
            decode::<bool>(input, Encoding::Payload, limits()),
            Err(Error::Malformed)
        );
        assert_eq!(
            decode::<bool>(
                input,
                Encoding::Payload,
                Limits {
                    nodes: 0,
                    ..limits()
                }
            ),
            Err(Error::Limit(LimitKind::Nodes))
        );
    }
    assert_eq!(
        decode::<bool>(&[1, 0], Encoding::Payload, limits()),
        Err(Error::Trailing)
    );
    assert_eq!(
        decode::<&str>(&[1, 255], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    assert_eq!(
        decode::<u16>(&[0x80], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
}

#[test]
fn frame_identity_precedes_payload_admission_but_follows_size() {
    let request = Encoding::Frame {
        method: 7,
        direction: Direction::Request,
    };
    let mut frame = [b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 0, 42];
    let exact = Limits {
        bytes: 12,
        allocation: 20,
        nodes: 1,
        depth: 1,
    };
    assert_eq!(decode::<u8>(&frame, request, exact), Ok(42));
    frame[10] = 1;
    assert_eq!(
        decode::<u8>(
            &frame,
            Encoding::Frame {
                method: 7,
                direction: Direction::Response
            },
            exact
        ),
        Ok(42)
    );
    assert_eq!(
        decode::<u8>(
            &frame,
            request,
            Limits {
                allocation: 0,
                ..exact
            }
        ),
        Err(Error::Identity)
    );
    assert_eq!(
        decode::<u8>(&frame, request, Limits { bytes: 11, ..exact }),
        Err(Error::Limit(LimitKind::Bytes))
    );
    assert_eq!(
        decode::<u8>(
            &frame[..10],
            request,
            Limits {
                allocation: 0,
                ..exact
            }
        ),
        Err(Error::Malformed)
    );
    frame[10] = 0;
    assert_eq!(
        decode::<u8>(
            &frame,
            request,
            Limits {
                allocation: 10,
                ..exact
            }
        ),
        Err(Error::Limit(LimitKind::Allocation))
    );
}
