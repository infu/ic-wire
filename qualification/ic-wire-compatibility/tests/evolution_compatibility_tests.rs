//! Generated compatibility checks across two nested schema revisions.

use super::encodings;
use proptest::{
    arbitrary::any, collection::vec, option::of, prop_assert_eq, prop_oneof, proptest,
    strategy::Strategy,
};
use wire::{Bytes, Limits, Wire, decode, encode, encode_to_slice};

const LIMITS: Limits = Limits {
    bytes: 2048,
    allocation: 128 * 1024,
    nodes: 4096,
    depth: 24,
};

///
/// Batch
/// Carries optional rows and a stable cursor after the collection.
///

#[derive(Debug, PartialEq, Wire)]
#[wire(crate = "wire")]
struct Batch<T> {
    rows: Vec<Option<T>>,
    cursor: u32,
}

impl<T> Batch<T> {
    fn map_rows<U>(&self, map: impl Fn(&T) -> U) -> Batch<U> {
        Batch {
            rows: self.rows.iter().map(|row| row.as_ref().map(&map)).collect(),
            cursor: self.cursor,
        }
    }
}

///
/// PreviousRow
/// Carries the original record and tuple layouts.
///

#[derive(Debug, PartialEq, Wire)]
#[wire(crate = "wire")]
struct PreviousRow {
    count: PreviousCount,
    active: bool,
}

///
/// PreviousCount
/// Carries the original positional value.
///

#[derive(Debug, PartialEq, Wire)]
#[wire(crate = "wire")]
struct PreviousCount(u16);

///
/// CurrentRow
/// Extends the record independently of its nested tuple.
///

#[derive(Debug, PartialEq, Wire)]
#[wire(crate = "wire")]
struct CurrentRow {
    count: CurrentCount,
    active: bool,
    #[wire(id = 129)]
    detail: Option<Bytes<130>>,
}

///
/// CurrentCount
/// Extends the tuple with a slot whose present value has an empty body.
///

#[derive(Debug, PartialEq, Wire)]
#[wire(crate = "wire")]
struct CurrentCount(u16, #[wire(id = 2)] Option<()>);

fn row() -> impl Strategy<Value = CurrentRow> {
    let detail = prop_oneof![vec(any::<u8>(), 0..=16), vec(any::<u8>(), 124..=130)];
    (any::<u16>(), of(any::<()>()), any::<bool>(), of(detail)).prop_map(
        |(count, stamp, active, detail)| CurrentRow {
            count: CurrentCount(count, stamp),
            active,
            detail: detail.map(|bytes| Bytes::new(bytes).unwrap()),
        },
    )
}

proptest! {
    #[test]
    fn nested_record_and_tuple_additions_project_in_both_directions(
        rows in vec(of(row()), 0..8),
        cursor in any::<u32>(),
    ) {
        let current = Batch { rows, cursor };
        let previous = current.map_rows(|row| PreviousRow {
            count: PreviousCount(row.count.0),
            active: row.active,
        });
        let without_additions = previous.map_rows(|row| CurrentRow {
            count: CurrentCount(row.count.0, None),
            active: row.active,
            detail: None,
        });

        for encoding in encodings() {
            let old_bytes = encode(&previous, encoding, LIMITS).unwrap();
            let new_bytes = encode(&current, encoding, LIMITS).unwrap();
            let absent_bytes = encode(&without_additions, encoding, LIMITS).unwrap();
            prop_assert_eq!(&old_bytes, &absent_bytes);

            let upgraded = decode::<Batch<CurrentRow>>(&old_bytes, encoding, LIMITS).unwrap();
            prop_assert_eq!(&upgraded, &without_additions);
            let downgraded = decode::<Batch<PreviousRow>>(&new_bytes, encoding, LIMITS).unwrap();
            prop_assert_eq!(&downgraded, &previous);
            let round_trip = decode::<Batch<CurrentRow>>(&new_bytes, encoding, LIMITS).unwrap();
            prop_assert_eq!(&round_trip, &current);

            let mut output = vec![0; new_bytes.len()];
            let from_slice = encode_to_slice(&current, encoding, LIMITS, &mut output).unwrap();
            prop_assert_eq!(from_slice, new_bytes.as_slice());
        }
    }
}
