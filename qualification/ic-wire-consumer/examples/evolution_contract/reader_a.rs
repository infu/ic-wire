//! Separately compiled reader with the original sparse fixture's declarations.
use serde::Deserialize;
use std::io::{self, Read};
use wire::{
    Direction, Encoding, Error, Limits, List, Packed, TextRef,
    evolution::{self, Extensions, OpaqueOpen, Projected, ReadContext},
};

const LIMITS: Limits = Limits {
    bytes: 65_536,
    allocation: 16_777_216,
    nodes: 200_000,
    depth: 32,
};

#[derive(Deserialize)]
struct Batch<'a> {
    #[serde(borrow)]
    groups: List<Group<'a>, 4>,
    #[serde(borrow)]
    suffix: TextRef<'a, 32>,
}

#[derive(Deserialize)]
struct Group<'a> {
    channel: u16,
    #[serde(borrow)]
    rows: List<Row<'a>, 512>,
    sequence: u64,
}

#[derive(Deserialize)]
struct Row<'a> {
    id: u32,
    #[serde(borrow)]
    state: OpaqueOpen<'a>,
    words: Packed<i16, 8>,
    #[serde(borrow)]
    fields: Extensions<'a>,
}

#[derive(Deserialize)]
struct Detail<'a> {
    #[serde(borrow)]
    message: TextRef<'a, 32>,
    #[serde(borrow)]
    fields: Extensions<'a>,
}

#[derive(Debug, PartialEq, Eq)]
enum State<'a> {
    Idle,
    Detail(&'a str),
}

fn project<'a>(row: &Row<'a>, ctx: &ReadContext) -> Result<Option<State<'a>>, Error> {
    let state = row
        .state
        .visit(ctx, |tag, field| match tag {
            1 => {
                field.decode::<()>()?;
                Ok(Some(State::Idle))
            }
            2 => field.project(|detail: Detail<'a>, child| {
                detail.fields.visit(child, |_, _| Ok(()))?;
                Ok(Some(State::Detail(detail.message.as_str())))
            }),
            _ => Ok(None),
        })?
        .flatten();
    row.fields.visit(ctx, |_, _| Ok(()))?;
    Ok(state)
}

fn check_witness(bytes: &[u8]) -> Result<(), Error> {
    let encoding = Encoding::Frame {
        method: 7,
        direction: Direction::Request,
    };
    evolution::decode_projected(
        bytes,
        encoding,
        LIMITS,
        |root: Projected<'_, Batch<'_>>, ctx| {
            root.extensions.visit(ctx, |_, _| Ok(()))?;
            let batch = root.core;
            assert_eq!(batch.suffix.as_str(), "tail ✅");
            let groups = batch.groups.as_slice();
            assert_eq!(groups.len(), 2);
            ctx.child().nested(|group_list| {
                for (group, (channel, sequence, first_id, states)) in groups.iter().zip([
                    (
                        17,
                        1_u64 << 60,
                        120,
                        [None, Some(State::Detail("steady Δ"))],
                    ),
                    (33, 257, 122, [Some(State::Idle), None]),
                ]) {
                    assert_eq!((group.channel, group.sequence), (channel, sequence));
                    assert_eq!(group.rows.as_slice().len(), 2);
                    group_list.nested(|group_ctx| {
                        group_ctx.nested(|row_list| {
                            for (index, (row, expected)) in
                                group.rows.as_slice().iter().zip(states).enumerate()
                            {
                                assert_eq!(row.id, first_id + index as u32);
                                assert_eq!(row.words.as_slice(), &[i16::MIN, -1, i16::MAX]);
                                let actual = row_list.nested(|row_ctx| project(row, row_ctx))?;
                                assert_eq!(actual, expected);
                                if let Some(State::Detail(message)) = actual {
                                    assert!(bytes.as_ptr_range().contains(&message.as_ptr()));
                                }
                            }
                            Ok(())
                        })
                    })?;
                }
                Ok(())
            })
        },
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut frame = Vec::new();
    io::stdin()
        .lock()
        .take(LIMITS.bytes as u64 + 1)
        .read_to_end(&mut frame)?;
    check_witness(&frame)?;
    println!("A projection verified");
    Ok(())
}
