//! Concrete nested DTOs for cross-language evolution and admission tests.
#[cfg(any(test, feature = "evolution-caller-buffer"))]
#[path = "slice_rows.rs"]
mod slice_rows;
use serde::{Deserialize, Serialize};
use wire::{
    BytesRef, Direction, Encoding, Error, Limits, List, Packed, TextRef,
    evolution::{
        self, Extensions, ExtensionsWriter, OpaqueOpen, Projected, ReadContext, WriteContext,
    },
};

pub(super) const METHOD: u64 = 7;
pub(super) const LIMITS: Limits = Limits {
    bytes: 65_536,
    allocation: 16_777_216,
    nodes: 200_000,
    depth: 32,
};
pub(super) const REQUEST: Encoding = Encoding::Frame {
    method: METHOD,
    direction: Direction::Request,
};
const CHUNK: [u8; 4] = [0, 255, 9, 1];

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Batch<'a, R> {
    pub(super) groups: List<Group<R>, 4>,
    #[serde(borrow)]
    pub(super) suffix: TextRef<'a, 32>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Group<R> {
    pub(super) channel: u16,
    pub(super) rows: List<R, 512>,
    pub(super) sequence: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Row<'a, const REV: u8> {
    id: u32,
    #[serde(borrow)]
    state: OpaqueOpen<'a>,
    words: Packed<i16, 8>,
    #[serde(borrow)]
    fields: Extensions<'a>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Detail<'a, const REV: u8> {
    #[serde(borrow)]
    message: TextRef<'a, 32>,
    #[serde(borrow)]
    fields: Extensions<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "tag", content = "value")]
pub(super) enum State<'a> {
    Idle(()),
    Detail {
        message: &'a str,
        audit: Option<&'a [u8]>,
    },
    Archived(&'a str),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct View<'a> {
    pub(super) id: u32,
    pub(super) state: Option<State<'a>>,
    pub(super) words: Vec<i16>,
    pub(super) note: Option<&'a str>,
    pub(super) marker: bool,
    pub(super) epoch: Option<u32>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct GroupView<'a> {
    pub(super) channel: u16,
    pub(super) rows: Vec<View<'a>>,
    pub(super) sequence: u64,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct BatchView<'a> {
    pub(super) groups: Vec<GroupView<'a>>,
    pub(super) suffix: &'a str,
}

/// The same row allocation holds the wire observations and then the typed row.
#[derive(Debug, PartialEq)]
pub(super) struct RowStorage<'a, const REV: u8> {
    id: u32,
    words: Packed<i16, 8>,
    projection: RowProjection<'a>,
}

#[derive(Debug, PartialEq)]
enum RowProjection<'a> {
    Wire(OpaqueOpen<'a>, Extensions<'a>),
    Ready(RowFields<'a>),
}

#[derive(Debug, PartialEq)]
pub(super) struct RowFields<'a> {
    state: Option<State<'a>>,
    note: Option<&'a str>,
    marker: bool,
    pub(super) epoch: Option<u32>,
}

impl<'de: 'a, 'a, const REV: u8> Deserialize<'de> for RowStorage<'a, REV> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let Row::<'_, REV> {
            id,
            state,
            words,
            fields,
        } = Row::deserialize(d)?;
        Ok(Self {
            id,
            words,
            projection: RowProjection::Wire(state, fields),
        })
    }
}

impl<'a, const REV: u8> RowStorage<'a, REV> {
    fn project(&mut self, ctx: &ReadContext<'_>) -> Result<bool, Error> {
        let RowProjection::Wire(state, fields) = self.projection else {
            return Err(Error::Malformed);
        };
        let view = project_row::<REV>(state, fields, ctx)?;
        let missing = REV == 2 && view.epoch.is_none();
        self.projection = RowProjection::Ready(view);
        Ok(missing)
    }

    fn into_view(self) -> View<'a> {
        let RowProjection::Ready(fields) = self.projection else {
            unreachable!("successful projection has no unprojected rows");
        };
        View {
            id: self.id,
            words: self.words.into_vec(),
            state: fields.state,
            note: fields.note,
            marker: fields.marker,
            epoch: fields.epoch,
        }
    }
}

pub(super) type Decoded<'a, const REV: u8> = Batch<'a, RowStorage<'a, REV>>;

impl<'a, const REV: u8> Decoded<'a, REV> {
    pub(super) fn row(&self, group: usize, row: usize) -> &RowFields<'a> {
        let RowProjection::Ready(fields) =
            &self.groups.as_slice()[group].rows.as_slice()[row].projection
        else {
            unreachable!("successful projection has no unprojected rows");
        };
        fields
    }

    pub(super) fn into_view(self) -> BatchView<'a> {
        BatchView {
            groups: self
                .groups
                .into_vec()
                .into_iter()
                .map(|group| GroupView {
                    channel: group.channel,
                    rows: group
                        .rows
                        .into_vec()
                        .into_iter()
                        .map(RowStorage::into_view)
                        .collect(),
                    sequence: group.sequence,
                })
                .collect(),
            suffix: self.suffix.as_str(),
        }
    }
}

const fn has_note(index: usize, dense: bool) -> bool {
    if dense {
        index % 4 < 2
    } else {
        index.is_multiple_of(16)
    }
}

const fn has_marker(index: usize, dense: bool) -> bool {
    if dense {
        index % 4 == 2
    } else {
        index % 64 == 2
    }
}

fn scratch_bound<const REV: u8>(count: usize, dense: bool) -> usize {
    let occurrences = |offset, period| count.saturating_sub(offset).div_ceil(period);
    let detail = occurrences(1, 4);
    let note = if dense {
        occurrences(0, 4) + detail
    } else {
        occurrences(0, 16)
    };
    let marker = occurrences(2, if dense { 4 } else { 64 });
    // This is scratch for the fixture's generated values. The DTO and codec
    // independently enforce their declared bounds on the encoded frames.
    match REV {
        0 => detail * 32,
        1 => (detail + note + marker) * 32,
        _ => detail * 24 + (detail + occurrences(0, 4) + count) * 32,
    }
}

fn make_row<'a, const REV: u8>(
    ctx: &WriteContext,
    scratch: &mut &'a mut [u8],
    index: usize,
    dense: bool,
) -> Result<Row<'a, REV>, Error> {
    let id = 120 + index as u32;
    let state = match index % 4 {
        0 if REV == 2 => {
            let (open, tail) = OpaqueOpen::write_in(
                ctx,
                3,
                &TextRef::<32>::new("stopped Δ")?,
                std::mem::take(scratch),
            )?;
            *scratch = tail;
            open
        }
        1 => {
            let fields = if REV == 2 {
                let mut block = ExtensionsWriter::new(std::mem::take(scratch));
                block.push(ctx, 4, &BytesRef::<16>::new(&CHUNK)?)?;
                let (fields, tail) = block.finish_and_tail();
                *scratch = tail;
                fields
            } else {
                Extensions::empty()
            };
            let value = Detail::<REV> {
                message: TextRef::new("steady Δ")?,
                fields,
            };
            let (open, tail) = OpaqueOpen::write_in(ctx, 2, &value, std::mem::take(scratch))?;
            *scratch = tail;
            open
        }
        2 => OpaqueOpen::unit(1)?,
        _ => OpaqueOpen::absent(),
    };
    let mut fields = ExtensionsWriter::new(std::mem::take(scratch));
    if REV == 2 {
        fields.push(ctx, 2, &(16_384 + id))?;
    }
    if REV > 0 && has_note(index, dense) {
        let note = if index.is_multiple_of(4) {
            ""
        } else {
            "low Δ"
        };
        fields.push(ctx, 7, &TextRef::<16>::new(note)?)?;
    }
    if REV > 0 && has_marker(index, dense) {
        fields.push(ctx, 9, &())?;
    }
    let (fields, tail) = fields.finish_and_tail();
    *scratch = tail;
    Ok(Row {
        id,
        state,
        words: Packed::new(vec![i16::MIN, -1, i16::MAX])?,
        fields,
    })
}

fn make_rows<'a, const REV: u8>(
    ctx: &WriteContext,
    scratch: &mut &'a mut [u8],
    indices: std::ops::Range<usize>,
    dense: bool,
) -> Result<List<Row<'a, REV>, 512>, Error> {
    let rows = indices
        .map(|index| ctx.nested(|row| make_row::<REV>(row, scratch, index, dense)))
        .collect::<Result<Vec<_>, _>>()?;
    List::new(rows)
}

pub(super) fn encode<const REV: u8>(
    count: usize,
    limits: Limits,
    output: &mut Vec<u8>,
) -> Result<(), Error> {
    encode_batch::<REV>(count, true, limits, output)
}

pub(super) fn encode_batch<const REV: u8>(
    count: usize,
    dense: bool,
    limits: Limits,
    output: &mut Vec<u8>,
) -> Result<(), Error> {
    let mut scratch = prepare_scratch::<REV>(count, dense)?;
    evolution::encode_projected_into(output, REQUEST, limits, |ctx| {
        prepare_batch::<REV>(&ctx.child(), &mut scratch, count, dense).map(Projected::empty)
    })?;
    Ok(())
}

#[cfg(any(test, feature = "evolution-caller-buffer"))]
pub(super) fn encode_batch_to_slice<const REV: u8>(
    count: usize,
    dense: bool,
    limits: Limits,
    output: &mut [u8],
) -> Result<&[u8], Error> {
    slice_rows::encode::<REV>(count, dense, limits, output)
}

fn prepare_scratch<const REV: u8>(count: usize, dense: bool) -> Result<Vec<u8>, Error> {
    assert!(count <= 1_024);
    let size = scratch_bound::<REV>(count, dense);
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    scratch.resize(size, 0);
    Ok(scratch)
}

fn prepare_batch<'a, const REV: u8>(
    ctx: &WriteContext<'_>,
    mut scratch: &'a mut [u8],
    count: usize,
    dense: bool,
) -> Result<Batch<'a, Row<'a, REV>>, Error> {
    let boundary = count.div_ceil(2);
    Ok(Batch {
        groups: List::new(vec![
            Group {
                channel: 17,
                rows: rows_at_depth(ctx, &mut scratch, 0..boundary, dense)?,
                sequence: 1_u64 << 60,
            },
            Group {
                channel: 33,
                rows: rows_at_depth(ctx, &mut scratch, boundary..count, dense)?,
                sequence: 257,
            },
        ])?,
        suffix: TextRef::new("tail ✅")?,
    })
}

fn rows_at_depth<'a, const REV: u8>(
    ctx: &WriteContext,
    scratch: &mut &'a mut [u8],
    indices: std::ops::Range<usize>,
    dense: bool,
) -> Result<List<Row<'a, REV>, 512>, Error> {
    ctx.nested(|groups| {
        groups.nested(|group| group.nested(|rows| make_rows(rows, scratch, indices, dense)))
    })
}

fn project_row<'a, const REV: u8>(
    state: OpaqueOpen<'a>,
    fields: Extensions<'a>,
    ctx: &ReadContext,
) -> Result<RowFields<'a>, Error> {
    let state = state
        .visit(ctx, |tag, field| match tag {
            1 => {
                field.decode::<()>()?;
                Ok(Some(State::Idle(())))
            }
            2 => field.project(|value: Detail<'a, REV>, child| {
                let mut audit = None;
                value.fields.visit(child, |id, ext| {
                    if REV == 2 && id == 4 {
                        audit = Some(ext.decode::<BytesRef<'a, 16>>()?.as_slice());
                    }
                    Ok(())
                })?;
                Ok(Some(State::Detail {
                    message: value.message.as_str(),
                    audit,
                }))
            }),
            3 if REV == 2 => Ok(Some(State::Archived(
                field.decode::<TextRef<'a, 32>>()?.as_str(),
            ))),
            _ => Ok(None),
        })?
        .flatten();
    let mut note = None;
    let mut marker = false;
    let mut epoch = None;
    fields.visit(ctx, |id, field| {
        match id {
            2 if REV == 2 => epoch = Some(field.decode::<u32>()?),
            7 if REV > 0 => note = Some(field.decode::<TextRef<'a, 16>>()?.as_str()),
            9 if REV > 0 => {
                field.decode::<()>()?;
                marker = true;
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(RowFields {
        state,
        note,
        marker,
        epoch,
    })
}

pub(super) fn decode<const REV: u8>(
    bytes: &[u8],
    limits: Limits,
) -> Result<Decoded<'_, REV>, Error> {
    evolution::decode_projected(
        bytes,
        REQUEST,
        limits,
        |root: Projected<'_, Decoded<'_, REV>>, ctx| {
            root.extensions.visit(ctx, |_, _| Ok(()))?;
            let mut batch = root.core;
            let mut missing = false;
            ctx.child().nested(|group_list| {
                for group in batch.groups.as_mut_slice() {
                    group_list.nested(|group_ctx| {
                        group_ctx.nested(|row_list| {
                            for row in group.rows.as_mut_slice() {
                                missing |= row_list.nested(|row_ctx| row.project(row_ctx))?;
                            }
                            Ok(())
                        })
                    })?;
                }
                Ok(())
            })?;
            // Report absence after checking all recognized fields.
            if missing {
                return Err(Error::Incompatible);
            }
            Ok(batch)
        },
    )
}
