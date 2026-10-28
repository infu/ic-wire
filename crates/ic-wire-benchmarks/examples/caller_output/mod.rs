//! Matched borrowed-value encoding into fresh, reused, slice and scoped output.
use ic_wire::{Limits, native};
use serde::Serialize;
use std::hint::black_box;

const LIMITS: Limits = Limits {
    bytes: 2 * 1024 * 1024,
    allocation: 64 * 1024 * 1024,
    nodes: 9 * 1024 * 1024,
    depth: 64,
};

#[derive(Serialize)]
struct Record {
    id: u64,
    label: String,
    hash: [u8; 32],
}

// Separate caller inlining budgets; library inlining inside each API wrapper
// remains ordinary optimization. Include the same fixed call/loop overhead in
// both variants, without subtraction. This qualifies isolated API kernels.
#[inline(never)]
fn fresh<T: Serialize + ?Sized>(value: &T) -> usize {
    let bytes = native::encode(value, LIMITS).unwrap();
    let length = bytes.len();
    black_box(&bytes);
    drop(bytes);
    length
}

#[inline(never)]
fn reused<T: Serialize + ?Sized>(value: &T, storage: &mut Vec<u8>) -> usize {
    native::encode_into(value, LIMITS, storage).unwrap();
    black_box(&*storage);
    storage.len()
}

#[cfg(not(feature = "caller-output-baseline"))]
#[inline(never)]
fn slice<T: Serialize + ?Sized>(value: &T, storage: &mut [u8]) -> usize {
    let bytes: &[u8] = native::encode_to_slice(value, LIMITS, storage).unwrap();
    black_box(bytes);
    bytes.len()
}

#[cfg(not(feature = "caller-output-baseline"))]
#[inline(never)]
fn scoped<T: Serialize>(value: T, scratch: &mut [u8]) -> usize {
    native::with_encoded(value, LIMITS, scratch, |bytes| {
        black_box(bytes);
        bytes.len()
    })
    .unwrap()
}

fn measure<T: Serialize>(value: T, repetitions: u32) {
    // All four API families admit the same root type, &T. with_encoded takes
    // that reference by value; the other APIs take a reference to it.
    let borrowed = &value;
    let expected = native::encode(&borrowed, LIMITS).unwrap();
    let mut reused_storage = Vec::with_capacity(expected.len());
    let mut storage = vec![0; expected.len()];
    let mut scratch = [0; 128];
    // Baseline and candidate prepare the same live storage, even when the old
    // library cannot execute the new paths. Setup is outside Counter0 windows.
    black_box(&mut storage);
    black_box(&mut scratch);
    let mut counts = [0; 4];
    for (operation, count) in counts.iter_mut().enumerate() {
        let mut run = || -> Option<usize> {
            match operation {
                0 => Some(fresh(black_box(&borrowed))),
                1 => Some(reused(black_box(&borrowed), &mut reused_storage)),
                #[cfg(not(feature = "caller-output-baseline"))]
                2 => Some(slice(black_box(&borrowed), &mut storage)),
                #[cfg(not(feature = "caller-output-baseline"))]
                3 => Some(scoped(black_box(borrowed), &mut scratch)),
                // Historical-library controls do the same four logical
                // encodings: the previously available reusable/fresh APIs are
                // the respective alternatives to the new slice/scoped paths.
                #[cfg(feature = "caller-output-baseline")]
                2 => Some(reused(black_box(&borrowed), &mut reused_storage)),
                #[cfg(feature = "caller-output-baseline")]
                3 => Some(fresh(black_box(&borrowed))),
                _ => None,
            }
        };
        if run().is_none() {
            continue;
        }
        let begin = ic_platform::ic0::performance_counter(0);
        for _ in 0..repetitions {
            black_box(run());
        }
        *count = ic_platform::ic0::performance_counter(0) - begin;
    }
    assert_eq!(reused_storage, expected);
    #[cfg(not(feature = "caller-output-baseline"))]
    {
        assert_eq!(
            native::encode_to_slice(&borrowed, LIMITS, &mut storage).unwrap(),
            expected
        );
        native::with_encoded(borrowed, LIMITS, &mut scratch, |bytes| {
            assert_eq!(bytes, expected)
        })
        .unwrap();
    }
    #[cfg(target_arch = "wasm32")]
    let pages = core::arch::wasm32::memory_size(0) as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let pages = 0;
    for count in counts.into_iter().chain([expected.len() as u64, pages]) {
        ic_platform::ic0::msg_reply_data_append(&count.to_le_bytes());
    }
    ic_platform::ic0::msg_reply_data_append(&expected);
    ic_platform::ic0::msg_reply();
}

// The fixture owns this export and uses safe platform system-call wrappers.
#[allow(unsafe_code)]
#[cfg_attr(
    target_arch = "wasm32",
    unsafe(export_name = "canister_update benchmark_output")
)]
/// Measure bounded encoder operations; preparation and transport are excluded.
pub extern "C" fn benchmark_output() {
    let mut args = [0; 12];
    assert_eq!(ic_platform::ic0::msg_arg_data_size(), args.len());
    ic_platform::ic0::msg_arg_data_copy(&mut args, 0);
    let get = |index| u32::from_le_bytes(args[index..index + 4].try_into().unwrap());
    let (kind, n, repetitions) = (get(0), get(4), get(8));
    assert!((1..=128).contains(&repetitions));
    assert!(n <= 262144);
    match kind {
        0 => measure((), repetitions),
        1 => measure(
            match n {
                0 => 0u64,
                1 => 7,
                _ => u64::MAX,
            },
            repetitions,
        ),
        2 => measure(
            (0..n)
                .map(|i| i.wrapping_mul(37).wrapping_add(11) as u8)
                .collect::<Vec<_>>(),
            repetitions,
        ),
        3 => measure(
            ic_wire::ByteBuf::from(
                (0..n)
                    .map(|i| i.wrapping_mul(37).wrapping_add(11) as u8)
                    .collect::<Vec<_>>(),
            ),
            repetitions,
        ),
        4 => {
            assert!(n <= 8192);
            measure(
                (0..n)
                    .map(|i| (i as u64).wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17))
                    .collect::<Vec<_>>(),
                repetitions,
            );
        }
        5 => {
            assert!(n <= 256);
            measure(
                (0..n)
                    .map(|i| Record {
                        id: (i as u64) << 40,
                        label: format!("record-{i:06}"),
                        hash: std::array::from_fn(|j| {
                            i.wrapping_mul(37).wrapping_add(j as u32 * 11) as u8
                        }),
                    })
                    .collect::<Vec<_>>(),
                repetitions,
            );
        }
        _ => panic!("unknown case"),
    }
}
