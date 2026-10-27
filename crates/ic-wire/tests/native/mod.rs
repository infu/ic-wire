//! Native Serde admission, wire compatibility and failure boundaries.
mod adversarial;
#[cfg(feature = "typescript")]
mod bulk_lint;
mod packed;
#[cfg(feature = "typescript")]
mod payload;
#[cfg(feature = "typescript")]
mod registry;
mod slice;
use ic_wire::{Error, Limits, native};
use serde::{Deserialize, Serialize};

const LIMITS: Limits = Limits {
    bytes: 4096,
    allocation: 64 * 1024,
    nodes: 4096,
    depth: 64,
};

#[test]
fn u64_shortcut_preserves_postcard_bytes_and_admission_at_boundaries() {
    let mut values: Vec<u64> = (0..=255).collect();
    for shift in (7..64).step_by(7) {
        let boundary = 1_u64 << shift;
        values.extend([boundary - 1, boundary, boundary + 1]);
    }
    values.push(u64::MAX);
    // The charge remains that of u64 even when its Postcard representation is
    // one byte: four nodes, depth three, and two eight-byte value charges x8.
    let exact = Limits {
        bytes: 32,
        allocation: 160,
        nodes: 4,
        depth: 3,
    };
    for &value in &values {
        let expected = postcard::to_allocvec(&value).unwrap();
        assert_eq!(native::encode(&value, exact).unwrap(), expected);
        assert_eq!(native::encoded_len(&value, exact).unwrap(), expected.len());
        assert_eq!(native::decode::<u64>(&expected, exact).unwrap(), value);
        for short in [
            Limits {
                allocation: 159,
                ..exact
            },
            Limits { nodes: 3, ..exact },
            Limits { depth: 2, ..exact },
        ] {
            assert_eq!(native::encode(&value, short), Err(Error::Limit));
        }
    }
    let expected = postcard::to_allocvec(&values).unwrap();
    assert_eq!(native::encode(&values, LIMITS).unwrap(), expected);
    assert_eq!(
        native::decode::<Vec<u64>>(&expected, LIMITS).unwrap(),
        values
    );
}
#[test]
fn integer_shortcuts_preserve_postcard_bytes_and_original_budgets() {
    macro_rules! check_values {
        ($ty:ty, $values:expr) => {{
            let exact = Limits {
                bytes: 32,
                allocation: 32 + 16 * size_of::<$ty>(),
                nodes: 4,
                depth: 3,
            };
            for value in $values {
                let expected = postcard::to_allocvec(&value).unwrap();
                assert_eq!(native::encode(&value, exact).unwrap(), expected);
                assert_eq!(native::encoded_len(&value, exact).unwrap(), expected.len());
                assert_eq!(native::decode::<$ty>(&expected, exact).unwrap(), value);
                for short in [
                    Limits {
                        allocation: exact.allocation - 1,
                        ..exact
                    },
                    Limits { nodes: 3, ..exact },
                    Limits { depth: 2, ..exact },
                ] {
                    assert_eq!(native::encode(&value, short), Err(Error::Limit));
                }
            }
            let values: Vec<$ty> = (0..=255).collect();
            let expected = postcard::to_allocvec(&values).unwrap();
            assert_eq!(native::encode(&values, LIMITS).unwrap(), expected);
            assert_eq!(
                native::decode::<Vec<$ty>>(&expected, LIMITS).unwrap(),
                values
            );
        }};
    }
    check_values!(u16, 0..=u16::MAX);
    // Exhaust the signed domain too: the direct writer must preserve all three
    // varint widths and charge i16 admission rather than its raw output bytes.
    check_values!(i16, i16::MIN..=i16::MAX);
    let mut values: Vec<u32> = (0..=255).collect();
    for shift in (7..32).step_by(7) {
        let boundary = 1_u32 << shift;
        values.extend([boundary - 1, boundary, boundary + 1]);
    }
    values.push(u32::MAX);
    check_values!(u32, values);
    let mut signed_values: Vec<i64> = (-128..=127).collect();
    // Zigzag changes width at signed magnitudes 2^6, 2^13, ... .
    for shift in (6..63).step_by(7) {
        let boundary = 1_i64 << shift;
        signed_values.extend([
            -boundary - 1,
            -boundary,
            -boundary + 1,
            boundary - 1,
            boundary,
            boundary + 1,
        ]);
    }
    signed_values.extend([i64::MIN, i64::MAX]);
    check_values!(i64, signed_values);
    // i8 is not zigzag encoded by Postcard and must not take this route.
    assert_eq!(native::encode(&-1_i8, LIMITS).unwrap(), [0xff]);
}
#[derive(Debug, Deserialize, PartialEq, Serialize)]
enum Tree {
    Leaf(String, u128),
    Branch(Vec<Self>),
}
#[derive(Serialize)]
struct Borrowed<'a> {
    label: &'a str,
    values: &'a [u64],
}
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Owned {
    label: String,
    values: Vec<u64>,
}
#[test]
fn borrowed_frames_match_owned_postcard_and_frozen_envelope() {
    let value = Borrowed {
        label: "é🦀",
        values: &[0, 128, u64::MAX],
    };
    let raw = postcard::to_allocvec(&value).unwrap();
    assert_eq!(native::encode(&value, LIMITS).unwrap(), raw);
    let expected = Owned {
        label: value.label.into(),
        values: value.values.to_vec(),
    };
    assert_eq!(native::decode::<Owned>(&raw, LIMITS).unwrap(), expected);
    let frame = native::encode_request(7, &value, LIMITS).unwrap();
    assert_eq!(&frame[11..], raw);
    assert_eq!(
        native::encoded_frame_len(&value, LIMITS).unwrap(),
        frame.len()
    );
    assert_eq!(native::encoded_len(&value, LIMITS).unwrap(), raw.len());
    assert_eq!(
        native::decode_request::<Owned>(7, &frame, LIMITS).unwrap(),
        expected
    );
    assert_eq!(
        native::decode_response::<Owned>(7, &frame, LIMITS),
        Err(Error::Identity)
    );
    assert_eq!(
        native::decode_request::<Owned>(8, &frame, LIMITS),
        Err(Error::Identity)
    );
    let response = native::encode_response(7, &value, LIMITS).unwrap();
    assert_eq!(
        native::decode_response::<Owned>(7, &response, LIMITS).unwrap(),
        expected
    );
    for n in [0_u64, 127, 128, u64::MAX] {
        for direction in [0_u8, 1] {
            let mut expected = vec![0x49, 0x57];
            expected.extend_from_slice(&7_u64.to_le_bytes());
            expected.push(direction);
            expected.extend(postcard::to_allocvec(&n).unwrap());
            let frame = if direction == 0 {
                native::encode_request(7, &n, LIMITS)
            } else {
                native::encode_response(7, &n, LIMITS)
            }
            .unwrap();
            assert_eq!(frame, expected);
        }
    }
    for end in 0..frame.len() {
        assert!(native::decode_request::<Owned>(7, &frame[..end], LIMITS).is_err());
    }
    for index in 0..frame.len() {
        for mask in [1, 0x80, 0xff] {
            let mut changed = frame.clone();
            changed[index] ^= mask;
            if let Ok(value) = native::decode_request::<Owned>(7, &changed, LIMITS) {
                assert_eq!(native::encode_request(7, &value, LIMITS).unwrap(), changed);
            }
        }
    }
    let mut trailing = frame;
    trailing.push(0);
    assert_eq!(
        native::decode_request::<Owned>(7, &trailing, LIMITS),
        Err(Error::Trailing)
    );
    assert_eq!(
        native::decode::<u64>(&[0x80, 0], LIMITS),
        Err(Error::Malformed)
    );
    assert_eq!(
        native::decode::<String>(&[1, 0xff], LIMITS),
        Err(Error::Malformed)
    );
}

#[test]
fn decode_only_types_and_independent_decode_allowances() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct DecodeOnly(u64);
    let bytes = postcard::to_allocvec(&u64::MAX).unwrap();
    let exact = Limits {
        bytes: 10,
        allocation: 74,
        nodes: 1,
        depth: 1,
    };
    assert_eq!(native::decode::<u64>(&bytes, exact), Ok(u64::MAX));
    for refused in [
        Limits {
            allocation: 73,
            ..exact
        },
        Limits { nodes: 0, ..exact },
        Limits { depth: 0, ..exact },
    ] {
        assert_eq!(native::decode::<u64>(&bytes, refused), Err(Error::Limit));
    }
    assert_eq!(
        native::decode::<DecodeOnly>(&bytes, LIMITS),
        Ok(DecodeOnly(u64::MAX))
    );
    let frame = native::encode_request(7, &u64::MAX, LIMITS).unwrap();
    assert_eq!(
        native::decode_request::<DecodeOnly>(7, &frame, LIMITS),
        Ok(DecodeOnly(u64::MAX))
    );
    let frame = native::encode_response(7, &u64::MAX, LIMITS).unwrap();
    assert_eq!(
        native::decode_response::<DecodeOnly>(7, &frame, LIMITS),
        Ok(DecodeOnly(u64::MAX))
    );
}

#[test]
fn strict_integer_reads_reject_every_redundant_u16_encoding() {
    for value in 0..=u16::MAX {
        let mut bytes = postcard::to_allocvec(&value).unwrap();
        assert_eq!(native::decode::<u16>(&bytes, LIMITS), Ok(value));
        *bytes.last_mut().unwrap() |= 128;
        bytes.push(0);
        assert_eq!(native::decode::<u16>(&bytes, LIMITS), Err(Error::Malformed));
    }
    for value in [0, 127, 128, u64::MAX as u128, u128::MAX] {
        let mut bytes = postcard::to_allocvec(&value).unwrap();
        assert_eq!(native::decode::<u128>(&bytes, LIMITS), Ok(value));
        *bytes.last_mut().unwrap() |= 128;
        bytes.push(0);
        assert_eq!(
            native::decode::<u128>(&bytes, LIMITS),
            Err(Error::Malformed)
        );
    }
}

#[test]
fn strict_parsing_allows_consumer_normalization() {
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(from = "u8")]
    struct Reduced(u8);
    impl From<u8> for Reduced {
        fn from(value: u8) -> Self {
            Self(value % 10)
        }
    }
    // A shortest-form primitive can still be changed by the consumer's visitor.
    assert_eq!(native::decode::<Reduced>(&[17], LIMITS), Ok(Reduced(7)));
    assert_eq!(native::decode::<Reduced>(&[7], LIMITS), Ok(Reduced(7)));
    // Collections can reorder or collapse entries without a parse error.
    for (bytes, expected) in [([2, 2, 1], vec![1, 2]), ([2, 1, 1], vec![1])] {
        assert_eq!(
            native::decode::<std::collections::BTreeSet<u8>>(&bytes, LIMITS),
            Ok(expected.into_iter().collect())
        );
    }
    // Upstream Postcard takes the first scalar from this multi-character string.
    assert_eq!(
        native::decode::<char>(&[2, b'a', b'b'], LIMITS),
        Err(Error::Malformed)
    );
    for value in ['a', 'é', '界', '🦀'] {
        let bytes = native::encode(&value, LIMITS).unwrap();
        assert_eq!(native::decode::<char>(&bytes, LIMITS), Ok(value));
    }
}

#[test]
fn recursive_and_zero_byte_values_cannot_bypass_budgets() {
    let mut tree = Tree::Leaf("leaf".into(), u128::MAX);
    let mut admitted = 0;
    for _ in 0..40 {
        let raw = postcard::to_allocvec(&tree).unwrap();
        match native::encode(&tree, LIMITS) {
            Ok(bytes) => {
                admitted += 1;
                assert_eq!(bytes, raw);
                assert_eq!(native::decode::<Tree>(&bytes, LIMITS).unwrap(), tree);
            }
            Err(error) => assert_eq!(error, Error::Limit),
        }
        tree = Tree::Branch(vec![tree]);
    }
    assert!(admitted > 0 && admitted < 40);
    assert_eq!(
        native::decode::<Tree>(&postcard::to_allocvec(&tree).unwrap(), LIMITS),
        Err(Error::Limit)
    );
    let limits = Limits {
        nodes: 16,
        ..LIMITS
    };
    let units = vec![(); 100];
    assert_eq!(native::encode(&units, limits), Err(Error::Limit));
    assert_eq!(
        native::decode::<Vec<()>>(&postcard::to_allocvec(&units).unwrap(), limits),
        Err(Error::Limit)
    );
    let forged = postcard::to_allocvec(&u64::MAX).unwrap();
    assert!(native::decode::<Vec<()>>(&forged, limits).is_err());
    assert!(native::decode::<Vec<u128>>(&forged, limits).is_err());
    assert!(native::decode::<String>(&forged, limits).is_err());
}

#[test]
fn every_allowance_is_enforced_and_empty_values_remain_valid() {
    let map = std::collections::BTreeMap::from([
        (
            "ok".to_owned(),
            Some(Ok::<_, (u8, String)>(vec![i64::MIN, 0, i64::MAX])),
        ),
        ("err".to_owned(), Some(Err((7, "reason".to_owned())))),
        ("none".to_owned(), None),
    ]);
    let bytes = native::encode(&map, LIMITS).unwrap();
    assert_eq!(bytes, postcard::to_allocvec(&map).unwrap());
    assert_eq!(native::decode::<std::collections::BTreeMap<String, Option<Result<Vec<i64>, (u8, String)>>>>(&bytes, LIMITS).unwrap(), map);
    // Consumers now own ordering and duplicate-key normalization.
    for (bytes, expected) in [
        ([2, 2, 20, 1, 10], vec![(1, 10), (2, 20)]),
        ([2, 1, 10, 1, 20], vec![(1, 20)]),
    ] {
        assert_eq!(
            native::decode::<std::collections::BTreeMap<u8, u8>>(&bytes, LIMITS),
            Ok(expected.into_iter().collect())
        );
    }
    let text = "x".repeat(128);
    let raw = postcard::to_allocvec(&text).unwrap();
    for limits in [
        Limits { bytes: 1, ..LIMITS },
        Limits {
            allocation: 8,
            ..LIMITS
        },
        Limits { nodes: 0, ..LIMITS },
        Limits { depth: 0, ..LIMITS },
    ] {
        assert_eq!(native::encode(&text, limits), Err(Error::Limit));
        assert_eq!(native::decode::<String>(&raw, limits), Err(Error::Limit));
    }
    assert_eq!(native::encode(&(), LIMITS).unwrap(), Vec::<u8>::new());
    native::decode::<()>(&[], LIMITS).unwrap();
    for bytes in 0..11 {
        let limits = Limits { bytes, ..LIMITS };
        assert_eq!(native::encode_request(7, &(), limits), Err(Error::Limit));
        assert_eq!(native::encoded_frame_len(&(), limits), Err(Error::Limit));
    }
    let exact = Limits {
        bytes: raw.len() + 11,
        ..LIMITS
    };
    assert_eq!(
        native::encode_request(7, &text, exact).unwrap().len(),
        exact.bytes
    );
    assert_eq!(
        native::encode_request(
            7,
            &text,
            Limits {
                bytes: exact.bytes - 1,
                ..exact
            }
        ),
        Err(Error::Limit)
    );
    let floats = [-0.0f64, f64::from_bits(1), f64::MAX];
    let bytes = native::encode(&floats, LIMITS).unwrap();
    let decoded: [f64; 3] = native::decode(&bytes, LIMITS).unwrap();
    assert_eq!(decoded.map(f64::to_bits), floats.map(f64::to_bits));
}

#[test]
fn frame_budgets_agree_at_header_boundary_and_preserve_error_precedence() {
    let exact = Limits {
        bytes: 11,
        allocation: 11,
        ..LIMITS
    };
    let request = native::encode_request(7, &(), exact).unwrap();
    let response = native::encode_response(7, &(), exact).unwrap();
    assert_eq!(native::encoded_frame_len(&(), exact), Ok(11));
    assert_eq!(native::decode_request::<()>(7, &request, exact), Ok(()));
    assert_eq!(native::decode_response::<()>(7, &response, exact), Ok(()));
    for short in 0..11 {
        let bytes = Limits {
            bytes: short,
            ..exact
        };
        assert_eq!(native::encoded_frame_len(&(), bytes), Err(Error::Limit));
        assert_eq!(native::encode_request(7, &(), bytes), Err(Error::Limit));
        assert_eq!(
            native::decode_request::<()>(7, &request, bytes),
            Err(Error::Limit)
        );
        let allocation = Limits {
            allocation: short,
            ..exact
        };
        assert_eq!(
            native::encoded_frame_len(&(), allocation),
            Err(Error::Limit)
        );
        assert_eq!(
            native::encode_response(7, &(), allocation),
            Err(Error::Limit)
        );
        assert_eq!(
            native::decode_request::<()>(7, &request, allocation),
            Err(Error::Limit)
        );
        // Identity and malformed-header checks still precede allocation admission.
        assert_eq!(
            native::decode_request::<()>(8, &request, allocation),
            Err(Error::Identity)
        );
        assert_eq!(
            native::decode_response::<()>(7, &request, allocation),
            Err(Error::Identity)
        );
        assert_eq!(
            native::decode_request::<()>(7, &request[..short], allocation),
            Err(Error::Malformed)
        );
    }
}

#[test]
fn scalar_admission_preserves_exact_resource_boundaries() {
    let limits = Limits {
        bytes: 10,
        allocation: 138,
        nodes: 4,
        depth: 3,
    };
    let bytes = postcard::to_allocvec(&u64::MAX).unwrap();
    assert_eq!(native::encoded_len(&u64::MAX, limits), Ok(10));
    assert_eq!(native::encode(&u64::MAX, limits).unwrap(), bytes);
    assert_eq!(native::decode::<u64>(&bytes, limits), Ok(u64::MAX));
    for refused in [
        Limits { nodes: 3, ..limits },
        Limits {
            allocation: 137,
            ..limits
        },
        Limits { depth: 2, ..limits },
    ] {
        assert_eq!(native::encoded_len(&u64::MAX, refused), Err(Error::Limit));
        assert_eq!(native::encode(&u64::MAX, refused), Err(Error::Limit));
    }
}

#[test]
fn borrowed_decode_points_into_the_frame_and_preserves_rejections() {
    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct View<'a> {
        text: &'a str,
        bytes: &'a [u8],
    }
    let value = View {
        text: "é🦀",
        bytes: &[0, 127, 255],
    };
    let frame = native::encode_request(7, &value, LIMITS).unwrap();
    let view: View<'_> = native::decode_request(7, &frame, LIMITS).unwrap();
    assert_eq!(view, value);
    let range = frame.as_ptr() as usize..frame.as_ptr() as usize + frame.len();
    assert!(range.contains(&(view.text.as_ptr() as usize)));
    assert!(range.contains(&(view.bytes.as_ptr() as usize)));
    for end in 0..frame.len() {
        assert!(native::decode_request::<View<'_>>(7, &frame[..end], LIMITS).is_err());
    }
    for limits in [
        Limits { nodes: 0, ..LIMITS },
        Limits {
            allocation: 0,
            ..LIMITS
        },
        Limits { depth: 0, ..LIMITS },
    ] {
        assert_eq!(
            native::decode_request::<View<'_>>(7, &frame, limits),
            Err(Error::Limit)
        );
    }
    let mut trailing = frame.clone();
    trailing.push(0);
    assert_eq!(
        native::decode_request::<View<'_>>(7, &trailing, LIMITS),
        Err(Error::Trailing)
    );
    assert_eq!(
        native::decode_response::<View<'_>>(7, &frame, LIMITS),
        Err(Error::Identity)
    );
    let mut overlong = frame[..11].to_vec();
    overlong.extend_from_slice(&[frame[11] | 0x80, 0]);
    overlong.extend_from_slice(&frame[12..]);
    assert_eq!(
        native::decode_request::<View<'_>>(7, &overlong, LIMITS),
        Err(Error::Malformed)
    );
    let reply = native::encode_response(7, &value, LIMITS).unwrap();
    assert_eq!(
        native::decode_response::<View<'_>>(7, &reply, LIMITS).unwrap(),
        value
    );
    assert_eq!(
        native::decode::<&str>(&[1, 0xff], LIMITS),
        Err(Error::Malformed)
    );
}

#[test]
fn reusable_output_keeps_capacity_and_clears_all_failure_paths() {
    let mut output = Vec::with_capacity(4096);
    let original_pointer = output.as_ptr();
    for value in [0, u64::MAX, 128, 1] {
        native::encode_request_into(7, &value, LIMITS, &mut output).unwrap();
        assert_eq!(output, native::encode_request(7, &value, LIMITS).unwrap());
        assert_eq!(output.as_ptr(), original_pointer);
        assert_eq!(output.capacity(), 4096);
    }
    let retained = output.clone();
    native::encode_response_into(7, &42_u64, LIMITS, &mut output).unwrap();
    assert_eq!(output, native::encode_response(7, &42_u64, LIMITS).unwrap());
    assert_eq!(retained, native::encode_request(7, &1_u64, LIMITS).unwrap());
    for limits in [
        Limits { bytes: 0, ..LIMITS },
        Limits {
            allocation: 0,
            ..LIMITS
        },
        Limits { nodes: 0, ..LIMITS },
    ] {
        assert_eq!(
            native::encode_request_into(7, &42_u64, limits, &mut output),
            Err(Error::Limit)
        );
        assert!(output.is_empty());
        native::encode_into(&42_u64, LIMITS, &mut output).unwrap();
        assert_eq!(output, native::encode(&42_u64, LIMITS).unwrap());
    }
    struct FailSecond(core::cell::Cell<bool>);
    impl Serialize for FailSecond {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if self.0.replace(true) {
                return Err(serde::ser::Error::custom("second pass fails"));
            }
            serializer.serialize_u8(0)
        }
    }
    assert_eq!(
        native::encode_into(
            &FailSecond(core::cell::Cell::new(false)),
            LIMITS,
            &mut output
        ),
        Err(Error::Malformed)
    );
    assert!(output.is_empty());
    native::encode_request_into(7, &42_u64, LIMITS, &mut output).unwrap();
    assert_eq!(native::decode_request::<u64>(7, &output, LIMITS), Ok(42));
}

#[test]
fn hidden_list_lengths_preserve_bounds_and_zero_sized_values() {
    // Native admission deliberately hides size hints from collection visitors.
    fn check<const N: usize>() {
        for count in 0..=N {
            let values: Vec<u32> = (0..count as u32).collect();
            let bytes = native::encode(&values, LIMITS).unwrap();
            let decoded = native::decode::<ic_wire::List<u32, N>>(&bytes, LIMITS).unwrap();
            assert_eq!(decoded.as_slice(), values);
            assert_eq!(native::encode(&decoded, LIMITS).unwrap(), bytes);
        }
        let excess = native::encode(&vec![0_u32; N + 1], LIMITS).unwrap();
        assert_eq!(
            native::decode::<ic_wire::List<u32, N>>(&excess, LIMITS),
            Err(Error::Malformed)
        );
    }
    check::<0>();
    check::<1>();
    check::<3>();
    check::<33>();
    let bytes = native::encode(&vec![(); 33], LIMITS).unwrap();
    assert_eq!(
        native::decode::<ic_wire::List<(), 33>>(&bytes, LIMITS)
            .unwrap()
            .as_slice()
            .len(),
        33
    );
    assert_eq!(
        native::decode::<ic_wire::List<(), 33>>(&bytes, Limits { nodes: 8, ..LIMITS }),
        Err(Error::Limit)
    );
}

#[test]
fn packed_i16_bytes_bounds_and_canonicality() {
    use ic_wire::PackedI16;
    for value in i16::MIN..=i16::MAX {
        let packed = PackedI16::<1>::new(vec![value]).unwrap();
        let word = value.to_le_bytes();
        let bytes = [2, word[0], word[1]];
        assert_eq!(native::encode(&packed, LIMITS).unwrap(), bytes);
        assert_eq!(
            native::decode::<PackedI16<1>>(&bytes, LIMITS).unwrap(),
            packed
        );
        assert_eq!(packed.as_slice(), &[value]);
    }
    let values = vec![i16::MIN, -1, 0, i16::MAX];
    let ptr = values.as_ptr();
    let packed = PackedI16::<4>::new(values).unwrap();
    assert_eq!(packed.as_slice().as_ptr(), ptr);
    let bytes = native::encode(&packed, LIMITS).unwrap();
    // A deliberately unaligned input must not require an i16-aligned byte view.
    let mut offset = vec![0];
    offset.extend_from_slice(&bytes);
    assert_eq!(
        native::decode::<PackedI16<4>>(&offset[1..], LIMITS).unwrap(),
        packed
    );
    let values = packed.into_vec();
    assert_eq!(values.as_ptr(), ptr);
    assert!(PackedI16::<0>::new(vec![0]).is_err());
    assert_eq!(
        native::decode::<PackedI16<0>>(&[0], LIMITS)
            .unwrap()
            .as_slice(),
        &[] as &[i16]
    );
    for bytes in [
        &[1, 0][..],
        &[4, 0, 0, 0, 0],
        &[2, 0],
        &[0x82, 0, 0, 0],
        &[0, 0],
    ] {
        assert!(native::decode::<PackedI16<1>>(bytes, LIMITS).is_err());
    }
    assert_eq!(
        native::encode(&vec![i16::MAX], LIMITS).unwrap(),
        [1, 254, 255, 3]
    );
    // A shared decoder must honor each field's own bound, not reuse a prior one.
    let pair = (
        PackedI16::<1>::new(vec![i16::MIN]).unwrap(),
        PackedI16::<2>::new(vec![0, i16::MAX]).unwrap(),
    );
    let bytes = native::encode(&pair, LIMITS).unwrap();
    assert_eq!(
        native::decode::<(PackedI16<1>, PackedI16<2>)>(&bytes, LIMITS).unwrap(),
        pair
    );
    assert_eq!(
        native::decode::<(PackedI16<2>, PackedI16<1>)>(&bytes, LIMITS),
        Err(Error::Malformed)
    );
    let packed = PackedI16::<1>::new(vec![0]).unwrap();
    for short in [
        Limits { bytes: 2, ..LIMITS },
        Limits { nodes: 0, ..LIMITS },
        Limits {
            allocation: 1,
            ..LIMITS
        },
    ] {
        assert_eq!(native::encode(&packed, short), Err(Error::Limit));
        assert_eq!(
            native::decode::<PackedI16<1>>(&[2, 0, 0], short),
            Err(Error::Limit)
        );
    }
}

#[test]
fn u16_three_group_decoder_matches_canonical_postcard_control() {
    // Exhaust continuations for extreme low groups against an independent
    // decode/re-encode oracle. All valid u16/i16 values are covered above too.
    for first in [128_u8, 255] {
        for second in 0_u8..=255 {
            for third in 0_u8..=255 {
                let bytes = [first, second, third];
                let expected = postcard::take_from_bytes::<u16>(&bytes)
                    .map_err(|_| Error::Malformed)
                    .and_then(|(value, remaining)| {
                        let consumed = bytes.len() - remaining.len();
                        if postcard::to_allocvec(&value).unwrap() != bytes[..consumed] {
                            Err(Error::Malformed)
                        } else if !remaining.is_empty() {
                            Err(Error::Trailing)
                        } else {
                            Ok(value)
                        }
                    });
                assert_eq!(native::decode::<u16>(&bytes, LIMITS), expected, "{bytes:?}");
            }
        }
    }
}
