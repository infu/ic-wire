//! Reuse one row-sized scratch slice while stock Serde writes each row.
use serde::{Serialize, Serializer, ser::SerializeSeq};
use wire::{
    Error, Limits, TextRef,
    evolution::{Projected, WriteContext, encode_projected_to_slice_with},
};

#[derive(Serialize)]
struct Batch<'a, G> {
    groups: &'a [G],
    suffix: TextRef<'a, 32>,
}

#[derive(Serialize)]
struct Group<R> {
    channel: u16,
    rows: R,
    sequence: u64,
}

struct Rows<'a, 'budget, const REV: u8> {
    context: &'a WriteContext<'budget>,
    range: std::ops::Range<usize>,
    dense: bool,
}

impl<const REV: u8> Serialize for Rows<'_, '_, REV> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.range.len()))?;
        self.context
            .nested(|groups| {
                groups.nested(|group| {
                    group.nested(|rows| {
                        let mut scratch = [0; 96];
                        for index in self.range.clone() {
                            let row = rows.nested(|ctx| {
                                super::make_row::<REV>(
                                    ctx,
                                    &mut scratch.as_mut_slice(),
                                    index,
                                    self.dense,
                                )
                            })?;
                            if let Err(error) = sequence.serialize_element(&row) {
                                return Ok(Err(error));
                            }
                        }
                        Ok(sequence.end())
                    })
                })
            })
            .map_err(serde::ser::Error::custom)?
    }
}

pub(super) fn encode<const REV: u8>(
    count: usize,
    dense: bool,
    limits: Limits,
    output: &mut [u8],
) -> Result<&[u8], Error> {
    assert!(count <= 1_024);
    encode_projected_to_slice_with(output, super::REQUEST, limits, |output| {
        let context = output.context().child();
        let boundary = count.div_ceil(2);
        let rows = |range| Rows::<'_, '_, REV> {
            context: &context,
            range,
            dense,
        };
        let groups = [
            Group {
                channel: 17,
                rows: rows(0..boundary),
                sequence: 1_u64 << 60,
            },
            Group {
                channel: 33,
                rows: rows(boundary..count),
                sequence: 257,
            },
        ];
        output.write(&Projected::empty(Batch {
            groups: groups.as_slice(),
            suffix: TextRef::new("tail ✅")?,
        }))
    })
}
