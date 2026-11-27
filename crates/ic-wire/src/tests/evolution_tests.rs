//! Literal and admission checks for the shared projection engine.

use crate::{
    Direction, Encoding, Error, LimitKind, Limits,
    evolution::{
        Extensions, ExtensionsWriter, OpaqueOpen, ReadContext, WriteContext, decode_value,
        encode_value_into, encode_value_to_slice, encode_value_to_slice_with,
    },
};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeTuple};
use std::{boxed::Box, cell::Cell, vec, vec::Vec};

const LIMITS: Limits = Limits {
    bytes: 64,
    allocation: 16_384,
    nodes: 1024,
    depth: 12,
};

#[test]
fn present_open_scopes_refuse_at_the_first_visit_that_exceeds_the_limits() {
    let body = [2];
    let decode = |nodes, depth| {
        let limits = Limits {
            nodes,
            depth,
            ..LIMITS
        };
        decode_value(
            &body,
            Encoding::Payload,
            limits,
            |open: OpaqueOpen<'_>, ctx| open.visit(ctx, |_, field| field.decode::<()>()),
        )
    };
    assert_eq!(decode(3, 2), Err(Error::Limit(LimitKind::Nodes)));
    assert_eq!(decode(4, 2), Err(Error::Limit(LimitKind::Depth)));
    assert_eq!(decode(5, 3), Err(Error::Limit(LimitKind::Nodes)));
    assert_eq!(decode(6, 3), Ok(Some(())));
}
const FRAME: Encoding = Encoding::Frame {
    method: 7,
    direction: Direction::Response,
};

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Row<'a> {
    #[serde(borrow)]
    state: OpaqueOpen<'a>,
    #[serde(borrow)]
    extensions: Extensions<'a>,
    suffix: u32,
}

fn project<'a>(
    row: Row<'a>,
    context: &ReadContext<'_>,
) -> Result<(Option<bool>, Option<&'a str>, u32), Error> {
    let state = row
        .state
        .visit(context, |tag, field| match tag {
            1 => field.decode::<()>().map(|()| Some(true)),
            4 => field.decode::<bool>().map(Some),
            _ => Ok(None),
        })?
        .flatten();
    let mut note = None;
    row.extensions.visit(context, |id, field| {
        if id == 2 {
            note = Some(field.decode::<&str>()?);
        }
        Ok(())
    })?;
    Ok((state, note, row.suffix))
}

#[test]
fn open_unknown_payload_skips_without_hiding_known_malformed_data() {
    let body = [11, 1, 2, 4, 2, 2, 1, b'x', 99];
    let (variant, note, suffix) = decode_value(&body, Encoding::Payload, LIMITS, project).unwrap();
    assert_eq!((variant, note, suffix), (None, Some("x"), 99));
    assert_eq!(note.unwrap().as_ptr(), body[7..].as_ptr());

    let bad_known_variant = [9, 1, 2, 4, 2, 2, 1, b'x', 99];
    let extra_known_unit_payload = [3, 1, 0, 0, 99];
    let absent_with_payload = [1, 1, 0, 0, 99];
    let odd_tag_empty_payload = [11, 0, 0, 99];
    let invalid_note_after_unknown = [11, 1, 2, 4, 2, 2, 1, 0xff, 99];
    for bytes in [
        bad_known_variant.as_slice(),
        &extra_known_unit_payload,
        &absent_with_payload,
        &odd_tag_empty_payload,
        &invalid_note_after_unknown,
    ] {
        assert_eq!(
            decode_value(bytes, Encoding::Payload, LIMITS, project),
            Err(Error::Malformed)
        );
    }
}

#[test]
fn extension_order_and_empty_presence_have_distinct_results() {
    assert_eq!(
        decode_value(&[2, 0, 99], Encoding::Payload, LIMITS, project),
        Ok((Some(true), None, 99))
    );
    assert_eq!(
        decode_value(&[2, 3, 2, 1, 0, 99], Encoding::Payload, LIMITS, project),
        Ok((Some(true), Some(""), 99))
    );

    for body in [
        [0, 4, 3, 0, 3, 0, 99], // Duplicate ID, both unknown.
        [0, 4, 4, 0, 3, 0, 99], // Descending IDs, both unknown.
        [0, 4, 0, 0, 3, 0, 99], // Reserved zero ID.
        [0, 4, 3, 1, 0, 4, 99], // Last ID has no length.
    ] {
        assert_eq!(
            decode_value(&body, Encoding::Payload, LIMITS, project),
            Err(Error::Malformed)
        );
    }

    let body = [0, 5, 1, 0, 2, 1, 0, 99];
    let preserved = decode_value(&body, Encoding::Payload, LIMITS, |row: Row<'_>, context| {
        let mut saw_retired = false;
        row.extensions.visit(context, |id, field| {
            if id == 1 {
                field.decode::<()>()?;
                saw_retired = true;
            }
            Ok(())
        })?;
        Ok(saw_retired)
    });
    assert_eq!(preserved, Ok(true));
}

#[test]
fn extension_headers_keep_stock_postcard_byte_span_acceptance() {
    let mut block = vec![1, 0, 2, 0x80, 1, 127];
    block.extend([b'a'; 127]);
    block.extend([127, 127, 126]);
    block.extend([b'b'; 126]);
    block.extend([0x80, 1, 0]); // The smallest two-byte ID.
    block.extend([0x81, 0x81, 0, 0x80, 0]); // Redundant ID and empty length.

    let input = postcard::to_allocvec(serde_bytes::Bytes::new(&block)).unwrap();
    let mut stock = block.as_slice();
    let limits = Limits {
        bytes: input.len(),
        ..LIMITS
    };
    let result = decode_value(
        &input,
        Encoding::Payload,
        limits,
        |extension: Extensions<'_>, context| {
            extension.visit(context, |id, field| {
                let ((expected, span), remaining) =
                    postcard::take_from_bytes::<(u32, &serde_bytes::Bytes)>(stock).unwrap();
                assert_eq!(id, expected);
                stock = remaining;
                match id {
                    1 | 128 | 129 => field.decode::<()>(),
                    2 | 127 => {
                        let data = field.decode::<&serde_bytes::Bytes>()?;
                        assert_eq!(data.as_ref(), &span.as_ref()[1..]);
                        Ok(())
                    }
                    _ => unreachable!("unexpected field ID"),
                }
            })
        },
    );
    assert_eq!(result, Ok(()));
    assert!(stock.is_empty());
}

#[test]
fn malformed_extension_headers_keep_callback_order_and_limit_precedence() {
    let malformed: &[(&[u8], &[u32])] = &[
        (&[1], &[]),                         // Missing length.
        (&[1, 1], &[]),                      // Short extent exceeds the block.
        (&[1, 0x80], &[]),                   // Truncated wide length.
        (&[1, 0x80, 1, 0], &[]),             // Wide extent exceeds the block.
        (&[0x80], &[]),                      // Truncated wide ID.
        (&[0, 0], &[]),                      // Zero ID.
        (&[0x80, 0, 0], &[]),                // Redundant zero ID.
        (&[1, 0, 1, 0], &[1]),               // Duplicate short ID.
        (&[1, 0, 0x81, 0, 0], &[1]),         // Duplicate with redundant ID.
        (&[0x80, 1, 0, 0x80, 1, 0], &[128]), // Duplicate wide ID.
        (&[0x80, 1, 0, 1, 0], &[128]),       // Descending across both parsers.
    ];
    for &(block, expected) in malformed {
        let input = postcard::to_allocvec(serde_bytes::Bytes::new(block)).unwrap();
        let short = Limits {
            nodes: 2 + expected.len(),
            ..LIMITS
        };
        for (limits, error) in [
            (short, Error::Limit(LimitKind::Nodes)),
            (LIMITS, Error::Malformed),
        ] {
            let mut seen = Vec::new();
            let result = decode_value(
                &input,
                Encoding::Payload,
                limits,
                |extension: Extensions<'_>, context| {
                    extension.visit(context, |id, _| {
                        seen.push(id);
                        Ok(())
                    })
                },
            );
            assert_eq!(result, Err(error), "{block:?}");
            assert_eq!(seen, expected, "{block:?}");
        }
    }
}

#[test]
fn writer_uses_reusable_output_and_caller_scratch() {
    let mut open = [0; 8];
    let mut extensions = [0; 8];
    let mut output = Vec::with_capacity(64);
    let original = output.as_ptr();
    let written = encode_value_into(&mut output, FRAME, LIMITS, |context| {
        let mut writer = ExtensionsWriter::new(&mut extensions);
        writer.push(context, 1, &())?;
        writer.push(context, 2, "x")?;
        Ok(Row {
            state: OpaqueOpen::write(context, 5, &2_u8, &mut open)?,
            extensions: writer.finish(),
            suffix: 99,
        })
    })
    .unwrap();
    assert_eq!(
        written,
        &[
            b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 1, 11, 1, 2, 6, 1, 0, 2, 2, 1, b'x', 99
        ]
    );
    assert_eq!(written.as_ptr(), original);
    assert_eq!(
        decode_value(written, FRAME, LIMITS, project),
        Ok((None, Some("x"), 99))
    );

    let result = encode_value_into(&mut output, Encoding::Payload, LIMITS, |context| {
        let mut writer = ExtensionsWriter::new(&mut extensions);
        writer.push(context, 2, "x")?;
        writer.push(context, 2, "y")?;
        Ok(writer.finish())
    });
    assert_eq!(result, Err(Error::Malformed));
    assert!(output.is_empty());
}

#[test]
fn leaf_decodes_share_exact_limits_and_cannot_hide_refusal() {
    let body = [5, 1, 0, 2, 1, 7];
    let limits = Limits {
        bytes: body.len(),
        allocation: body.len() + 8 * (size_of::<&[u8]>() + 5 + size_of::<u8>()),
        nodes: 6,
        depth: 3,
    };
    let project = |extensions: Extensions<'_>, context: &ReadContext<'_>| {
        extensions.visit(context, |id, field| match id {
            1 => field.decode::<()>(),
            2 => field.decode::<u8>().map(|_| ()),
            _ => unreachable!(),
        })
    };
    assert_eq!(
        decode_value(&body, Encoding::Payload, limits, project),
        Ok(())
    );
    for (short, kind) in [
        (
            Limits {
                bytes: limits.bytes - 1,
                ..limits
            },
            LimitKind::Bytes,
        ),
        (
            Limits {
                allocation: limits.allocation - 1,
                ..limits
            },
            LimitKind::Allocation,
        ),
        (
            Limits {
                nodes: limits.nodes - 1,
                ..limits
            },
            LimitKind::Nodes,
        ),
        (
            Limits {
                depth: limits.depth - 1,
                ..limits
            },
            LimitKind::Depth,
        ),
    ] {
        assert_eq!(
            decode_value(&body, Encoding::Payload, short, project),
            Err(Error::Limit(kind))
        );
    }
    let short = Limits {
        nodes: limits.nodes - 1,
        ..limits
    };
    assert_eq!(
        decode_value(&body, Encoding::Payload, short, |extensions, context| {
            let _ = project(extensions, context);
            Err::<(), _>(Error::Incompatible)
        }),
        Err(Error::Limit(LimitKind::Nodes))
    );
}

#[test]
fn writes_share_budget_across_leaves_and_output_passes() {
    let mut scratch = [0; 8];
    let mut output = Vec::new();
    let limits = Limits {
        bytes: 6,
        nodes: 18,
        ..LIMITS
    };
    let mut encode = |limits| {
        encode_value_into(&mut output, Encoding::Payload, limits, |context| {
            let mut writer = ExtensionsWriter::new(&mut scratch);
            writer.push(context, 1, &())?;
            writer.push(context, 2, &7_u8)?;
            Ok(writer.finish())
        })
        .map(<[u8]>::len)
    };
    assert_eq!(encode(limits), Ok(6));
    assert_eq!(
        encode(Limits {
            nodes: 17,
            ..limits
        }),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert!(output.is_empty());
}

#[test]
fn single_scratch_pass_keeps_sibling_spans_and_prefix_boundaries() {
    struct Counted<'a>(&'a Cell<usize>);
    impl Serialize for Counted<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.0.set(self.0.get() + 1);
            serializer.serialize_u8(2)
        }
    }

    let visits = Cell::new(0);
    let mut scratch = [0; 5];
    let mut output = Vec::new();
    let bytes = encode_value_into(&mut output, Encoding::Payload, LIMITS, |ctx| {
        let (open, tail) = OpaqueOpen::write_in(ctx, 5, &Counted(&visits), &mut scratch)?;
        let mut writer = ExtensionsWriter::new(tail);
        writer.push(ctx, 2, "x")?;
        let (extensions, remaining) = writer.finish_and_tail();
        assert!(remaining.is_empty());
        Ok(Row {
            state: open,
            extensions,
            suffix: 99,
        })
    })
    .unwrap();
    assert_eq!(visits.get(), 1);
    assert_eq!(bytes, [11, 1, 2, 4, 2, 2, 1, b'x', 99]);

    let payload = [0x2a; 127];
    let payload = crate::BytesRef::<127>::new(&payload).unwrap();
    let limits = Limits {
        bytes: 133,
        ..LIMITS
    };
    let mut scratch = [0; 131];
    let bytes = encode_value_into(&mut output, Encoding::Payload, limits, |ctx| {
        let mut writer = ExtensionsWriter::new(&mut scratch);
        writer.push(ctx, 1, &payload)?;
        Ok(writer.finish())
    })
    .unwrap();
    assert_eq!(bytes.len(), 133);
    assert_eq!(&bytes[..6], &[0x83, 1, 1, 0x80, 1, 0x7f]);
    assert_eq!(&bytes[6..], &[0x2a; 127]);

    // The body and minimum prefix fit. Expanding the length requires one
    // extra byte, so return a sticky error before shifting the staged bytes.
    let result = encode_value_into(&mut output, Encoding::Payload, limits, |ctx| {
        let mut writer = ExtensionsWriter::new(&mut scratch[..130]);
        assert_eq!(
            writer.push(ctx, 1, &payload),
            Err(Error::Limit(LimitKind::Scratch))
        );
        Ok(writer.finish())
    });
    assert_eq!(result, Err(Error::Limit(LimitKind::Scratch)));
    assert!(output.is_empty());
    let too_short = Limits {
        bytes: 130,
        ..limits
    };
    assert_eq!(
        encode_value_into(&mut output, Encoding::Payload, too_short, |ctx| {
            let mut writer = ExtensionsWriter::new(&mut scratch);
            writer.push(ctx, 1, &payload)?;
            Ok(writer.finish())
        }),
        Err(Error::Limit(LimitKind::Bytes))
    );
    assert!(output.is_empty());
}

#[test]
fn staged_extension_headers_match_stock_postcard_across_id_and_length_widths() {
    let full = [0x2a; 127];
    let body = crate::BytesRef::<127>::new(&full).unwrap();
    let mut output = Vec::new();
    let mut scratch = [0; 141];
    let limits = Limits {
        bytes: 143,
        ..LIMITS
    };
    let bytes = encode_value_into(&mut output, Encoding::Payload, limits, |context| {
        let mut writer = ExtensionsWriter::new(&mut scratch);
        writer.push(context, 127, &())?;
        writer.push(context, 128, &1_u8)?;
        writer.push(context, u32::MAX, &body)?;
        Ok(writer.finish())
    })
    .unwrap();
    assert_eq!(
        &bytes[..16],
        &[
            0x8d, 0x01, 0x7f, 0, 0x80, 1, 1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x80, 1, 0x7f
        ]
    );
    assert_eq!(&bytes[16..], &full);
}

#[test]
fn caught_scratch_capacity_and_nested_buffer_refusals_stay_hard() {
    struct Catches;
    impl Serialize for Catches {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple(1)?;
            let _ = tuple.serialize_element(&1024_u16);
            tuple.end()
        }
    }

    let mut output = Vec::new();
    for (bytes, scratch_len, kind) in [(64, 1, LimitKind::Scratch), (1, 4, LimitKind::Bytes)] {
        let mut scratch = [0; 4];
        let limits = Limits { bytes, ..LIMITS };
        let result = encode_value_into(&mut output, Encoding::Payload, limits, |ctx| {
            let error = OpaqueOpen::write(ctx, 1, &Catches, &mut scratch[..scratch_len]);
            assert_eq!(error, Err(Error::Limit(kind)));
            Ok(())
        });
        assert_eq!(result, Err(Error::Limit(kind)));
        assert!(output.is_empty());
    }

    struct Payload;
    impl Serialize for Payload {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(b"ab")
        }
    }

    for units in [4, 5] {
        let mut scratch = [0; 4];
        let limits = Limits {
            allocation: LIMITS.bytes + 8 * units,
            ..LIMITS
        };
        let result = encode_value_into(&mut output, Encoding::Payload, limits, |ctx| {
            OpaqueOpen::write(ctx, 1, &Payload, &mut scratch)?;
            Ok(())
        });
        let expected = if units == 4 {
            Err(Error::Limit(LimitKind::Allocation))
        } else {
            Ok([].as_slice())
        };
        assert_eq!(result, expected);
    }
}

#[test]
fn projected_slice_keeps_frame_bytes_and_shared_admission() {
    let limits = Limits {
        bytes: 4,
        allocation: 4 + 8 * (8 + 2 * size_of::<Extensions<'_>>()),
        nodes: 9,
        depth: 5,
    };
    let mut scratch = [0; 3];
    let mut output = [0; 4];
    let mut encode = |limits| {
        encode_value_to_slice(&mut output, Encoding::Payload, limits, |ctx| {
            let mut writer = ExtensionsWriter::new(&mut scratch);
            writer.push(ctx, 1, &7_u8)?;
            Ok(writer.finish())
        })
        .map(<[u8]>::to_vec)
    };
    assert_eq!(encode(limits), Ok(vec![3, 1, 1, 7]));
    for (limits, kind) in [
        (Limits { bytes: 3, ..limits }, LimitKind::Bytes),
        (
            Limits {
                allocation: limits.allocation - 1,
                ..limits
            },
            LimitKind::Allocation,
        ),
        (Limits { nodes: 8, ..limits }, LimitKind::Nodes),
        (Limits { depth: 4, ..limits }, LimitKind::Depth),
    ] {
        assert_eq!(encode(limits), Err(Error::Limit(kind)));
    }

    let prepare = |_: &WriteContext<'_>| {
        Ok(Row {
            state: OpaqueOpen::unit(3)?,
            extensions: Extensions::empty(),
            suffix: 99,
        })
    };
    let mut output = [0; 14];
    let mut expected = Vec::new();
    encode_value_into(&mut expected, FRAME, LIMITS, prepare).unwrap();
    assert_eq!(
        encode_value_to_slice(&mut output, FRAME, LIMITS, prepare),
        Ok(expected.as_slice())
    );
    assert_eq!(
        encode_value_to_slice(&mut output[..13], FRAME, LIMITS, prepare),
        Err(Error::Limit(LimitKind::Output))
    );
}

#[test]
fn projected_slice_keeps_swallowed_final_output_refusals() {
    struct Catches;
    impl Serialize for Catches {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut tuple = serializer.serialize_tuple(1)?;
            let _ = tuple.serialize_element(&1024_u16);
            tuple.end()
        }
    }

    let mut output = [0; 1];
    assert_eq!(
        encode_value_to_slice(&mut output, Encoding::Payload, LIMITS, |_| Ok(Catches)),
        Err(Error::Limit(LimitKind::Output))
    );
    let narrow = Limits { bytes: 1, ..LIMITS };
    assert_eq!(
        encode_value_to_slice(&mut output, Encoding::Payload, narrow, |_| Ok(Catches)),
        Err(Error::Limit(LimitKind::Bytes))
    );
}

#[test]
fn scoped_slice_stages_a_borrowed_dto_during_the_final_write() {
    struct Lazy<'a, 'scope>(&'a WriteContext<'scope>);

    impl Serialize for Lazy<'_, '_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut scratch = [0; 3];
            let mut fields = ExtensionsWriter::new(&mut scratch);
            fields
                .push(self.0, 1, &7_u8)
                .map_err(serde::ser::Error::custom)?;
            fields.finish().serialize(serializer)
        }
    }

    let mut output = [0; 4];
    let mut encode = |nodes| {
        let limits = Limits {
            bytes: output.len(),
            nodes,
            ..LIMITS
        };
        encode_value_to_slice_with(&mut output, Encoding::Payload, limits, |mut destination| {
            let context = destination.context();
            destination.write_value(&Lazy(&context))
        })
        .map(<[u8]>::to_vec)
    };
    assert_eq!(encode(9), Ok(vec![3, 1, 1, 7]));
    assert_eq!(encode(8), Err(Error::Limit(LimitKind::Nodes)));
}

#[test]
fn scoped_slice_publishes_only_after_one_successful_write_and_callback() {
    let mut output = [0xbb; 12];
    assert_eq!(
        encode_value_to_slice_with(&mut output, FRAME, LIMITS, |_| Ok(())),
        Err(Error::Malformed)
    );
    assert_eq!(output, [0xbb; 12]);

    assert_eq!(
        encode_value_to_slice_with(&mut output, FRAME, LIMITS, |mut destination| {
            destination.write_value(&7_u8)?;
            Err(Error::Incompatible)
        }),
        Err(Error::Incompatible)
    );
    assert_eq!(output[..11], [0xbb; 11]);

    let limits = Limits { nodes: 4, ..LIMITS };
    assert_eq!(
        encode_value_to_slice_with(&mut output, FRAME, limits, |mut destination| {
            let context = destination.context();
            destination.write_value(&7_u8)?;
            let _ = context.nested(|_| Ok(()));
            Ok(())
        }),
        Err(Error::Limit(LimitKind::Nodes))
    );
    assert_eq!(output[..11], [0xbb; 11]);

    let bytes = encode_value_to_slice_with(&mut output, FRAME, LIMITS, |mut destination| {
        destination.write_value(&7_u8)
    })
    .unwrap();
    assert_eq!(bytes, [b'I', b'W', 7, 0, 0, 0, 0, 0, 0, 0, 1, 7]);

    let bytes =
        encode_value_to_slice_with(&mut [], Encoding::Payload, LIMITS, |mut destination| {
            destination.write_value(&())
        })
        .unwrap();
    assert!(bytes.is_empty());
}

#[derive(Serialize, Deserialize)]
struct Chain<'a> {
    id: u8,
    #[serde(borrow)]
    next: Option<Box<Chain<'a>>>,
    #[serde(borrow)]
    fields: Extensions<'a>,
}

#[derive(Serialize, Deserialize)]
struct Packet<'a>(u16, #[serde(borrow)] Chain<'a>, u64);

fn collect_chain(
    chain: Chain<'_>,
    context: &ReadContext<'_>,
    knows_marker: bool,
    output: &mut Vec<(u8, bool)>,
) -> Result<(), Error> {
    let mut marker = false;
    chain.fields.visit(context, |id, field| {
        if knows_marker && id == 7 {
            field.decode::<()>()?;
            marker = true;
        }
        Ok(())
    })?;
    output.push((chain.id, marker));
    if let Some(next) = chain.next {
        context.nested(|option| {
            option.nested(|child| collect_chain(*next, child, knows_marker, output))
        })?;
    }
    Ok(())
}

#[test]
fn extensions_inside_a_recursive_tuple_slot_keep_the_fixed_neighbors() {
    let mut scratch = [0; 8];
    let mut bytes = Vec::new();
    for writer_knows_marker in [false, true] {
        encode_value_into(&mut bytes, FRAME, LIMITS, |ctx| {
            let fields = ctx.nested(|chain| {
                chain.nested(|option| {
                    option.nested(|child| {
                        let mut fields = ExtensionsWriter::new(&mut scratch);
                        if writer_knows_marker {
                            fields.push(child, 7, &())?;
                        }
                        Ok(fields.finish())
                    })
                })
            })?;
            Ok(Packet(
                257,
                Chain {
                    id: 7,
                    fields: Extensions::empty(),
                    next: Some(Box::new(Chain {
                        id: 10,
                        fields,
                        next: None,
                    })),
                },
                1_u64 << 60,
            ))
        })
        .unwrap();
        for reader_knows_marker in [false, true] {
            let (head, chain, tail) =
                decode_value(&bytes, FRAME, LIMITS, |packet: Packet<'_>, ctx| {
                    ctx.admit_allocation::<(u8, bool)>(2)?;
                    let mut output = Vec::with_capacity(2);
                    ctx.nested(|child| {
                        collect_chain(packet.1, child, reader_knows_marker, &mut output)
                    })?;
                    Ok((packet.0, output, packet.2))
                })
                .unwrap();
            assert_eq!(head, 257);
            assert_eq!(tail, 1_u64 << 60);
            assert_eq!(
                chain,
                [(7, false), (10, writer_knows_marker && reader_knows_marker)]
            );
        }
    }
}
