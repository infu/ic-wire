//! Isolated allocator measurements, without the concurrent Rust test harness.
// All fixture preparation and assertions occur outside measured regions.
mod support;

use serde::{Serialize, Serializer};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};
use std::{alloc::System, cell::Cell, hint::black_box};
use support::{decode, encode};
use wire::LimitKind;
use wire::{
    Bytes, BytesRef, Direction, Encoding, Error, Limits, Packed, Text,
    evolution::{Projected, encode_projected_into, encode_projected_to_slice, projected_len},
};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 8192,
        nodes: 256,
        depth: 32,
    }
}

fn measure<T>(operation: impl FnOnce() -> T) -> (T, Stats) {
    let region = Region::new(GLOBAL);
    let result = black_box(operation());
    let stats = region.change();
    (result, stats)
}

fn no_allocation(stats: Stats) {
    assert_eq!(stats.allocations, 0, "{stats:?}");
    assert_eq!(stats.reallocations, 0, "{stats:?}");
    assert_eq!(stats.bytes_allocated, 0, "{stats:?}");
}

fn one_allocation(stats: Stats, bytes: usize) {
    assert_eq!(stats.allocations, 1, "{stats:?}");
    assert_eq!(stats.reallocations, 0, "{stats:?}");
    assert_eq!(stats.bytes_allocated, bytes, "{stats:?}");
}

fn preflight_refusal_and_sizing_allocate_nothing() {
    let value = black_box(128_u16);
    for encoding in [
        Encoding::Payload,
        Encoding::Frame {
            method: 7,
            direction: Direction::Request,
        },
    ] {
        for (short, reason) in [
            Limits {
                bytes: 0,
                ..limits()
            },
            Limits {
                allocation: 0,
                ..limits()
            },
            Limits {
                nodes: 0,
                ..limits()
            },
            Limits {
                depth: 0,
                ..limits()
            },
        ]
        .into_iter()
        .zip([
            LimitKind::Bytes,
            LimitKind::Allocation,
            LimitKind::Nodes,
            LimitKind::Depth,
        ]) {
            let (result, stats) = measure(|| encode(&value, encoding, short));
            assert_eq!(result, Err(Error::Limit(reason)));
            no_allocation(stats);
        }
        let message = Projected::empty(value);
        let (result, stats) = measure(|| projected_len(&message, encoding, limits()));
        assert_eq!(
            result.unwrap(),
            if encoding == Encoding::Payload { 3 } else { 14 }
        );
        no_allocation(stats);
    }
}

struct PassProbe {
    pass: Cell<usize>,
    allocations_seen: Cell<[usize; 2]>,
}

impl Serialize for PassProbe {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let pass = self.pass.get();
        let mut seen = self.allocations_seen.get();
        if pass < seen.len() {
            seen[pass] = GLOBAL.stats().allocations;
        }
        self.allocations_seen.set(seen);
        self.pass.set(pass + 1);
        serializer.serialize_u16(128)
    }
}

fn owned_output_is_reserved_between_passes() {
    for encoding in [
        Encoding::Payload,
        Encoding::Frame {
            method: 7,
            direction: Direction::Response,
        },
    ] {
        let value = PassProbe {
            pass: Cell::new(0),
            allocations_seen: Cell::new([0; 2]),
        };
        let before = GLOBAL.stats().allocations;
        let (result, stats) = measure(|| encode(&value, encoding, limits()));
        let output = result.unwrap();
        let length = if encoding == Encoding::Payload { 3 } else { 14 };
        assert_eq!(output.len(), length);
        one_allocation(stats, length);
        assert_eq!(value.pass.get(), 2);
        assert_eq!(value.allocations_seen.get(), [before, before + 1]);
    }
}

fn caller_owned_destinations_allocate_nothing() {
    let value = black_box(128_u16);
    let encoding = Encoding::Frame {
        method: 7,
        direction: Direction::Response,
    };
    let mut reused = Vec::with_capacity(64);
    reused.extend_from_slice(&[42; 16]);
    let (result, stats) = measure(|| {
        encode_projected_into(&mut reused, encoding, limits(), |_| {
            Ok(Projected::empty(value))
        })
    });
    assert_eq!(result.unwrap().len(), 14);
    no_allocation(stats);
    let mut slice = [0xa5; 64];
    let (result, stats) = measure(|| {
        encode_projected_to_slice(&mut slice, encoding, limits(), |_| {
            Ok(Projected::empty(value))
        })
    });
    assert_eq!(result.unwrap().len(), 14);
    no_allocation(stats);
    let mut short = [0xa5; 13];
    let (result, stats) = measure(|| {
        encode_projected_to_slice(&mut short, encoding, limits(), |_| {
            Ok(Projected::empty(value))
        })
    });
    assert_eq!(result, Err(Error::Limit(LimitKind::Output)));
    assert_eq!(&short[..11], &[0xa5; 11]);
    no_allocation(stats);
    let (result, stats) = measure(|| {
        encode_projected_into(
            &mut reused,
            encoding,
            Limits {
                nodes: 0,
                ..limits()
            },
            |_| Ok(Projected::empty(value)),
        )
    });
    assert_eq!(result, Err(Error::Limit(LimitKind::Nodes)));
    assert!(reused.is_empty());
    no_allocation(stats);
}

fn automatic_scratch_is_admitted_before_allocation() {
    #[derive(wire::Wire)]
    #[wire(crate = "wire")]
    struct Reply<'a> {
        code: u8,
        #[wire(id = 1)]
        details: Option<BytesRef<'a, 256>>,
    }

    let mut output = [0; 384];
    let encoding = Encoding::Payload;
    let policy = Limits {
        bytes: output.len(),
        ..limits()
    };
    for (details, expected) in [
        (None, [4, 0].as_slice()),
        (
            Some(BytesRef::new(&[0x7f]).unwrap()),
            &[4, 4, 1, 2, 1, 0x7f],
        ),
    ] {
        let value = black_box(Reply { code: 4, details });
        let (result, stats) =
            measure(|| wire::encode_to_slice(&value, encoding, policy, &mut output));
        assert_eq!(result.unwrap(), expected);
        no_allocation(stats);
    }

    let large = [0x7f; 256];
    let value = black_box(Reply {
        code: 4,
        details: Some(BytesRef::new(&large).unwrap()),
    });
    let tight = Limits {
        allocation: policy.bytes + 128,
        ..policy
    };
    let (result, stats) = measure(|| wire::encode_to_slice(&value, encoding, tight, &mut output));
    assert_eq!(result, Err(Error::Limit(LimitKind::Allocation)));
    no_allocation(stats);

    let (result, stats) = measure(|| wire::encode_to_slice(&value, encoding, policy, &mut output));
    let encoded = result.unwrap();
    assert_eq!(&encoded[..8], &[4, 0x85, 2, 1, 0x82, 2, 0x80, 2]);
    assert_eq!(&encoded[8..], large);
    assert!(stats.bytes_allocated > 0, "{stats:?}");
}

fn decode_field_refusal_precedes_owned_allocation() {
    // The generic element bound must reject before allocating for wider words too.
    let mut wide = [0; 18];
    wide[0] = 16;
    let (result, stats) = measure(|| decode::<Packed<u64, 1>>(&wide, Encoding::Payload, limits()));
    assert_eq!(result, Err(Error::Malformed));
    no_allocation(stats);
    let input = black_box([3, 1, 2, 3, 0]);
    let (result, stats) = measure(|| decode::<Bytes<2>>(&input, Encoding::Payload, limits()));
    assert_eq!(result, Err(Error::Malformed));
    no_allocation(stats);
    for input in [&[1, 0, 0][..], &[4, 0, 0, 0, 0, 0][..], &[4, 0, 0][..]] {
        let (result, stats) =
            measure(|| decode::<Packed<i16, 1>>(black_box(input), Encoding::Payload, limits()));
        assert_eq!(result, Err(Error::Malformed));
        no_allocation(stats);
    }
    let (result, stats) = measure(|| {
        decode::<Bytes<3>>(
            &input,
            Encoding::Payload,
            Limits {
                allocation: 0,
                ..limits()
            },
        )
    });
    assert_eq!(result, Err(Error::Limit(LimitKind::Allocation)));
    no_allocation(stats);
    // A huge element count must not reserve before the first seed is refused.
    let (result, stats) = measure(|| {
        decode::<Vec<u8>>(
            &[0xff, 0x7f, 0],
            Encoding::Payload,
            Limits {
                nodes: 1,
                ..limits()
            },
        )
    });
    assert_eq!(result, Err(Error::Limit(LimitKind::Nodes)));
    no_allocation(stats);
}

fn successful_decode_controls_and_borrowing() {
    let input = black_box([3, 1, 2, 3, 0]);
    let (result, stats) = measure(|| decode::<Bytes<3>>(&input, Encoding::Payload, limits()));
    assert_eq!(result.unwrap().as_slice(), &[1, 2, 3]);
    one_allocation(stats, 3);
    let (result, stats) =
        measure(|| decode::<Packed<i16, 2>>(&[4, 1, 0, 255, 255, 0], Encoding::Payload, limits()));
    assert_eq!(result.unwrap().as_slice(), &[1, -1]);
    one_allocation(stats, 4);
    let (result, stats) =
        measure(|| decode::<BytesRef<'_, 3>>(&input, Encoding::Payload, limits()));
    assert_eq!(result.unwrap().as_slice().as_ptr(), input[1..].as_ptr());
    no_allocation(stats);
    let (result, stats) = measure(|| decode::<Bytes<0>>(&[0, 0], Encoding::Payload, limits()));
    assert!(result.unwrap().as_slice().is_empty());
    no_allocation(stats);
}

fn incremental_and_trusted_work_are_not_a_global_allocation_ban() {
    // A valid prefix is allocated before the later invalid boolean is read.
    let (result, stats) =
        measure(|| decode::<(Bytes<3>, bool)>(&[3, 1, 2, 3, 2, 0], Encoding::Payload, limits()));
    assert_eq!(result, Err(Error::Malformed));
    one_allocation(stats, 3);
    assert_eq!(stats.deallocations, 1);
    // Text uses owned byte decoding before UTF-8 validation.
    let (result, stats) = measure(|| decode::<Text<1>>(&[1, 255, 0], Encoding::Payload, limits()));
    assert_eq!(result, Err(Error::Malformed));
    one_allocation(stats, 1);
    assert_eq!(stats.deallocations, 1);
    struct Custom;
    impl Serialize for Custom {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let allocation = black_box(vec![42_u8; 17]);
            black_box(&allocation);
            serializer.serialize_unit()
        }
    }
    let mut output = [0xa5; 1];
    let (result, stats) = measure(|| {
        encode_projected_to_slice(&mut output, Encoding::Payload, limits(), |_| {
            Ok(Projected::empty(Custom))
        })
    });
    assert_eq!(result, Ok([0].as_slice()));
    one_allocation(stats, 17);
}

fn failed_second_pass_releases_output_without_growth() {
    struct Changes {
        pass: Cell<usize>,
        fail: bool,
    }
    impl Serialize for Changes {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let first = self.pass.replace(self.pass.get() + 1) == 0;
            if !first && self.fail {
                return Err(serde::ser::Error::custom("second pass refusal"));
            }
            serializer.serialize_u16(if first { 1 } else { 128 })
        }
    }
    for fail in [false, true] {
        let value = Changes {
            pass: Cell::new(0),
            fail,
        };
        let (result, stats) = measure(|| encode(&value, Encoding::Payload, limits()));
        assert_eq!(result, Err(Error::Malformed));
        one_allocation(stats, 2);
        assert_eq!(stats.deallocations, 1);
        assert_eq!(stats.bytes_deallocated, 2);
        assert_eq!(value.pass.get(), 2);
    }
}

fn domain_validation_allocation_boundaries() {
    #[derive(serde::Deserialize)]
    struct Bools(#[serde(with = "wire::serde_packed::bools")] Vec<bool>);
    #[derive(serde::Deserialize)]
    struct Floats(#[serde(with = "wire::serde_packed::finite_f32")] Vec<f32>);
    #[derive(serde::Deserialize)]
    struct Doubles(#[serde(with = "wire::serde_packed::finite_f64")] Vec<f64>);
    #[derive(serde::Deserialize)]
    struct Nonzero(#[serde(with = "wire::serde_packed::nonzero_i64")] Vec<std::num::NonZeroI64>);
    let (result, stats) = measure(|| decode::<Bools>(&[2, 0, 2, 0], Encoding::Payload, limits()));
    assert!(result.is_err());
    no_allocation(stats);
    let (result, stats) = measure(|| {
        decode::<Floats>(
            &[8, 0, 0, 128, 63, 0, 0, 128, 127, 0],
            Encoding::Payload,
            limits(),
        )
    });
    assert!(result.is_err());
    no_allocation(stats);
    let (result, stats) = measure(|| {
        decode::<Doubles>(
            &[8, 0, 0, 0, 0, 0, 0, 240, 127, 0],
            Encoding::Payload,
            limits(),
        )
    });
    assert!(result.is_err());
    no_allocation(stats);
    let (result, stats) = measure(|| decode::<Bools>(&[2, 0, 1, 0], Encoding::Payload, limits()));
    assert_eq!(result.unwrap().0, [false, true]);
    one_allocation(stats, 2);
    let (result, stats) =
        measure(|| decode::<Floats>(&[4, 0, 0, 128, 63, 0], Encoding::Payload, limits()));
    assert_eq!(result.unwrap().0, [1.0]);
    one_allocation(stats, 4);
    let (result, stats) = measure(|| {
        decode::<Doubles>(
            &[8, 0, 0, 0, 0, 0, 0, 240, 63, 0],
            Encoding::Payload,
            limits(),
        )
    });
    assert_eq!(result.unwrap().0, [1.0]);
    one_allocation(stats, 8);
    let (result, stats) =
        measure(|| decode::<Nonzero>(&[8, 0, 0, 0, 0, 0, 0, 0, 0, 0], Encoding::Payload, limits()));
    assert!(result.is_err());
    one_allocation(stats, 8);
    assert_eq!(
        decode::<Nonzero>(&[8, 1, 0, 0, 0, 0, 0, 0, 0, 0], Encoding::Payload, limits())
            .unwrap()
            .0[0]
            .get(),
        1
    );
}

fn main() {
    preflight_refusal_and_sizing_allocate_nothing();
    owned_output_is_reserved_between_passes();
    caller_owned_destinations_allocate_nothing();
    automatic_scratch_is_admitted_before_allocation();
    decode_field_refusal_precedes_owned_allocation();
    successful_decode_controls_and_borrowing();
    incremental_and_trusted_work_are_not_a_global_allocation_ban();
    failed_second_pass_releases_output_without_growth();
    domain_validation_allocation_boundaries();
    println!("allocation qualification: 9 groups passed");
}
