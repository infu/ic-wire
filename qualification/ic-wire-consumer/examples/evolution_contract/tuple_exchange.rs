//! Generated framed clients for an appendable tuple inside a list and at the method root.
use std::{collections::BTreeMap, error::Error, path::Path};

use serde::{Deserialize, Serialize};
use wire::{
    Direction, Encoding, Error as WireError, List, TextRef,
    evolution::{Evolve, ExtensionSlot, Projected, encode_projected_into},
    typescript::{Method, PayloadLimits, SchemaRegistry, Wire},
};

use crate::fixture::LIMITS;

const METHOD: u64 = 71;
const SEQUENCE: u64 = 1 << 60;

#[derive(Evolve)]
#[wire(crate = "wire")]
struct Empty {}

#[derive(Evolve)]
#[wire(crate = "wire")]
struct RowTail<'a> {
    #[wire(id = 4)]
    marker: Option<()>,
    #[wire(id = 3)]
    note: Option<TextRef<'a, 24>>,
}

#[derive(Evolve)]
#[wire(crate = "wire")]
struct PacketTail {
    #[wire(id = 3)]
    flag: Option<bool>,
}

macro_rules! revision {
    ($row:ident, $packet:ident, $row_tail:ty, $packet_tail:ty) => {
        #[derive(Serialize, Deserialize, Wire)]
        #[wire(crate = "wire")]
        struct $row<'a>(
            u16,
            #[serde(borrow)] TextRef<'a, 16>,
            #[serde(borrow)]
            #[wire(extensions)]
            ExtensionSlot<'a, $row_tail>,
        );

        #[derive(Serialize, Deserialize, Wire)]
        #[wire(crate = "wire")]
        struct $packet<'a>(
            #[serde(borrow)] List<$row<'a>, 4>,
            u64,
            #[serde(borrow)]
            #[wire(extensions)]
            ExtensionSlot<'a, $packet_tail>,
        );
    };
}

revision!(RowA, PacketA, Empty, Empty);
revision!(RowB, PacketB, RowTail<'a>, PacketTail);

const fn frame(direction: Direction) -> Encoding {
    Encoding::Frame {
        method: METHOD,
        direction,
    }
}

fn encode_a(direction: Direction) -> Result<Vec<u8>, WireError> {
    let mut output = Vec::new();
    encode_projected_into(&mut output, frame(direction), LIMITS, |ctx| {
        let rows: List<RowA<'_>, 4> = ctx.child().nested(|ctx| {
            ctx.nested(|ctx| {
                List::new(vec![
                    RowA(
                        777,
                        TextRef::new("first")?,
                        Empty {}.encode_extensions(ctx, &mut [])?,
                    ),
                    RowA(
                        9,
                        TextRef::new("second")?,
                        Empty {}.encode_extensions(ctx, &mut [])?,
                    ),
                ])
            })
        })?;
        let tail = Empty {}.encode_extensions(ctx, &mut [])?;
        Ok(Projected::new((rows, SEQUENCE), tail.into_raw()))
    })?;
    Ok(output)
}

fn encode_b(direction: Direction) -> Result<Vec<u8>, WireError> {
    let mut output = Vec::new();
    let mut first = [0; 24];
    let mut second = [0; 8];
    let mut tail = [0; 8];
    encode_projected_into(&mut output, frame(direction), LIMITS, |ctx| {
        let rows: List<RowB<'_>, 4> = ctx.child().nested(|ctx| {
            ctx.nested(|ctx| {
                let first = RowTail {
                    marker: None,
                    note: Some(TextRef::new("extra Δ")?),
                }
                .encode_extensions(ctx, &mut first)?;
                let second = RowTail {
                    marker: Some(()),
                    note: None,
                }
                .encode_extensions(ctx, &mut second)?;
                List::new(vec![
                    RowB(777, TextRef::new("first")?, first),
                    RowB(9, TextRef::new("second")?, second),
                ])
            })
        })?;
        let tail = PacketTail { flag: Some(false) }.encode_extensions(ctx, &mut tail)?;
        Ok(Projected::new((rows, SEQUENCE), tail.into_raw()))
    })?;
    Ok(output)
}

fn generate<T: Wire>(dir: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let mut registry = SchemaRegistry::new(Default::default()).map_err(|e| format!("{e:?}"))?;
    registry
        .register::<T>("TuplePacket")
        .map_err(|e| format!("{e:?}"))?;
    let registry = registry.finish().map_err(|e| format!("{e:?}"))?;
    let limits = PayloadLimits {
        bytes: 256,
        nodes: 256,
        depth: 24,
    };
    let method = Method {
        export: "packet",
        id: METHOD,
        request: "TuplePacket",
        response: "TuplePacket",
        request_limits: limits,
        response_limits: limits,
    };
    let source = registry
        .method_module(&[method], "./runtime.ts", &BTreeMap::new(), |report| {
            assert!(report.is_clean(), "{report:?}");
        })
        .map_err(|e| format!("{e:?}"))?;
    std::fs::write(dir.join(format!("tuple-{name}.ts")), source)?;
    Ok(())
}

pub(super) fn emit(dir: &Path) -> Result<(), Box<dyn Error>> {
    generate::<PacketA<'_>>(dir, "a")?;
    generate::<PacketB<'_>>(dir, "b")?;
    for (name, direction) in [("req", Direction::Request), ("res", Direction::Response)] {
        std::fs::write(
            dir.join(format!("tuple-a-{name}.bin")),
            encode_a(direction)?,
        )?;
        std::fs::write(
            dir.join(format!("tuple-b-{name}.bin")),
            encode_b(direction)?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wire::evolution::decode_projected;

    fn decode_a(bytes: &[u8], direction: Direction) -> Result<(), WireError> {
        decode_projected(
            bytes,
            frame(direction),
            LIMITS,
            |packet: Projected<'_, (List<RowA<'_>, 4>, u64)>, ctx| {
                let (list, sequence) = packet.core;
                assert_eq!(sequence, SEQUENCE);
                Empty::decode_extensions(ExtensionSlot::from_raw(packet.extensions), ctx)?;
                ctx.child().nested(|rows| {
                    let [first, second] = list.into_vec().try_into().ok().unwrap();
                    for (row, expected) in [(first, (777, "first")), (second, (9, "second"))] {
                        rows.nested(|row_ctx| {
                            assert_eq!((row.0, row.1.as_str()), expected);
                            Empty::decode_extensions(row.2, row_ctx)?;
                            Ok(())
                        })?;
                    }
                    Ok(())
                })
            },
        )
    }

    fn decode_b(bytes: &[u8], direction: Direction, extended: bool) -> Result<(), WireError> {
        decode_projected(
            bytes,
            frame(direction),
            LIMITS,
            |packet: Projected<'_, (List<RowB<'_>, 4>, u64)>, ctx| {
                let (list, sequence) = packet.core;
                assert_eq!(sequence, SEQUENCE);
                let tail =
                    PacketTail::decode_extensions(ExtensionSlot::from_raw(packet.extensions), ctx)?;
                assert_eq!(tail.flag, extended.then_some(false));
                ctx.child().nested(|rows| {
                    let [first, second] = list.into_vec().try_into().ok().unwrap();
                    for (row, expected, tail) in [
                        (first, (777, "first"), (extended.then_some("extra Δ"), None)),
                        (second, (9, "second"), (None, extended.then_some(()))),
                    ] {
                        rows.nested(|row_ctx| {
                            assert_eq!((row.0, row.1.as_str()), expected);
                            let decoded = RowTail::decode_extensions(row.2, row_ctx)?;
                            assert_eq!(
                                (decoded.note.map(|text| text.as_str()), decoded.marker),
                                tail
                            );
                            Ok(())
                        })?;
                    }
                    Ok(())
                })
            },
        )
    }

    pub(super) fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
        for (suffix, direction) in [("req", Direction::Request), ("res", Direction::Response)] {
            for (revision, extended) in [("a", false), ("b", true)] {
                let name = format!("tuple-{revision}-{suffix}");
                let rust = std::fs::read(dir.join(format!("{name}.bin")))?;
                let browser = std::fs::read(dir.join(format!("{name}-js.bin")))?;
                assert_eq!(rust, browser);
                decode_a(&browser, direction)?;
                decode_b(&browser, direction, extended)?;
                for end in 0..browser.len() {
                    assert!(decode_a(&browser[..end], direction).is_err());
                    assert!(decode_b(&browser[..end], direction, extended).is_err());
                }
            }
        }
        let invalid = std::fs::read(dir.join("tuple-invalid-slot-js.bin"))?;
        assert_eq!(
            decode_b(&invalid, Direction::Response, true),
            Err(WireError::Malformed)
        );
        decode_a(&invalid, Direction::Response)?;
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
    tests::check(dir)
}
