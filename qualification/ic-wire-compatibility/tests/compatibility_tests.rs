//! Independent literal bytes, primitive rules and destination custody.
mod support;

use serde::{Deserialize, Serialize};
use wire::{Direction, Encoding, Error, Limits};

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 8192,
        nodes: 256,
        depth: 32,
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::Wire)]
#[wire(crate = "wire")]
enum Choice {
    Ready,
    Value(u16),
}
#[derive(Debug, PartialEq, wire::Wire)]
#[wire(crate = "wire")]
struct Record {
    number: u8,
    choice: Choice,
    optional: Option<u8>,
    bytes: wire::Bytes<3>,
    packed: wire::Packed<i16, 2>,
    ordinary: wire::List<i16, 2>,
}
fn value() -> Record {
    Record {
        number: 7,
        choice: Choice::Value(128),
        optional: Some(9),
        bytes: wire::Bytes::new(vec![0xaa, 0xbb]).unwrap(),
        packed: wire::Packed::new(vec![1, -1]).unwrap(),
        ordinary: wire::List::new(vec![1, -1]).unwrap(),
    }
}
const PAYLOAD: &[u8] = &[
    7, 1, 0x80, 1, 1, 9, 2, 0xaa, 0xbb, 4, 1, 0, 255, 255, 2, 2, 1, 0,
];

fn encodings() -> [Encoding; 3] {
    [
        Encoding::Payload,
        Encoding::Frame {
            method: 7,
            direction: Direction::Request,
        },
        Encoding::Frame {
            method: 7,
            direction: Direction::Response,
        },
    ]
}

fn literal(encoding: Encoding) -> Vec<u8> {
    let mut bytes = match encoding {
        Encoding::Payload => vec![],
        Encoding::Frame {
            direction: Direction::Request,
            ..
        } => vec![b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 0],
        Encoding::Frame {
            direction: Direction::Response,
            ..
        } => vec![b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 1],
    };
    bytes.extend_from_slice(PAYLOAD);
    bytes
}

#[test]
fn representative_record_matches_independent_literals() {
    for encoding in encodings() {
        let expected = literal(encoding);
        assert_eq!(
            wire::encode(&value(), encoding, limits()).unwrap(),
            expected
        );
        assert_eq!(
            wire::decode::<Record>(&expected, encoding, limits()).unwrap(),
            value()
        );
        for length in 0..expected.len() {
            assert!(wire::decode::<Record>(&expected[..length], encoding, limits()).is_err());
        }
    }
}
#[test]
fn reusable_and_slice_destinations_preserve_custody_and_bytes() {
    for encoding in encodings() {
        for limit in [
            limits(),
            Limits {
                nodes: 0,
                ..limits()
            },
            Limits {
                bytes: 10,
                ..limits()
            },
        ] {
            let expected = if limit.nodes == 0 {
                Err(Error::Limit(wire::LimitKind::Nodes))
            } else if limit.bytes == 10 {
                Err(Error::Limit(wire::LimitKind::Bytes))
            } else {
                Ok(literal(encoding))
            };
            let mut output = vec![42; 64];
            let result = wire::encode_into(&value(), encoding, limit, &mut output)
                .map(|bytes| bytes.to_vec());
            assert_eq!(result, expected);
            if expected.is_err() {
                assert!(output.is_empty());
            }
            for capacity in 0..=32 {
                let mut storage = vec![0xa5; capacity];
                let result = wire::encode_to_slice(&value(), encoding, limit, &mut storage)
                    .map(|bytes| bytes.to_vec());
                if let Ok(bytes) = &expected {
                    if capacity < bytes.len() {
                        assert_eq!(result, Err(Error::Limit(wire::LimitKind::Output)));
                    } else {
                        assert_eq!(&result, &expected);
                        assert!(storage[bytes.len()..].iter().all(|byte| *byte == 0xa5));
                    }
                } else {
                    // A caller-slice write may exhaust its capacity first.
                    assert!(matches!(result, Err(Error::Limit(_))));
                }
            }
        }
    }
}
#[test]
fn all_u16_and_i16_encodings_follow_varint_and_zigzag_rules() {
    for bits in 0..=u16::MAX {
        let mut literal = if bits < 128 {
            vec![bits as u8]
        } else if bits < 16384 {
            vec![(bits as u8 & 127) | 128, (bits >> 7) as u8]
        } else {
            vec![
                (bits as u8 & 127) | 128,
                ((bits >> 7) as u8 & 127) | 128,
                (bits >> 14) as u8,
            ]
        };
        literal.push(0);
        assert_eq!(
            support::encode(&bits, Encoding::Payload, limits()).unwrap(),
            literal
        );
        assert_eq!(
            support::decode::<u16>(&literal, Encoding::Payload, limits()).unwrap(),
            bits
        );
        let signed = ((bits >> 1) as i16) ^ -((bits & 1) as i16);
        assert_eq!(
            support::encode(&signed, Encoding::Payload, limits()).unwrap(),
            literal
        );
        assert_eq!(
            support::decode::<i16>(&literal, Encoding::Payload, limits()).unwrap(),
            signed
        );
    }
}
#[test]
fn short_primitives_follow_literal_acceptance_rules() {
    assert_eq!(
        support::decode::<u16>(&[], Encoding::Payload, limits()),
        Err(Error::Malformed)
    );
    for first in 0..=u8::MAX {
        let boolean = match first {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Malformed),
        };
        assert_eq!(
            support::decode::<bool>(&[first, 0], Encoding::Payload, limits()),
            boolean
        );
        let single = if first < 128 {
            Ok(first as u16)
        } else {
            Err(Error::Malformed)
        };
        assert_eq!(
            support::decode::<u16>(&[first, 0], Encoding::Payload, limits()),
            single
        );
        for second in 0..=u8::MAX {
            let input = if first < 128 {
                [first, 0, second]
            } else if second >= 128 {
                [first, second, 128]
            } else {
                [first, second, 0]
            };
            let expected = if first < 128 {
                Err(Error::Trailing)
            } else if second >= 128 {
                Err(Error::Malformed)
            } else {
                Ok((first & 127) as u16 | ((second as u16) << 7))
            };
            assert_eq!(
                support::decode::<u16>(&input, Encoding::Payload, limits()),
                expected
            );
        }
    }
}
#[test]
fn borrowed_bytes_retain_the_input_span() {
    let input = [3, b'a', b'b', b'c', 0];
    let decoded =
        support::decode::<wire::BytesRef<'_, 3>>(&input, Encoding::Payload, limits()).unwrap();
    assert_eq!(decoded.as_slice().as_ptr(), input[1..].as_ptr());
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    #[derive(Deserialize)]
    struct Floats(#[serde(with = "wire::serde_packed::finite_f32")] Vec<f32>);

    proptest! {
        #[test]
        fn arbitrary_payloads_and_frames(
            bytes in prop::collection::vec(any::<u8>(), 0..80),
            byte_cap in 0..64usize, nodes in 0..32usize, depth in 0..5usize,
            allocation in 0..1024usize,
        ) {
            let limits = Limits { bytes: byte_cap, allocation, nodes, depth };
            for encoding in encodings() {
                let mut frame = literal(encoding);
                frame.truncate(frame.len() - PAYLOAD.len());
                frame.extend_from_slice(&bytes);
                for input in [&bytes, &frame] {
                    let result = support::decode::<Vec<Vec<u8>>>(input, encoding, limits);
                    if input.len() > byte_cap {
                        prop_assert_eq!(result, Err(Error::Limit(wire::LimitKind::Bytes)));
                    } else if let Ok(values) = result {
                        let elements: usize = values.iter().map(Vec::len).sum();
                        let needed_depth = 1 + usize::from(!values.is_empty()) + usize::from(elements > 0);
                        prop_assert!(depth >= needed_depth);
                        let needed_nodes = 1 + 2 * (values.len() + elements);
                        prop_assert!(nodes >= needed_nodes);
                        let units = size_of::<Vec<Vec<u8>>>() + 2 * (values.len() * size_of::<Vec<u8>>() + elements);
                        prop_assert!(allocation >= byte_cap + 8 * units);
                    }
                }
            }
        }

        #[test]
        fn generated_packed_values_round_trip(words in prop::collection::vec(any::<i16>(), 0..16)) {
            let value = wire::Packed::<_, 16>::new(words).unwrap();
            for encoding in encodings() {
                let bytes = support::encode(&value, encoding, limits()).unwrap();
                let decoded = support::decode(&bytes, encoding, limits()).unwrap();
                prop_assert_eq!(&value, &decoded);
            }
        }

        #[test]
        fn packed_word_domains(bytes in prop::collection::vec(any::<u8>(), 0..40)) {
            let mut payload = vec![bytes.len() as u8];
            payload.extend_from_slice(&bytes);
            payload.push(0);
            let packed = support::decode::<wire::Packed<i16>>(&payload, Encoding::Payload, limits());
            prop_assert_eq!(packed.is_ok(), bytes.len().is_multiple_of(2));
            let floats = support::decode::<Floats>(&payload, Encoding::Payload, limits());
            let (words, tail) = bytes.as_chunks::<4>();
            let expected: Vec<_> = words.iter().map(|word| f32::from_le_bytes(*word)).collect();
            if tail.is_empty() && expected.iter().all(|word| word.is_finite()) {
                prop_assert_eq!(floats.unwrap().0, expected);
            } else {
                prop_assert!(matches!(floats, Err(Error::Malformed)));
            }
        }
    }
}
