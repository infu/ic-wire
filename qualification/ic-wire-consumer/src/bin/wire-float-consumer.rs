//! Standalone finite-f32 workload, separate from the frozen upload oracle.
use serde::{Deserialize, Serialize};
use std::hint::black_box;

#[derive(Serialize, Deserialize, wire::Wire)]
#[wire(crate = "wire", fixed)]
struct PackedValues(#[serde(with = "wire::serde_packed::finite_f32")] Vec<f32>);

#[derive(wire::Wire)]
#[wire(crate = "wire")]
struct Values(PackedValues);

const LIMITS: wire::Limits = wire::Limits {
    bytes: 32768,
    allocation: 1048576,
    nodes: 131072,
    depth: 16,
};

fn main() {
    for size in [0, 1, 7, 128, 4096] {
        let values = Values(PackedValues(
            (0..black_box(size))
                .map(|i| {
                    f32::from_bits([0, 0x80000000, 1, 0x007fffff, 0x3f800000, 0xff7fffff][i % 6])
                })
                .collect(),
        ));
        let mut bytes = wire::encode(&values, wire::Encoding::Payload, LIMITS).unwrap();
        let decode = |bytes: &[u8]| wire::decode::<Values>(bytes, wire::Encoding::Payload, LIMITS);
        let decoded = decode(black_box(&bytes)).unwrap();
        let Values(PackedValues(decoded)) = decoded;
        let Values(PackedValues(values)) = values;
        assert_eq!(decoded.len(), size);
        assert!(
            decoded
                .iter()
                .zip(&values)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        if size != 0 {
            let end = bytes.len() - 1;
            bytes[end - 4..end].copy_from_slice(&f32::INFINITY.to_le_bytes());
            assert!(decode(black_box(&bytes)).is_err());
        }
    }
}
