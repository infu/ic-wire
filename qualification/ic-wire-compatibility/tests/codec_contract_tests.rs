//! Frame borrowing, truncation and packed floating-point contract checks.
mod support;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Borrowed<'a> {
    text: &'a str,
    #[serde(borrow, with = "serde_bytes")]
    bytes: &'a [u8],
}

fn borrowed() {
    for size in [1, 127, 128, 4096] {
        let text = "é🌍".repeat(size);
        let data = vec![0xa5; size];
        let value = Borrowed {
            text: &text,
            bytes: &data,
        };
        let encoded = encode(&value);
        let decoded: Borrowed<'_> = decode(&encoded).unwrap();
        assert_eq!(decoded, value);
        let start = encoded.as_ptr() as usize;
        let end = start + encoded.len();
        for (ptr, len) in [
            (decoded.text.as_ptr(), decoded.text.len()),
            (decoded.bytes.as_ptr(), decoded.bytes.len()),
        ] {
            let ptr = ptr as usize;
            assert!(
                ptr >= start && ptr + len <= end,
                "decoded data must borrow input"
            );
        }
    }
}
#[test]
fn text_and_bytes_borrow_the_original_frame() {
    borrowed();
}

fn prefixes() {
    for size in [0, 1, 127, 128] {
        let value = (
            "héllo",
            vec![
                Some(Ok::<u64, String>(128)),
                None,
                Some(Err("failure".into())),
            ],
            vec![7_u32; size],
        );
        let bytes = encode(&value);
        type Owned = (String, Vec<Option<Result<u64, String>>>, Vec<u32>);
        for end in 0..bytes.len() {
            assert!(
                decode::<Owned>(&bytes[..end]).is_err(),
                "accepted prefix {end}"
            );
        }
        for extra in [1, 2, 127] {
            let mut trailing = bytes.clone();
            trailing.extend(vec![0; extra]);
            assert!(decode::<Owned>(&trailing).is_err());
        }
    }
}
#[test]
fn every_truncated_prefix_and_trailing_span_refuses() {
    prefixes();
}

macro_rules! adapter {
    ($name:ident, $ty:ty, $path:literal) => {
        #[derive(Deserialize)]
        struct $name(#[serde(with = $path)] $ty);
    };
}
adapter!(Floats, Vec<f32>, "wire::serde_packed::finite_f32");
adapter!(Doubles, Vec<f64>, "wire::serde_packed::finite_f64");

fn invalid_float<T: DeserializeOwned>(width: usize, bad_words: &[Vec<u8>]) {
    for count in [1, 2, 7, 8, 9, 127, 128, 129] {
        for position in [0, count / 2, count - 1] {
            for bad in bad_words {
                let mut bytes = vec![0; count * width];
                bytes[position * width..(position + 1) * width].copy_from_slice(bad);
                assert!(decode::<T>(&encode(&serde_bytes::ByteBuf::from(bytes))).is_err());
            }
        }
        for remainder in 1..width {
            let bytes = vec![0; count * width + remainder];
            assert!(decode::<T>(&encode(&serde_bytes::ByteBuf::from(bytes))).is_err());
        }
    }
}
#[test]
fn nonfinite_words_at_first_middle_last_and_partial_words_refuse() {
    let f32s: Vec<_> = [
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0x7f80_0001),
    ]
    .map(|n| n.to_le_bytes().to_vec())
    .into();
    let f64s: Vec<_> = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0x7ff0_0000_0000_0001),
    ]
    .map(|n| n.to_le_bytes().to_vec())
    .into();
    invalid_float::<Floats>(4, &f32s);
    invalid_float::<Doubles>(8, &f64s);
    // Inspect successful values by bits: ordinary float equality misses signed zero.
    let single = [0.0_f32, -0.0, f32::from_bits(1), f32::MAX, -f32::MAX];
    let double = [0.0_f64, -0.0, f64::from_bits(1), f64::MAX, -f64::MAX];
    let single_bytes: Vec<_> = single.iter().flat_map(|x| x.to_le_bytes()).collect();
    let double_bytes: Vec<_> = double.iter().flat_map(|x| x.to_le_bytes()).collect();
    let encoded = encode(&serde_bytes::ByteBuf::from(single_bytes));
    assert_eq!(
        decode::<Floats>(&encoded)
            .unwrap()
            .0
            .iter()
            .map(|n| n.to_bits())
            .collect::<Vec<_>>(),
        single.map(f32::to_bits)
    );
    let encoded = encode(&serde_bytes::ByteBuf::from(double_bytes));
    assert_eq!(
        decode::<Doubles>(&encoded)
            .unwrap()
            .0
            .iter()
            .map(|n| n.to_bits())
            .collect::<Vec<_>>(),
        double.map(f64::to_bits)
    );
}

const LIMITS: wire::Limits = wire::Limits {
    bytes: 64 * 1024 * 1024,
    allocation: 512 * 1024 * 1024,
    nodes: 32 * 1024 * 1024,
    depth: 64,
};
const ENCODING: wire::Encoding = wire::Encoding::Frame {
    method: 7,
    direction: wire::Direction::Request,
};
fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    support::encode(value, ENCODING, LIMITS).unwrap()
}
fn decode<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, wire::Error> {
    support::decode(bytes, ENCODING, LIMITS)
}
