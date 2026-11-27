//! Installed counters for the complete sparse-extension fixture.
#[allow(
    dead_code,
    reason = "The Wasm counter uses the public fixture operations"
)]
mod fixture;

use ic_platform_runtime::raw;
use std::hint::black_box;

const CASES: [(u8, usize, bool); 8] = [
    (0, 0, false),
    (0, 1, false),
    (0, 16, false),
    (1, 128, false),
    (1, 128, true),
    (1, 1_024, false),
    (1, 1_024, true),
    (2, 128, false),
];

#[cfg(not(feature = "evolution-caller-buffer"))]
fn measure_encode<const REV: u8>(count: usize, dense: bool) -> (Vec<u8>, u64) {
    let encode = || {
        let mut bytes = Vec::new();
        fixture::encode_batch::<REV>(count, dense, fixture::LIMITS, &mut bytes)
            .unwrap_or_else(|_| raw::trap(b"encode fixture"));
        bytes
    };
    let bytes = encode();
    let start = raw::instruction_counter();
    for _ in 0..8 {
        drop(black_box(encode()));
    }
    (bytes, raw::instruction_counter() - start)
}

#[cfg(feature = "evolution-caller-buffer")]
fn measure_encode<const REV: u8>(count: usize, dense: bool) -> (Vec<u8>, u64) {
    let mut buffer = vec![0; fixture::LIMITS.bytes];
    let first = fixture::encode_batch_to_slice::<REV>(count, dense, fixture::LIMITS, &mut buffer)
        .unwrap_or_else(|_| raw::trap(b"encode fixture"))
        .len();
    let start = raw::instruction_counter();
    for _ in 0..8 {
        let frame =
            fixture::encode_batch_to_slice::<REV>(count, dense, fixture::LIMITS, &mut buffer)
                .unwrap_or_else(|_| raw::trap(b"encode fixture"));
        black_box(frame);
    }
    let cost = raw::instruction_counter() - start;
    buffer.truncate(first);
    (buffer, cost)
}

fn probe<const REV: u8>(count: usize, dense: bool, cross: bool) -> [u64; 4] {
    let cross = cross && !cfg!(feature = "evolution-b-only");
    let (bytes, encode_cost) = measure_encode::<REV>(black_box(count), black_box(dense));
    let decode = |bytes| {
        fixture::decode::<REV>(bytes, fixture::LIMITS)
            .unwrap_or_else(|_| raw::trap(b"decode fixture"))
    };
    let cross_decode = |bytes| {
        fixture::decode::<0>(bytes, fixture::LIMITS).unwrap_or_else(|_| raw::trap(b"old reader"))
    };
    if decode(&bytes).suffix.as_str() != "tail ✅"
        || (cross && cross_decode(&bytes).suffix.as_str() != "tail ✅")
    {
        raw::trap(b"fixture suffix");
    }
    let start = raw::instruction_counter();
    for _ in 0..8 {
        drop(black_box(decode(black_box(&bytes))));
    }
    let end = raw::instruction_counter();
    let cross_cost = if cross {
        let start = raw::instruction_counter();
        for _ in 0..8 {
            drop(black_box(cross_decode(black_box(&bytes))));
        }
        raw::instruction_counter() - start
    } else {
        0
    };
    [bytes.len() as u64, encode_cost, end - start, cross_cost]
}

#[expect(
    unsafe_code,
    reason = "Fixed IC query export for installed qualification"
)]
#[unsafe(export_name = "canister_query measure")]
extern "C" fn measure() {
    if raw::msg_arg_data_size() != 0 {
        raw::trap(b"expected empty request");
    }
    for (revision, count, dense) in CASES {
        #[cfg(feature = "evolution-b-only")]
        let counts = if revision == 1 {
            probe::<1>(count, dense, false)
        } else {
            continue;
        };
        #[cfg(not(feature = "evolution-b-only"))]
        let counts = match revision {
            0 => probe::<0>(count, dense, false),
            1 => probe::<1>(count, dense, true),
            _ => probe::<2>(count, dense, true),
        };
        for count in counts {
            raw::msg_reply_data_append(&count.to_le_bytes());
        }
    }
    raw::msg_reply();
}
