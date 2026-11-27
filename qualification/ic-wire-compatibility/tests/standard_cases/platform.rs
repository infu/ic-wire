use crate::support::{decode, encode};
use crate::{limits, message_type, run_typescript};
use ic_platform_types::{Account, Principal};
use serde_json::json;
use wire::Encoding;
use wire::typescript::PayloadLimits;

#[test]
fn platform_identity_supplier_preserves_absent_and_explicit_zero() {
    let graph = message_type::<Account>("Account");
    let module = graph
        .payload_module(
            "codec",
            "MessageAccount",
            "./runtime-adapter.ts",
            PayloadLimits {
                bytes: 128,
                nodes: 16,
                depth: 8,
            },
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap();
    let mut explicit = vec![1, 4, 1, 32];
    explicit.extend([0; 33]);
    let literals = vec![vec![1, 4, 0, 0], explicit];
    let output: Vec<Vec<u8>> = serde_json::from_value(run_typescript(
        &json!({"module":module,"literals":literals}),
        include_str!("platform.mjs"),
    ))
    .unwrap();
    assert_eq!(output, literals);
    for (bytes, subaccount) in output.iter().zip([None, Some([0; 32])]) {
        let account = Account {
            owner: Principal::anonymous(),
            subaccount,
        };
        assert_eq!(
            encode(&account, Encoding::Payload, limits()).unwrap(),
            *bytes
        );
        let decoded = decode::<Account>(bytes, Encoding::Payload, limits()).unwrap();
        assert_eq!(decoded.owner, account.owner);
        // Account equality intentionally normalizes None and zero: inspect the field.
        assert_eq!(decoded.subaccount, subaccount);
    }
    for length in [0, 1, 29] {
        let value = Principal::try_from_slice(&vec![7; length]).unwrap();
        let mut bytes = vec![length as u8];
        bytes.extend(vec![7; length]);
        bytes.push(0);
        assert_eq!(encode(&value, Encoding::Payload, limits()).unwrap(), bytes);
        assert_eq!(
            decode::<Principal>(&bytes, Encoding::Payload, limits()).unwrap(),
            value
        );
    }
    assert!(Principal::try_from_slice(&[7; 30]).is_err());
    for bytes in [vec![30; 31], vec![2, 7]] {
        assert_eq!(
            decode::<Principal>(&bytes, Encoding::Payload, limits()),
            Err(wire::Error::Malformed)
        );
    }
    assert!(decode::<Account>(&[1, 4, 1, 31], Encoding::Payload, limits()).is_err());
}
