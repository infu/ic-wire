use super::{definition, limits, run_typescript};
use crate::support::{decode, encode};
use serde_json::json;
use wire::{Encoding, Packed};

#[test]
fn packed_element_bounds_are_shared_by_all_browser_word_kinds() {
    use std::collections::BTreeMap;
    use wire::typescript::{SchemaLimits, ValidatedRegistry};
    assert_eq!(
        definition::<Packed<u64, 1>>("Words"),
        json!({"k":"packed","word":"u64","max":1})
    );
    for (word, width) in [
        ("i8", 1),
        ("i16", 2),
        ("u16", 2),
        ("i32", 4),
        ("u32", 4),
        ("i64", 8),
        ("u64", 8),
        ("hash16", 16),
        ("hash32", 32),
        ("bool", 1),
        ("finite-f32", 4),
        ("finite-f64", 8),
    ] {
        let schema = json!({"k":"packed","word":word,"max":1});
        let graph = ValidatedRegistry::from_metadata(
            &BTreeMap::from([("Words".into(), schema.clone())]),
            SchemaLimits::default(),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&graph.metadata_json().unwrap()).unwrap()["Words"],
            schema
        );
        let unsafe_bound = json!({"k":"packed","word":word,"max":((1_u64<<53)-1)/width+1});
        assert_eq!(
            ValidatedRegistry::from_metadata(
                &BTreeMap::from([("Words".into(), unsafe_bound)]),
                SchemaLimits::default()
            )
            .unwrap_err(),
            wire::typescript::GenerationError::Complexity
        );
    }
    let output = run_typescript(&json!({}), include_str!("bounded-packed.mjs"));
    assert_eq!(output, json!(13));
}

#[test]
fn packed_words_keep_byte_prefixes_and_owned_storage() {
    let mut cases = Vec::new();
    macro_rules! case {
        ($ty:ty,$word:literal,$values:expr,$bytes:expr) => {{
            let value=Packed::<$ty>::new($values).unwrap();
            let mut bytes: Vec<u8> = $bytes;
            bytes.push(0);
            assert_eq!(encode(&value,Encoding::Payload,limits()).unwrap(),bytes);
            assert_eq!(decode::<Packed<$ty>>(&bytes,Encoding::Payload,limits()).unwrap(),value);
            let schema=definition::<Packed<$ty>>("Words");
            assert_eq!(schema,json!({"k":"packed","word":$word}));
            cases.push(json!({"schema":schema,"bytes":bytes}));
        }};
    }
    case!(i8, "i8", vec![-128, 127], vec![2, 128, 127]);
    case!(i16, "i16", vec![-1, 128], vec![4, 255, 255, 128, 0]);
    case!(u16, "u16", vec![0, 65535], vec![4, 0, 0, 255, 255]);
    case!(
        i32,
        "i32",
        vec![-1, 128],
        vec![8, 255, 255, 255, 255, 128, 0, 0, 0]
    );
    case!(u32, "u32", vec![0x12345678], vec![4, 120, 86, 52, 18]);
    case!(
        i64,
        "i64",
        vec![-1],
        vec![8, 255, 255, 255, 255, 255, 255, 255, 255]
    );
    case!(
        u64,
        "u64",
        vec![0x0102030405060708],
        vec![8, 8, 7, 6, 5, 4, 3, 2, 1]
    );
    let mut hash16 = vec![16];
    hash16.extend([7; 16]);
    case!([u8; 16], "hash16", vec![[7; 16]], hash16);
    let mut hash32 = vec![32];
    hash32.extend([9; 32]);
    case!([u8; 32], "hash32", vec![[9; 32]], hash32);
    let output = run_typescript(&json!({"cases":cases}), include_str!("packed.mjs"));
    assert_eq!(output, json!(cases.len()));
    assert_eq!(
        decode::<Packed<u32>>(&[3, 0, 0, 0], Encoding::Payload, limits()),
        Err(wire::Error::Malformed)
    );
    assert_eq!(
        decode::<Packed<[u8; 16]>>(&[1, 0], Encoding::Payload, limits()),
        Err(wire::Error::Malformed)
    );
}
