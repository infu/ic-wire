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

fn measure<T: Serialize>(value: T, repetitions: u32) {
    // All four API families admit the same root type, &T. with_encoded takes
    // that reference by value; the other APIs take a reference to it.
    let borrowed = &value;
    let expected = native::encode(&borrowed, LIMITS).unwrap();
    let mut reused = Vec::with_capacity(expected.len());
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
                0 => {
                    let bytes = native::encode(black_box(&borrowed), LIMITS).unwrap();
                    let length = bytes.len();
                    black_box(&bytes);
                    drop(bytes);
                    Some(length)
                }
                1 => {
                    native::encode_into(black_box(&borrowed), LIMITS, &mut reused).unwrap();
                    black_box(&reused);
                    Some(reused.len())
                }
                #[cfg(not(feature = "caller-output-baseline"))]
                2 => {
                    let bytes = native::encode_to_slice(black_box(&borrowed), LIMITS, &mut storage)
                        .unwrap();
                    black_box(bytes);
                    Some(bytes.len())
                }
                #[cfg(not(feature = "caller-output-baseline"))]
                3 => Some(
                    native::with_encoded(black_box(borrowed), LIMITS, &mut scratch, |bytes| {
                        black_box(bytes);
                        bytes.len()
                    })
                    .unwrap(),
                ),
                // Historical-library controls do the same four logical
                // encodings: the previously available reusable/fresh APIs are
                // the respective alternatives to the new slice/scoped paths.
                #[cfg(feature = "caller-output-baseline")]
                2 => {
                    native::encode_into(black_box(&borrowed), LIMITS, &mut reused).unwrap();
                    black_box(&reused);
                    Some(reused.len())
                }
                #[cfg(feature = "caller-output-baseline")]
                3 => {
                    let bytes = native::encode(black_box(&borrowed), LIMITS).unwrap();
                    black_box(&bytes);
                    Some(bytes.len())
                }
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
    assert_eq!(reused, expected);
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
