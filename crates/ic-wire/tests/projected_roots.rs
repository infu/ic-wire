//! Literal checks for manually projected public message roots.

use ic_wire::{
    Encoding, Error, LimitKind, Limits,
    evolution::{ExtensionsWriter, Projected, decode_projected, encode_projected_to_slice_with},
};

const LIMITS: Limits = Limits {
    bytes: 128,
    allocation: 8192,
    nodes: 128,
    depth: 12,
};

#[test]
fn scoped_projection_writes_and_requires_a_sparse_root() {
    let mut storage = [0xff; 5];
    let mut scratch = [0; 3];
    let bytes = encode_projected_to_slice_with(&mut storage, Encoding::Payload, LIMITS, |dest| {
        let mut extensions = ExtensionsWriter::new(&mut scratch);
        extensions.push(&dest.context(), 7, &true)?;
        dest.write(&Projected::new(42_u8, extensions.finish()))
    })
    .unwrap();
    assert_eq!(bytes, [42, 3, 7, 1, 1]);

    let older_reader = |bytes: &[u8]| {
        decode_projected(
            bytes,
            Encoding::Payload,
            LIMITS,
            |message: Projected<'_, u8>, ctx| {
                message.extensions.visit(ctx, |_, _| Ok(()))?;

                Ok(message.core)
            },
        )
    };
    assert_eq!(older_reader(bytes), Ok(42));
    assert_eq!(older_reader(&[42]), Err(Error::Malformed));
    assert_eq!(older_reader(&[42, 2, 7, 1]), Err(Error::Malformed));
    assert_eq!(older_reader(&[42, 0, 0]), Err(Error::Trailing));

    assert_eq!(
        encode_projected_to_slice_with(&mut storage[..1], Encoding::Payload, LIMITS, |dest| {
            dest.write(&Projected::empty(42_u8))
        }),
        Err(Error::Limit(LimitKind::Output))
    );
}
