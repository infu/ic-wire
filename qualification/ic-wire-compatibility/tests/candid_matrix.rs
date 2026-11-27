//! Matched values encoded as Wire payloads and single-argument Candid messages.
mod support;

use candid::CandidType;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt::Debug;
use wire::LimitKind;
use wire::{Encoding, Limits};

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
enum Outcome {
    Pending,
    Accepted(u64),
    Rejected(String),
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
struct Update {
    revision: u64,
    label: String,
    samples: Vec<i16>,
    outcome: Option<Outcome>,
}

fn compare<T>(case: &str, value: &T)
where
    T: CandidType + Debug + DeserializeOwned + PartialEq + Serialize,
{
    let (wire, candid) = lengths(value, Encoding::Payload);
    println!("{case}\t{wire}\t{candid}");
}

#[test]
fn matched_payloads() {
    println!("case\twire_payload_bytes\tcandid_message_bytes");
    for n in [0, 127, 128, 16_383, 16_384, u64::MAX] {
        compare(&format!("u64/{n}"), &n);
    }
    for n in [i64::MIN, -65, -64, -1, 0, 63, 64, i64::MAX] {
        compare(&format!("i64/{n}"), &n);
    }
    compare("bool", &true);
    compare("float64", &1.25_f64);
    compare("text/empty", &String::new());
    compare("text/utf8", &String::from("温度🌡"));
    for len in [0, 1, 127, 128, 4096] {
        let bytes: Vec<u8> = (0..=255).cycle().take(len).collect();
        compare(&format!("blob/{len}"), &serde_bytes::ByteBuf::from(bytes));
        compare(&format!("u32-list/{len}"), &vec![128_u32; len]);
        compare(&format!("u32-max-list/{len}"), &vec![u32::MAX; len]);
    }
    for (name, outcome) in [
        ("absent", None),
        ("pending", Some(Outcome::Pending)),
        ("accepted", Some(Outcome::Accepted(128))),
        ("rejected", Some(Outcome::Rejected(String::from("invalid")))),
    ] {
        compare(
            name,
            &Update {
                revision: 129,
                label: String::from("sensor"),
                samples: vec![i16::MIN, 0, i16::MAX],
                outcome,
            },
        );
    }
}

/// Return the delivered lengths after checking the values on the receiving side.
fn lengths<T>(value: &T, encoding: Encoding) -> (usize, usize)
where
    T: CandidType + Debug + DeserializeOwned + PartialEq + Serialize,
{
    let ordinary = wire_bytes(value, encoding);
    let message = candid::encode_one(value).unwrap();
    assert_eq!(&candid::decode_one::<T>(&message).unwrap(), value);
    (ordinary, message.len())
}

fn wire_bytes<T: Serialize + DeserializeOwned + PartialEq + Debug>(
    value: &T,
    encoding: Encoding,
) -> usize {
    checked_wire_len(value, encoding, MATRIX_LIMITS)
}

const MATRIX_LIMITS: Limits = Limits {
    bytes: 65_536,
    allocation: 4_194_304,
    nodes: 65_536,
    depth: 32,
};

fn checked_wire_len<T: Serialize + DeserializeOwned + PartialEq + Debug>(
    value: &T,
    encoding: Encoding,
    limits: Limits,
) -> usize {
    let payload = support::encode(value, encoding, limits).unwrap();
    assert_eq!(
        &support::decode::<T>(&payload, encoding, limits).unwrap(),
        value
    );
    payload.len()
}

fn numeric_rows<T>(name: &str, small: T, wide: T)
where
    T: wire::PackedElement + CandidType + DeserializeOwned + Serialize,
{
    for len in [1, 127, 128, 4096] {
        for (distribution, stride) in [
            ("small", 0),
            ("sparse-wide", 16),
            ("half-wide", 2),
            ("wide", 1),
        ] {
            let words: Vec<T> = (0..len)
                .map(|i| {
                    if stride != 0 && i % stride == 0 {
                        wide
                    } else {
                        small
                    }
                })
                .collect();
            let (ordinary, candid) = lengths(&words, Encoding::Payload);
            let packed = wire_bytes(
                &wire::Packed::<T, 4096>::new(words).unwrap(),
                Encoding::Payload,
            );
            println!("numeric/{name}/{distribution}/{len}\t{ordinary}\t{candid}\t{packed}");
        }
    }
}

#[test]
fn numeric_distributions() {
    println!("case\twire_payload_bytes\tcandid_message_bytes\tpacked_wire_bytes");
    numeric_rows("i16", -1_i16, i16::MIN);
    numeric_rows("u16", 1_u16, u16::MAX);
    numeric_rows("i32", -1_i32, i32::MIN);
    numeric_rows("u32", 1_u32, u32::MAX);
    numeric_rows("i64", -1_i64, i64::MIN);
    numeric_rows("u64", 1_u64, u64::MAX);
}

fn batch_row<T: CandidType + Debug + DeserializeOwned + PartialEq + Serialize>(
    name: &str,
    items: impl Iterator<Item = T>,
) {
    let values: Vec<T> = items.collect();
    let frame = Encoding::Frame {
        method: 7,
        direction: wire::Direction::Request,
    };
    let mut individual = (0, 0);
    for value in &values {
        let (wire, candid) = lengths(value, frame);
        individual.0 += wire;
        individual.1 += candid;
    }
    let (wire, candid) = lengths(&values, frame);
    println!(
        "batch/{name}/{}\t{}\t{}\t{}\t{}",
        values.len(),
        individual.0,
        wire,
        individual.1,
        candid
    );
}

#[test]
fn message_granularity() {
    println!(
        "case\tindividual_wire_frames\tbatched_wire_frame\tindividual_candid_messages\tbatched_candid_message"
    );
    for len in [1, 16, 128] {
        batch_row("counter", 0_u32..len);
        batch_row(
            "event",
            (0..len).map(|i| Update {
                revision: u64::from(i),
                label: String::from("sensor"),
                samples: vec![-1, 1, 2],
                outcome: Some(Outcome::Accepted(u64::from(i))),
            }),
        );
    }
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
enum Tree {
    Leaf(i32),
    Branch(Vec<Tree>),
}

#[test]
fn structured_payloads() {
    use std::collections::BTreeMap;
    compare("tree/empty", &Tree::Branch(vec![]));
    compare(
        "tree/deep",
        &(0..6).fold(Tree::Leaf(-1), |child, _| Tree::Branch(vec![child])),
    );
    compare(
        "tree/wide",
        &Tree::Branch((0..64).map(Tree::Leaf).collect()),
    );
    for len in [0, 1, 127, 128] {
        let map: BTreeMap<_, _> = (0..len)
            .map(|i: u32| (format!("key-{i}"), i.checked_sub(1)))
            .collect();
        compare(&format!("map/text-option/{len}"), &map);
        for (name, stride) in [("small", 0), ("sparse-wide", 16), ("wide", 1)] {
            let mut index = 0_usize;
            let ragged: Vec<Vec<u16>> = (0..len)
                .map(|row| {
                    (0..row % 3)
                        .map(|_| {
                            let wide = stride != 0 && index.is_multiple_of(stride);
                            index += 1;
                            if wide { u16::MAX } else { 1 }
                        })
                        .collect()
                })
                .collect();
            compare(&format!("nested/ragged/{name}/{len}"), &ragged);
        }
    }
    compare(
        "result/tuple",
        &Result::<_, String>::Ok((true, 128_u32, "温度".to_owned())),
    );
    compare(
        "result/error",
        &Result::<(bool, u32, String), _>::Err("拒否".to_owned()),
    );
}

#[test]
fn float_bits() {
    // Compare bits so signed zeros and NaN payloads remain distinguishable.
    for bits in [
        0,
        0x8000_0000,
        1,
        0x7f7f_ffff,
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_1234,
    ] {
        let value = f32::from_bits(bits);
        let limits = Limits {
            bytes: 64,
            allocation: 4096,
            nodes: 32,
            depth: 8,
        };
        let wire = support::encode(&value, Encoding::Payload, limits).unwrap();
        let message = candid::encode_one(value).unwrap();
        assert_eq!(
            support::decode::<f32>(&wire, Encoding::Payload, limits)
                .unwrap()
                .to_bits(),
            bits
        );
        assert_eq!(candid::decode_one::<f32>(&message).unwrap().to_bits(), bits);
        println!("f32/{bits:08x}\t{}\t{}", wire.len(), message.len());
    }
}

#[test]
fn platform_identities() {
    use ic_platform_types::{Account, Principal};

    #[derive(CandidType, Deserialize)]
    struct CandidAccount {
        owner: candid::Principal,
        subaccount: Option<[u8; 32]>,
    }

    let limits = Limits {
        bytes: 128,
        allocation: 8192,
        nodes: 128,
        depth: 16,
    };
    println!("case\twire_payload_bytes\tcandid_message_bytes");
    for (name, owner) in [
        ("management", Principal::management_canister()),
        ("anonymous", Principal::anonymous()),
        ("max-length", Principal::from_slice(&[7; 29])),
    ] {
        let candid_owner = candid::Principal::from_slice(owner.as_slice());
        let message = candid::encode_one(candid_owner).unwrap();
        assert_eq!(
            candid::decode_one::<candid::Principal>(&message)
                .unwrap()
                .as_slice(),
            owner.as_slice()
        );
        let size = wire_bytes(&owner, Encoding::Payload);
        println!("principal/{name}\t{size}\t{}", message.len());
        for (kind, subaccount) in [
            ("absent", None),
            ("zero", Some([0; 32])),
            ("nonzero", Some([17; 32])),
        ] {
            let account = Account { owner, subaccount };
            let bytes = support::encode(&account, Encoding::Payload, limits).unwrap();
            let actual: Account = support::decode(&bytes, Encoding::Payload, limits).unwrap();
            assert_eq!((actual.owner, actual.subaccount), (owner, subaccount));
            let message = candid::encode_one(CandidAccount {
                owner: candid_owner,
                subaccount,
            })
            .unwrap();
            let decoded: CandidAccount = candid::decode_one(&message).unwrap();
            assert_eq!(
                (decoded.owner.as_slice(), decoded.subaccount),
                (owner.as_slice(), subaccount)
            );
            println!("account/{name}/{kind}\t{}\t{}", bytes.len(), message.len());
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct BulkFlags(#[serde(with = "wire::serde_packed::bools")] Vec<bool>);

#[test]
fn fixed_width_fields() {
    println!("case\twire_payload_bytes\tcandid_message_bytes\tbulk_wire_bytes");
    for len in [0, 1, 3, 4, 7, 8, 127, 128, 511, 512, 1024] {
        let flags: Vec<_> = (0..len).map(|i| i % 3 == 0).collect();
        let (ordinary, candid) = lengths(&flags, Encoding::Payload);
        let bulk = wire_bytes(&BulkFlags(flags), Encoding::Payload);
        assert_eq!(ordinary, bulk, "one byte per flag in both representations");
        println!("flags/{len}\t{ordinary}\t{candid}\t{bulk}");
        hash_list::<16>(len);
        hash_list::<32>(len);
    }
}

fn hash_list<const WIDTH: usize>(len: usize)
where
    [u8; WIDTH]: CandidType + Debug + Serialize + DeserializeOwned + wire::PackedElement,
{
    let hashes: Vec<[u8; WIDTH]> = (0..len)
        .map(|i| std::array::from_fn(|j| (i + j).to_le_bytes()[0]))
        .collect();
    let limits = Limits {
        nodes: 524_288,
        ..MATRIX_LIMITS
    };
    let ordinary = checked_wire_len(&hashes, Encoding::Payload, limits);
    let message = candid::encode_one(&hashes).unwrap();
    assert_eq!(
        candid::decode_one::<Vec<[u8; WIDTH]>>(&message).unwrap(),
        hashes
    );
    let candid = message.len();
    if WIDTH == 32 && len == 511 {
        assert_eq!(
            support::encode(&hashes, Encoding::Payload, MATRIX_LIMITS),
            Err(wire::Error::Limit(LimitKind::Nodes))
        );
    }
    let packed = wire::Packed::<_, 1024>::new(hashes).unwrap();
    let bulk = checked_wire_len(&packed, Encoding::Payload, limits);
    if len * WIDTH < 128 {
        assert_eq!(bulk, ordinary);
    } else if len * WIDTH == 128 {
        assert_eq!(bulk, ordinary + 1);
    }
    if WIDTH == 32 && len == 511 {
        assert_eq!(wire_bytes(&packed, Encoding::Payload), bulk);
    }
    println!("hash{WIDTH}/{len}\t{ordinary}\t{candid}\t{bulk}");
}

#[derive(Debug, Deserialize, Serialize)]
struct Finite32(#[serde(with = "wire::serde_packed::finite_f32")] Vec<f32>);
#[derive(Debug, Deserialize, Serialize)]
struct Finite64(#[serde(with = "wire::serde_packed::finite_f64")] Vec<f64>);

#[test]
fn finite_float_layouts() {
    println!("case\twire_payload_bytes\tcandid_message_bytes\tbulk_wire_bytes");
    macro_rules! rows {
        ($float:ty, $bulk:ident) => {{
            let width = size_of::<$float>();
            let pattern = [
                -0.0,
                0.0,
                <$float>::from_bits(1),
                <$float>::MIN,
                <$float>::MAX,
                1.25,
            ];
            for len in [
                0,
                1,
                128 / width - 1,
                128 / width,
                127,
                128,
                1024,
                16_384 / width - 1,
                16_384 / width,
            ] {
                let values: Vec<_> = pattern.into_iter().cycle().take(len).collect();
                let plain = support::encode(&values, Encoding::Payload, MATRIX_LIMITS).unwrap();
                let message = candid::encode_one(&values).unwrap();
                let bulk = $bulk(values);
                let packed = support::encode(&bulk, Encoding::Payload, MATRIX_LIMITS).unwrap();
                // Ordinary prefixes count elements. Bulk prefixes count payload bytes.
                let prefix_bytes = |n| 1 + usize::from(n >= 128) + usize::from(n >= 16_384);
                assert_eq!(plain.len(), len * width + prefix_bytes(len) + 1);
                assert_eq!(packed.len(), len * width + prefix_bytes(len * width) + 1);
                for decoded in [
                    support::decode::<Vec<$float>>(&plain, Encoding::Payload, MATRIX_LIMITS)
                        .unwrap(),
                    candid::decode_one::<Vec<$float>>(&message).unwrap(),
                    support::decode::<$bulk>(&packed, Encoding::Payload, MATRIX_LIMITS)
                        .unwrap()
                        .0,
                ] {
                    assert!(
                        decoded
                            .iter()
                            .map(|v| v.to_bits())
                            .eq(bulk.0.iter().map(|v| v.to_bits()))
                    );
                }
                println!(
                    "finite/{}/{len}\t{}\t{}\t{}",
                    stringify!($float),
                    plain.len(),
                    message.len(),
                    packed.len()
                );
            }
            for invalid in [<$float>::INFINITY, <$float>::NEG_INFINITY, <$float>::NAN] {
                let value = $bulk(vec![1.0, invalid]);
                assert!(support::encode(&value, Encoding::Payload, MATRIX_LIMITS).is_err());
                let mut payload = vec![u8::try_from(2 * width).unwrap()];
                payload.extend_from_slice(&(1.0 as $float).to_le_bytes());
                payload.extend_from_slice(&invalid.to_le_bytes());
                payload.push(0);
                assert!(matches!(
                    support::decode::<$bulk>(&payload, Encoding::Payload, MATRIX_LIMITS),
                    Err(wire::Error::Malformed)
                ));
            }
        }};
    }
    rows!(f32, Finite32);
    rows!(f64, Finite64);
}

#[test]
fn wide_integer_lists() {
    for len in [1, 127, 128] {
        for (name, value) in [
            ("small", 1),
            ("above-u64", 1_u128 << 64),
            ("max", u128::MAX),
        ] {
            compare(&format!("u128/{name}/{len}"), &vec![value; len]);
        }
        for (name, value) in [
            ("small", -1),
            ("below-i64", i128::from(i64::MIN) - 1),
            ("min", i128::MIN),
            ("max", i128::MAX),
        ] {
            compare(&format!("i128/{name}/{len}"), &vec![value; len]);
        }
    }
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
struct IndexedReadings {
    slots: u32,
    present: Vec<(u32, u16)>,
}

#[test]
fn sparse_slots() {
    println!("case\toption_wire\tindexed_wire\toption_candid\tindexed_candid");
    for slots in [16_u32, 128, 1024] {
        for (pattern, stride) in [("empty", 0), ("every-16", 16), ("every-2", 2), ("full", 1)] {
            // A present zero is a reading. Empty and trailing slots retain their positions.
            let dense: Vec<_> = (0..slots)
                .map(|i| (stride != 0 && i % stride == 0).then_some(u16::try_from(i).unwrap()))
                .collect();
            let indexed = IndexedReadings {
                slots,
                present: (0..slots)
                    .zip(&dense)
                    .filter_map(|(i, v)| v.map(|n| (i, n)))
                    .collect(),
            };
            let mut restored = vec![None; usize::try_from(indexed.slots).unwrap()];
            for &(i, value) in &indexed.present {
                assert!(
                    restored[usize::try_from(i).unwrap()]
                        .replace(value)
                        .is_none()
                );
            }
            assert_eq!(dense, restored);
            let (dw, dc) = lengths(&dense, Encoding::Payload);
            let (iw, ic) = lengths(&indexed, Encoding::Payload);
            println!("slots/{pattern}/{slots}\t{dw}\t{iw}\t{dc}\t{ic}");
        }
    }
}

#[test]
fn utf8_length_prefixes() {
    println!("case\tcharacters\tutf8_bytes\twire_payload_bytes\tcandid_message_bytes");
    for (bytes, prefix) in [(127, 1), (128, 2), (16_383, 2), (16_384, 3)] {
        for (name, glyph) in [
            ("ascii", "a"),
            ("two-byte", "é"),
            ("three-byte", "温"),
            ("four-byte", "🌡"),
        ] {
            let value = glyph.repeat(bytes / glyph.len()) + &"x".repeat(bytes % glyph.len());
            assert_eq!(value.len(), bytes);
            let (wire, candid) = lengths(&value, Encoding::Payload);
            assert_eq!(wire, bytes + prefix + 1);
            println!(
                "text/{name}/{bytes}\t{}\t{bytes}\t{wire}\t{candid}",
                value.chars().count()
            );
        }
    }
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
struct LabelTable {
    labels: Vec<String>,
    indices: Vec<u16>,
}

#[test]
fn repeated_labels() {
    use std::collections::BTreeMap;
    println!("case\tinline_wire\ttable_wire\tinline_candid\ttable_candid");
    for (count, unique) in [
        (1, 1),
        (16, 1),
        (16, 4),
        (16, 16),
        (256, 1),
        (256, 4),
        (256, 128),
        (256, 129),
        (256, 256),
    ] {
        for width in [2, 12, 64] {
            // Permute the label order while keeping each value exactly 'width' bytes.
            let inline: Vec<String> = (0..count)
                .map(|i| format!("{:0width$x}", (i * 17) % unique))
                .collect();
            let mut table = LabelTable {
                labels: Vec::new(),
                indices: Vec::new(),
            };
            let mut known = BTreeMap::new();
            for label in &inline {
                let index = known.entry(label).or_insert_with(|| {
                    let index = u16::try_from(table.labels.len()).unwrap();
                    table.labels.push(label.clone());
                    index
                });
                table.indices.push(*index);
            }
            assert_eq!(table.labels.len(), unique);
            assert!(
                table
                    .indices
                    .iter()
                    .map(|&i| &table.labels[usize::from(i)])
                    .eq(&inline)
            );
            let (iw, ic) = lengths(&inline, Encoding::Payload);
            let (tw, tc) = lengths(&table, Encoding::Payload);
            println!("labels/{count}/{unique}/{width}\t{iw}\t{tw}\t{ic}\t{tc}");
        }
    }
}

#[test]
fn optional_and_zero_byte_elements() {
    println!("case\twire_payload_bytes\tcandid_message_bytes");
    for count in [0, 1, 127, 128] {
        let units = vec![(); count];
        let optional_units: Vec<_> = (0..count).map(|i| (i % 2 == 0).then_some(())).collect();
        let nested: Vec<_> = [None, Some(None), Some(Some(0_u16)), Some(Some(u16::MAX))]
            .into_iter()
            .cycle()
            .take(count)
            .collect();
        let (wire, candid) = lengths(&units, Encoding::Payload);
        assert_eq!(wire, if count < 128 { 2 } else { 3 });
        println!("unit-list/{count}\t{wire}\t{candid}");
        compare(&format!("optional-unit-list/{count}"), &optional_units);
        compare(&format!("nested-option-list/{count}"), &nested);
    }
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
struct GappedSequence {
    first: u64,
    gaps: Vec<u64>,
}

#[test]
fn ordered_sequences() {
    println!("case\tabsolute_wire\tgaps_wire\tabsolute_candid\tgaps_candid");
    for count in [1, 16, 128, 129] {
        for (pattern, step) in [
            ("duplicate", 0),
            ("adjacent", 1),
            ("one-byte-gap", 127),
            ("two-byte-gap", 128),
            ("wide-gap", 1_u64 << 48),
        ] {
            let absolute: Vec<_> = (0..count).map(|i| 1_700_000_000 + i * step).collect();
            let deltas = GappedSequence {
                first: absolute[0],
                gaps: absolute.windows(2).map(|pair| pair[1] - pair[0]).collect(),
            };
            let mut restored = vec![deltas.first];
            for gap in &deltas.gaps {
                restored.push(restored.last().unwrap().checked_add(*gap).unwrap());
            }
            assert_eq!(restored, absolute);
            let (aw, ac) = lengths(&absolute, Encoding::Payload);
            let (gw, gc) = lengths(&deltas, Encoding::Payload);
            println!("sequence/{pattern}/{count}\t{aw}\t{gw}\t{ac}\t{gc}");
        }
    }
}

#[derive(CandidType, Debug, Deserialize, PartialEq, Serialize)]
struct FlagBitmap {
    count: u16,
    bytes: serde_bytes::ByteBuf,
}

#[test]
fn flag_bitmaps() {
    println!("case\tflags_wire\tbitmap_wire\tflags_candid\tbitmap_candid");
    for count in [0_u16, 1, 2, 3, 7, 8, 9, 127, 128, 1016, 1017] {
        let flags: Vec<_> = (0..count).map(|i| i % 3 == 0).collect();
        let bytes: Vec<_> = flags
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0, |byte, (bit, flag)| byte | (u8::from(*flag) << bit))
            })
            .collect();
        let bitmap = FlagBitmap {
            count,
            bytes: bytes.into(),
        };
        assert!(
            (0..usize::from(bitmap.count))
                .map(|i| bitmap.bytes[i / 8] & (1 << (i % 8)) != 0)
                .eq(flags.iter().copied())
        );
        let (fw, fc) = lengths(&flags, Encoding::Payload);
        let (bw, bc) = lengths(&bitmap, Encoding::Payload);
        match count {
            0 | 1 => assert!(bw > fw),
            2 => assert_eq!(bw, fw),
            _ => assert!(bw < fw),
        }
        println!("flag-bitmap/{count}\t{fw}\t{bw}\t{fc}\t{bc}");
    }
}

#[derive(CandidType, Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
enum State {
    Ready,
    Busy,
    Done,
}

#[test]
fn state_runs() {
    println!("case\tstates_wire\truns_wire\tstates_candid\truns_candid");
    for (count, run_lengths) in [
        (0_u16, &[1][..]),
        (1, &[1]),
        (127, &[8, 127]),
        (128, &[8, 127, 128]),
        (256, &[1, 8, 127, 128, 256]),
    ] {
        for &run_length in run_lengths {
            let states: Vec<_> = (0..count)
                .map(|i| [State::Ready, State::Busy, State::Done][usize::from(i / run_length) % 3])
                .collect();
            let runs: Vec<_> = states
                .chunk_by(|left, right| left == right)
                .map(|run| (run[0], u16::try_from(run.len()).unwrap()))
                .collect();
            assert!(
                runs.iter()
                    .flat_map(|&(state, n)| std::iter::repeat_n(state, usize::from(n)))
                    .eq(states.iter().copied())
            );
            let (sw, sc) = lengths(&states, Encoding::Payload);
            let (rw, rc) = lengths(&runs, Encoding::Payload);
            println!("state-runs/{count}/{run_length}\t{sw}\t{rw}\t{sc}\t{rc}");
        }
    }
}

#[test]
fn one_byte_varint_edges() {
    fn check<T>(name: &str, values: Vec<T>, expected: usize)
    where
        T: wire::PackedElement + CandidType + DeserializeOwned + Serialize,
    {
        let (ordinary, candid) = lengths(&values, Encoding::Payload);
        let packed = wire_bytes(
            &wire::Packed::<T, 4>::new(values).unwrap(),
            Encoding::Payload,
        );
        assert_eq!(ordinary, expected);
        println!("varint-edges/{name}\t{ordinary}\t{candid}\t{packed}");
    }
    println!("case\twire_payload_bytes\tcandid_message_bytes\tpacked_wire_bytes");
    check("u16", vec![0_u16, 127, 128], 6);
    check("u32", vec![0_u32, 127, 128], 6);
    check("u64", vec![0_u64, 127, 128], 6);
    check("i16", vec![-65_i16, -64, 63, 64], 8);
    check("i32", vec![-65_i32, -64, 63, 64], 8);
    check("i64", vec![-65_i64, -64, 63, 64], 8);
}

/// Both coordinate layouts represent exactly three axes per point.
#[test]
fn three_axis_points() {
    println!("case\tarray_wire\tvec_wire\tarray_candid\tvec_candid");
    for count in [0, 1, 127, 128, 1024] {
        let points: Vec<[i16; 3]> = [[0, -1, 1], [i16::MIN, 63, i16::MAX], [-65, -64, 64]]
            .into_iter()
            .cycle()
            .take(count)
            .collect();
        let variable: Vec<Vec<i16>> = points.iter().map(|point| point.to_vec()).collect();
        let (aw, ac) = lengths(&points, Encoding::Payload);
        let (vw, vc) = lengths(&variable, Encoding::Payload);
        assert_eq!(
            vw,
            aw + count,
            "one inner count byte per variable-width row"
        );
        assert_eq!(ac, vc, "Candid represents both inner types as vec int16");
        println!("points/xyz/{count}\t{aw}\t{vw}\t{ac}\t{vc}");
    }
}

#[test]
fn grouped_relations() {
    use std::collections::BTreeMap;
    println!("case\tpairs_wire\tgroups_wire\tpairs_candid\tgroups_candid");
    // Edges are ordered by source. Preserve repeated targets and their order.
    for (sources, fanouts) in [
        (0_u32, &[0][..]),
        (1, &[1, 127, 128]),
        (16, &[1, 2, 8]),
        (127, &[1, 8]),
        (128, &[1, 8]),
    ] {
        for &fanout in fanouts {
            let first_keys: &[u32] = if sources == 0 { &[0] } else { &[0, 1_000_000] };
            for &first_key in first_keys {
                let pairs: Vec<_> = (first_key..first_key + sources)
                    .flat_map(|source| {
                        (0..fanout).map(move |i| (source, [255_u16, 0, 255, 1][i % 4]))
                    })
                    .collect();
                let mut groups = BTreeMap::<u32, Vec<u16>>::new();
                for &(source, target) in &pairs {
                    groups.entry(source).or_default().push(target);
                }
                assert!(
                    groups
                        .iter()
                        .flat_map(|(&source, targets)| {
                            targets.iter().map(move |&target| (source, target))
                        })
                        .eq(pairs.iter().copied())
                );
                let (pw, pc) = lengths(&pairs, Encoding::Payload);
                let (gw, gc) = lengths(&groups, Encoding::Payload);
                println!("relations/{first_key}/{sources}/{fanout}\t{pw}\t{gw}\t{pc}\t{gc}");
            }
        }
    }
}
