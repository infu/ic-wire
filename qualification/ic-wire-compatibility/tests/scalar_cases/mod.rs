use super::{limits, run_typescript};
use crate::support::{decode, encode};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wire::typescript::{SchemaLimits, ValidatedRegistry};
use wire::{Encoding, Error, Text};

#[test]
fn wide_integers_and_utf8_exchange_without_normalizing_retained_input() {
    let types = BTreeMap::from([
        ("Unsigned".into(), json!({"k":"u","bits":64})),
        ("Signed".into(), json!({"k":"i","bits":128})),
        ("Text".into(), json!({"k":"text","max":8})),
    ]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let mut cases = Vec::new();
    for (value, mut bytes) in [
        (0_u64, vec![0]),
        (128, vec![128, 1]),
        (
            u64::MAX,
            vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 1],
        ),
    ] {
        bytes.push(0);
        assert_eq!(encode(&value, Encoding::Payload, limits()).unwrap(), bytes);
        cases.push(json!({"name":"Unsigned","value":value.to_string(),"bytes":bytes}));
    }
    for (value, mut bytes) in [
        (0_i128, vec![0]),
        (-1, vec![1]),
        (i128::MIN, [vec![255; 18], vec![3]].concat()),
        (i128::MAX, [vec![254], vec![255; 17], vec![3]].concat()),
    ] {
        bytes.push(0);
        assert_eq!(encode(&value, Encoding::Payload, limits()).unwrap(), bytes);
        cases.push(json!({"name":"Signed","value":value.to_string(),"bytes":bytes}));
    }
    for (value, mut bytes) in [
        ("", vec![0]),
        ("é", vec![2, 195, 169]),
        ("\u{feff}a", vec![4, 239, 187, 191, 97]),
        ("😀", vec![4, 240, 159, 152, 128]),
    ] {
        bytes.push(0);
        assert_eq!(
            encode(
                &Text::<8>::new(value.into()).unwrap(),
                Encoding::Payload,
                limits()
            )
            .unwrap(),
            bytes
        );
        cases.push(json!({"name":"Text","value":value,"bytes":bytes}));
    }
    let invalid_text = vec![
        vec![1, 255],
        vec![2, 192, 128],
        vec![3, 237, 160, 128],
        vec![4, 244, 144, 128, 128],
    ];
    for bytes in &invalid_text {
        assert_eq!(
            decode::<Text<8>>(bytes, Encoding::Payload, limits()),
            Err(Error::Malformed)
        );
    }
    let redundant = [128, 0, 0];
    assert_eq!(
        decode::<u64>(&redundant, Encoding::Payload, limits()).unwrap(),
        0
    );
    assert_eq!(
        decode::<i128>(&redundant, Encoding::Payload, limits()).unwrap(),
        0
    );
    let fixture = json!({"types":serde_json::from_str::<Value>(&graph.metadata_json().unwrap()).unwrap(), "cases":cases,"invalidText":invalid_text});
    let result = run_typescript(&fixture, include_str!("exchange.mjs"));
    let outputs: Vec<Vec<u8>> = serde_json::from_value(result).unwrap();
    assert_eq!(outputs.len(), cases.len());
    for (bytes, case) in outputs.iter().zip(&cases) {
        match case["name"].as_str().unwrap() {
            "Unsigned" => assert_eq!(
                decode::<u64>(bytes, Encoding::Payload, limits())
                    .unwrap()
                    .to_string(),
                case["value"].as_str().unwrap()
            ),
            "Signed" => assert_eq!(
                decode::<i128>(bytes, Encoding::Payload, limits())
                    .unwrap()
                    .to_string(),
                case["value"].as_str().unwrap()
            ),
            "Text" => assert_eq!(
                decode::<Text<8>>(bytes, Encoding::Payload, limits()).unwrap(),
                Text::new(case["value"].as_str().unwrap().into()).unwrap()
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn generated_boolean_payloads_agree_with_the_browser_decoder() {
    use proptest::{collection::vec, prelude::*, strategy::ValueTree, test_runner::TestRunner};

    let mut runner = TestRunner::default();
    let inputs = vec(prop_oneof![Just(0_u8), Just(1), any::<u8>()], 0..8);
    let mut cases = Vec::new();
    for _ in 0..256 {
        let bytes = inputs.new_tree(&mut runner).unwrap().current();
        let expected = match decode::<bool>(
            &bytes,
            Encoding::Payload,
            wire::Limits {
                bytes: 4,
                ..limits()
            },
        ) {
            Ok(value) => json!(value),
            Err(Error::Limit(wire::LimitKind::Bytes)) => json!("limit"),
            // The browser groups a trailing suffix with malformed representations.
            Err(Error::Trailing | Error::Malformed) => json!("malformed"),
            Err(error) => panic!("unexpected bool error {error:?} for {bytes:?}"),
        };
        cases.push(json!([bytes, expected]));
    }
    assert_eq!(
        run_typescript(&json!(cases), include_str!("generated.mjs")),
        json!(true)
    );
}
