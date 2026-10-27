//! Caller-storage encoding retains bytes, admission and two-pass failure bounds.
use ic_wire::{Error, Limits, native};
use serde::{Serialize, Serializer};
use std::cell::Cell;

const LIMITS: Limits = Limits {
    bytes: 4096,
    allocation: 65536,
    nodes: 4096,
    depth: 64,
};

#[test]
fn caller_storage_matches_owned_payloads_and_both_frame_directions() {
    for value in [vec![], vec![0, 127, 128, u64::MAX], vec![65535; 128]] {
        let expected = native::encode(&value, LIMITS).unwrap();
        let mut storage = vec![0xa5; expected.len() + 16];
        let encoded = native::encode_to_slice(&value, LIMITS, &mut storage).unwrap();
        assert_eq!(encoded, expected);
        assert_eq!(native::decode::<Vec<u64>>(encoded, LIMITS).unwrap(), value);
        assert!(storage[expected.len()..].iter().all(|b| *b == 0xa5));
        for response in [false, true] {
            let expected = if response {
                native::encode_response(91, &value, LIMITS).unwrap()
            } else {
                native::encode_request(91, &value, LIMITS).unwrap()
            };
            let encoded = if response {
                native::encode_response_to_slice(91, &value, LIMITS, &mut storage)
            } else {
                native::encode_request_to_slice(91, &value, LIMITS, &mut storage)
            }
            .unwrap();
            assert_eq!(encoded, expected);
        }
    }
    assert_eq!(
        native::encode_to_slice(&(), LIMITS, &mut []).unwrap(),
        &[] as &[u8]
    );
    assert!(
        native::encode_to_slice(&(), Limits { bytes: 0, ..LIMITS }, &mut [])
            .unwrap()
            .is_empty()
    );
    let mut storage = [0; 32];
    let encoded = {
        let value = String::from("owned value");
        native::encode_to_slice(&value, LIMITS, &mut storage).unwrap()
    };
    assert_eq!(
        native::decode::<String>(encoded, LIMITS).unwrap(),
        "owned value"
    );
}

#[test]
fn short_storage_and_failed_admission_write_nothing() {
    for length in 0..12 {
        let mut storage = vec![0xa5; length];
        assert_eq!(
            native::encode_request_to_slice(7, &u64::MAX, LIMITS, &mut storage),
            Err(Error::Limit)
        );
        assert!(storage.iter().all(|b| *b == 0xa5));
    }
    for limits in [
        Limits { bytes: 0, ..LIMITS },
        Limits {
            allocation: 0,
            ..LIMITS
        },
        Limits { nodes: 0, ..LIMITS },
        Limits { depth: 0, ..LIMITS },
    ] {
        let mut storage = [0xa5; 128];
        assert_eq!(
            native::encode_to_slice(&7u64, limits, &mut storage),
            Err(Error::Limit)
        );
        assert_eq!(storage, [0xa5; 128]);
    }
    let mut storage = [0xa5; 11];
    assert_eq!(
        native::encode_response_to_slice(
            7,
            &(),
            Limits {
                bytes: 10,
                ..LIMITS
            },
            &mut storage
        ),
        Err(Error::Limit)
    );
    assert_eq!(storage, [0xa5; 11]);
}

struct Changing(Cell<bool>);
impl Serialize for Changing {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.replace(true) {
            serializer.serialize_u64(u64::MAX)
        } else {
            serializer.serialize_u64(0)
        }
    }
}

#[test]
fn a_longer_second_pass_cannot_use_unadmitted_spare_storage() {
    let mut storage = [0xa5; 128];
    assert_eq!(
        native::encode_to_slice(&Changing(Cell::new(false)), LIMITS, &mut storage),
        Err(Error::Malformed)
    );
    assert!(storage[1..].iter().all(|b| *b == 0xa5));
    for length in [0, 128] {
        let published = Cell::new(false);
        assert_eq!(
            native::with_encoded(
                Changing(Cell::new(false)),
                LIMITS,
                &mut storage[..length],
                |_| { published.set(true) }
            ),
            Err(Error::Malformed)
        );
        assert!(!published.get());
    }
}

#[test]
fn scoped_output_chooses_storage_once_and_drops_input_before_publication() {
    for length in [0, 1, 127, 128, 129, 409] {
        let value = vec![37u8; length];
        let expected = native::encode(&value, LIMITS).unwrap();
        let mut scratch = [0xa5; 128];
        let result =
            native::with_encoded(value, LIMITS, &mut scratch, |bytes| bytes.to_vec()).unwrap();
        assert_eq!(result, expected);
        if expected.len() > scratch.len() {
            assert_eq!(scratch, [0xa5; 128]);
        }
    }
    struct Dropped<'a>(&'a Cell<bool>);
    impl Serialize for Dropped<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_u64(7)
        }
    }
    impl Drop for Dropped<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    for length in [0, 128] {
        let dropped = Cell::new(false);
        let mut scratch = vec![0; length];
        native::with_encoded(Dropped(&dropped), LIMITS, &mut scratch, |bytes| {
            assert!(dropped.get());
            assert_eq!(bytes, [7]);
        })
        .unwrap();
    }
    let published = Cell::new(false);
    assert_eq!(
        native::with_encoded(7u64, Limits { nodes: 0, ..LIMITS }, &mut [], |_| published
            .set(true)),
        Err(Error::Limit)
    );
    assert!(!published.get());
}

#[test]
fn exact_byte_caps_are_independent_of_storage_capacity() {
    for value in [0u64, 127, 128, u64::MAX] {
        let expected = native::encode(&value, LIMITS).unwrap();
        let mut storage = [0; 128];
        let limits = Limits {
            bytes: expected.len(),
            ..LIMITS
        };
        assert_eq!(
            native::encode_to_slice(&value, limits, &mut storage).unwrap(),
            expected
        );
        assert_eq!(
            native::encode_to_slice(
                &value,
                Limits {
                    bytes: expected.len() - 1,
                    ..LIMITS
                },
                &mut storage
            ),
            Err(Error::Limit)
        );
    }
}
