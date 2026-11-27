//! Independent application-owned frame vectors for the current codec.
use ic_wire_consumer::{
    Error, REQUEST_LIMITS, Receipt, decode_receipt, decode_upload, encode_receipt, encode_upload,
    sample_upload,
};
use wire::LimitKind;
const REQUEST: &[u8] = &[
    73, 87, 7, 0, 0, 0, 0, 0, 0, 0, 0, 128, 1, 6, 115, 101, 110, 115, 111, 114, 2, 0, 255, 6, 0,
    128, 0, 0, 255, 127, 2, 1, 128, 1, 0,
];
const RESPONSE: &[u8] = &[73, 87, 7, 0, 0, 0, 0, 0, 0, 0, 1, 129, 1, 1, 0];
#[test]
fn frozen_contract_and_refusals() {
    let request = sample_upload().unwrap();
    let response = Receipt {
        revision: 129,
        accepted: true,
    };
    assert_eq!(encode_upload(&request).unwrap(), REQUEST);
    assert_eq!(decode_upload(REQUEST).unwrap(), request);
    assert_eq!(encode_receipt(&response).unwrap(), RESPONSE);
    assert_eq!(decode_receipt(RESPONSE).unwrap(), response);
    for end in 0..REQUEST.len() {
        assert!(decode_upload(&REQUEST[..end]).is_err());
    }
    for end in 0..RESPONSE.len() {
        assert!(decode_receipt(&RESPONSE[..end]).is_err());
    }
    assert_eq!(decode_upload(RESPONSE), Err(Error::Identity));
    let mut wrong = REQUEST.to_vec();
    wrong[2] = 8;
    assert_eq!(decode_upload(&wrong), Err(Error::Identity));
    let mut trailing = REQUEST.to_vec();
    trailing.push(0);
    assert_eq!(decode_upload(&trailing), Err(Error::Trailing));
    assert_eq!(
        decode_upload(&vec![0; REQUEST_LIMITS.bytes + 1]),
        Err(Error::Limit(LimitKind::Bytes))
    );
    let mut invalid = RESPONSE.to_vec();
    invalid[RESPONSE.len() - 2] = 2;
    assert_eq!(decode_receipt(&invalid), Err(Error::Malformed));
}

#[cfg(feature = "ic")]
#[test]
fn counter_reply_preserves_operation_order_and_frames() {
    let mut reads = 0_u64;
    let reply = ic_wire_consumer::instruction_reply(|| {
        reads += 1;
        reads * reads
    });
    let expected: Vec<_> = (0..7_u64)
        .flat_map(|i| (4 * i + 3).to_le_bytes())
        .chain(REQUEST.iter().chain(RESPONSE).copied())
        .collect();
    assert_eq!(reads, 14);
    assert_eq!(reply, expected);
}
