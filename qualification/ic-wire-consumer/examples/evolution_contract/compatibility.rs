//! Directional compatibility for the sparse extension contract.
//!
//! A host can project the existing per-method manifest's reachable shape graph
//! into this model. The deployed name, method ID, request and response roots
//! come from the manifest. Invocation mode and stable slot, extension and case
//! identities must come from the owning declarations. Current manifest field
//! order and variant ordinals cannot supply these identities for the new format.
//! An extension record or tuple includes block framing even with no descriptors.
//! Tuple tails use contiguous positions with stable names and no tombstones.
//! Text, byte and list limits are value bounds. This check does not prove frame
//! size, allocation or instruction limits, or account for custom Serde behavior.

#![allow(dead_code, reason = "Private callers select subsets of schema kinds")]

use std::collections::BTreeSet;

const MAX_DEPTH: usize = 64;
const MAX_STEPS: usize = 4096;
const MAX_DROP_PATHS: usize = MAX_STEPS;
const MAX_MEMBERS: usize = 256;
const MAX_HISTORY: usize = 32;
const MAX_NAME: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scalar {
    Unit,
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    FixedBytes(u32),
}

type Positional<'a> = [&'a Shape<'a>];

#[derive(Clone, Copy)]
pub(super) enum Shape<'a> {
    Scalar(Scalar),
    Number {
        scalar: Scalar,
        min: Option<f64>,
        max: Option<f64>,
    },
    Text {
        max: u32,
    },
    Bytes {
        max: u32,
    },
    BoundedBytes {
        min: u32,
        max: u32,
    },
    Packed {
        word: &'a str,
        nonzero: bool,
        max: u32,
    },
    List {
        element: &'a Shape<'a>,
        max: u32,
    },
    Array {
        element: &'a Shape<'a>,
        len: u32,
    },
    Option(&'a Shape<'a>),
    Recover(&'a Shape<'a>),
    Result {
        ok: &'a Shape<'a>,
        err: &'a Shape<'a>,
    },
    Tuple(&'a Positional<'a>),
    ExtensionTuple {
        core: &'a Positional<'a>,
        tail: &'a [Slot<'a>],
    },
    FixedRecord(&'a [Slot<'a>]),
    /// The extension block length follows the core even with no descriptors.
    Record {
        core: &'a [Slot<'a>],
        extensions: &'a [Extension<'a>],
    },
    /// Fused absent/known/unknown representation. Unknown tags become absent.
    OpenVariant(&'a [Case<'a>]),
    /// Fused representation with no absent tag. Unknown tags need a recovery scope.
    RequiredOpenVariant(&'a [Case<'a>]),
    ClosedVariant(&'a [Case<'a>]),
    /// Index into the host projection's definition slice.
    Reference(usize),
}

pub(super) struct Slot<'a> {
    pub id: &'a str,
    pub shape: &'a Shape<'a>,
}

#[derive(Clone, Copy)]
pub(super) enum ExtensionField<'a> {
    Optional(&'a Shape<'a>),
    Required(&'a Shape<'a>),
    Retired,
}

impl<'a> ExtensionField<'a> {
    fn live(self) -> Option<&'a Shape<'a>> {
        match self {
            Self::Optional(shape) | Self::Required(shape) => Some(shape),
            Self::Retired => None,
        }
    }
}

pub(super) struct Extension<'a> {
    pub id: u32,
    pub name: &'a str,
    pub field: ExtensionField<'a>,
}

pub(super) struct Case<'a> {
    pub id: u32,
    pub name: &'a str,
    pub payload: Option<&'a Shape<'a>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Query,
    CompositeQuery,
    Update,
}

pub(super) struct Method<'a> {
    pub name: &'a str,
    pub id: u64,
    pub mode: Mode,
    pub request: &'a Shape<'a>,
    pub response: &'a Shape<'a>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Compatibility {
    Preserved,
    Drops(Vec<String>),
    Breaking { path: String, reason: &'static str },
    Unsupported { path: String, reason: &'static str },
}

struct Fault {
    path: String,
    reason: &'static str,
    unsupported: bool,
}

type Check = Result<(), Fault>;
type Node = *const ();

fn fail(path: &str, reason: &'static str) -> Check {
    Err(Fault {
        path: path.into(),
        reason,
        unsupported: false,
    })
}

fn unsupported(path: &str, reason: &'static str) -> Check {
    Err(Fault {
        path: path.into(),
        reason,
        unsupported: true,
    })
}

fn node(shape: &Shape<'_>) -> Node {
    std::ptr::from_ref(shape).cast()
}

fn step(remaining: &mut usize, depth: usize, path: &str) -> Check {
    if *remaining == 0 || depth >= MAX_DEPTH {
        return unsupported(path, "schema check exceeds its complexity limit");
    }
    *remaining -= 1;
    Ok(())
}

fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_NAME
}

fn count(value: usize, path: &str) -> Check {
    if value > MAX_MEMBERS {
        return unsupported(path, "schema has too many members to check");
    }
    Ok(())
}

fn resolve<'a>(
    mut shape: &'a Shape<'a>,
    definitions: &'a [Shape<'a>],
    remaining: &mut usize,
    path: &str,
) -> Result<&'a Shape<'a>, Fault> {
    let mut seen = 0;
    while let Shape::Reference(index) = shape {
        step(remaining, seen, path)?;
        shape = definitions.get(*index).ok_or_else(|| Fault {
            path: path.into(),
            reason: "schema reference has no definition",
            unsupported: false,
        })?;
        seen += 1;
    }
    Ok(shape)
}

struct Admission<'a> {
    definitions: &'a [Shape<'a>],
    seen: BTreeSet<Node>,
    remaining: usize,
}

impl<'a> Admission<'a> {
    fn new(definitions: &'a [Shape<'a>]) -> Self {
        Self {
            definitions,
            seen: BTreeSet::new(),
            remaining: MAX_STEPS,
        }
    }

    fn slots(&mut self, slots: &[Slot<'a>], depth: usize, path: &str) -> Check {
        count(slots.len(), path)?;
        let mut ids = BTreeSet::new();
        for slot in slots {
            if !name(slot.id) || !ids.insert(slot.id) {
                return fail(path, "fixed slots must have unique stable identities");
            }
            self.shape(slot.shape, depth + 1, &format!("{path}.{}", slot.id))?;
        }
        Ok(())
    }

    fn positional(&mut self, items: &Positional<'a>, depth: usize, path: &str) -> Check {
        count(items.len(), path)?;
        for (position, shape) in items.iter().enumerate() {
            self.shape(shape, depth + 1, &format!("{path}.{position}"))?;
        }
        Ok(())
    }

    fn shape(&mut self, shape: &'a Shape<'a>, depth: usize, path: &str) -> Check {
        step(&mut self.remaining, depth, path)?;
        let shape = resolve(shape, self.definitions, &mut self.remaining, path)?;
        if !self.seen.insert(node(shape)) {
            return Ok(());
        }
        match shape {
            Shape::Scalar(_) | Shape::Text { .. } | Shape::Bytes { .. } | Shape::Packed { .. } => {
                Ok(())
            }
            Shape::BoundedBytes { min, max } if min <= max => Ok(()),
            Shape::BoundedBytes { .. } => fail(path, "byte bounds are inverted"),
            Shape::Number { min, max, .. }
                if min.iter().chain(max).all(|limit| limit.is_finite())
                    && !min.zip(*max).is_some_and(|(min, max)| min > max) =>
            {
                Ok(())
            }
            Shape::Number { .. } => fail(path, "numeric bounds are invalid"),
            Shape::Reference(_) => unreachable!("references resolved before admission"),
            Shape::Option(element) | Shape::Recover(element) => {
                self.shape(element, depth + 1, &format!("{path}.some"))
            }
            Shape::Result { ok, err } => {
                self.shape(ok, depth + 1, &format!("{path}.ok"))?;
                self.shape(err, depth + 1, &format!("{path}.err"))
            }
            Shape::List { element, .. } | Shape::Array { element, .. } => {
                self.shape(element, depth + 1, &format!("{path}[]"))
            }
            Shape::Tuple(items) => self.positional(items, depth, path),
            Shape::FixedRecord(slots) => self.slots(slots, depth, path),
            Shape::ExtensionTuple { core, tail } => {
                count(core.len() + tail.len(), path)?;
                self.positional(core, depth, path)?;
                let mut names = BTreeSet::new();
                for (index, slot) in tail.iter().enumerate() {
                    let position = core.len() + index;
                    let path = format!("{path}.{position}");
                    if !name(slot.id) || !names.insert(slot.id) {
                        return fail(&path, "tuple tail slots must have unique stable names");
                    }
                    self.shape(slot.shape, depth + 1, &path)?;
                }
                Ok(())
            }
            Shape::Record { core, extensions } => {
                self.slots(core, depth, path)?;
                count(extensions.len(), path)?;
                let mut ids = BTreeSet::new();
                let mut names = BTreeSet::new();
                for ext in *extensions {
                    step(&mut self.remaining, depth, path)?;
                    if ext.id == 0
                        || !name(ext.name)
                        || !ids.insert(ext.id)
                        || !names.insert(ext.name)
                    {
                        return fail(
                            path,
                            "extensions must have unique nonzero IDs and stable names",
                        );
                    }
                    if let Some(value) = ext.field.live() {
                        self.shape(value, depth + 1, &extension_path(path, ext))?;
                    }
                }
                Ok(())
            }
            Shape::OpenVariant(cases)
            | Shape::RequiredOpenVariant(cases)
            | Shape::ClosedVariant(cases) => {
                count(cases.len(), path)?;
                let open = matches!(shape, Shape::OpenVariant(_) | Shape::RequiredOpenVariant(_));
                let mut ids = BTreeSet::new();
                let mut names = BTreeSet::new();
                for case in *cases {
                    step(&mut self.remaining, depth, path)?;
                    if (open && !(1..=u32::MAX >> 1).contains(&case.id))
                        || !name(case.name)
                        || !ids.insert(case.id)
                        || !names.insert(case.name)
                    {
                        return fail(
                            path,
                            "variant cases must have valid unique IDs and stable names",
                        );
                    }
                    if let Some(payload) = case.payload {
                        self.shape(payload, depth + 1, &case_path(path, case))?;
                    }
                }
                Ok(())
            }
        }
    }

    fn methods(&mut self, methods: &[Method<'a>]) -> Check {
        count(methods.len(), "$")?;
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for method in methods {
            if !name(method.name) || !names.insert(method.name) || !ids.insert(method.id) {
                return fail("$", "methods must have unique names and routing IDs");
            }
            self.shape(method.request, 0, &format!("$.{}.request", method.name))?;
            self.shape(method.response, 0, &format!("$.{}.response", method.name))?;
        }
        Ok(())
    }
}

fn extension_path(path: &str, ext: &Extension<'_>) -> String {
    format!("{path}.{}#{}", ext.name, ext.id)
}

fn case_path(path: &str, case: &Case<'_>) -> String {
    format!("{path}<{}#{}>", case.name, case.id)
}

#[derive(Clone, Copy)]
enum Chronology {
    Unrelated,
    WriterEarlier,
    ReaderEarlier,
}

#[derive(Clone, Copy)]
enum UnknownCase {
    Closed,
    Optional,
    Required,
}

struct Comparator<'a> {
    definitions: &'a [Shape<'a>],
    active: Vec<(Node, Node, bool)>,
    recovery: Vec<String>,
    remaining: usize,
    drops: Vec<String>,
}

impl<'a> Comparator<'a> {
    fn new(definitions: &'a [Shape<'a>]) -> Self {
        Self {
            definitions,
            active: Vec::new(),
            recovery: Vec::new(),
            remaining: MAX_STEPS,
            drops: Vec::new(),
        }
    }

    fn report(self, result: Check) -> Compatibility {
        match result {
            Err(Fault {
                path,
                reason,
                unsupported: true,
            }) => Compatibility::Unsupported { path, reason },
            Err(Fault { path, reason, .. }) => Compatibility::Breaking { path, reason },
            Ok(()) if self.drops.is_empty() => Compatibility::Preserved,
            Ok(()) => Compatibility::Drops(self.drops),
        }
    }

    fn recover_missing(&mut self, path: &str, reason: &'static str) -> Check {
        let Some(scope) = self.recovery.last() else {
            return fail(path, reason);
        };
        self.drops.push(scope.clone());
        Ok(())
    }

    fn shape(
        &mut self,
        writer: &'a Shape<'a>,
        reader: &'a Shape<'a>,
        order: Chronology,
        path: &str,
    ) -> Check {
        step(&mut self.remaining, self.active.len(), path)?;
        let writer = resolve(writer, self.definitions, &mut self.remaining, path)?;
        let reader = resolve(reader, self.definitions, &mut self.remaining, path)?;
        let pair = (node(writer), node(reader), !self.recovery.is_empty());
        if self.active.contains(&pair) {
            return Ok(());
        }
        self.active.push(pair);
        let checked = self.shape_inner(writer, reader, order, path);
        self.active.pop();
        checked
    }

    fn shape_inner(
        &mut self,
        writer: &'a Shape<'a>,
        reader: &'a Shape<'a>,
        order: Chronology,
        path: &str,
    ) -> Check {
        use Shape::*;
        match (writer, reader) {
            (Scalar(a), Scalar(b)) if a == b => Ok(()),
            (
                Number {
                    scalar: a,
                    min: amin,
                    max: amax,
                },
                Number {
                    scalar: b,
                    min: bmin,
                    max: bmax,
                },
            ) if a == b => number_bounds(*amin, *amax, *bmin, *bmax, path),
            (
                Scalar(a),
                Number {
                    scalar: b,
                    min,
                    max,
                },
            ) if a == b => number_bounds(None, None, *min, *max, path),
            (Number { scalar: a, .. }, Scalar(b)) if a == b => Ok(()),
            (Text { max: a }, Text { max: b }) | (Bytes { max: a }, Bytes { max: b }) => {
                bound(*a, *b, path)
            }
            (
                BoundedBytes {
                    min: amin,
                    max: amax,
                },
                BoundedBytes {
                    min: bmin,
                    max: bmax,
                },
            ) if amin >= bmin => bound(*amax, *bmax, path),
            (BoundedBytes { max: a, .. }, Bytes { max: b })
            | (Bytes { max: a }, BoundedBytes { min: 0, max: b }) => bound(*a, *b, path),
            (BoundedBytes { .. } | Bytes { .. }, BoundedBytes { .. }) => {
                fail(path, "reader minimum byte length excludes writer values")
            }
            (
                Packed {
                    word: a,
                    nonzero: an,
                    max: am,
                },
                Packed {
                    word: b,
                    nonzero: bn,
                    max: bm,
                },
            ) if a == b => {
                if *bn && !an {
                    return fail(path, "reader disallows zero packed words");
                }
                bound(*am, *bm, path)
            }
            (
                List {
                    element: a,
                    max: wa,
                },
                List {
                    element: b,
                    max: rb,
                },
            ) => {
                bound(*wa, *rb, path)?;
                self.shape(a, b, order, &format!("{path}[]"))
            }
            (
                Array {
                    element: a,
                    len: wa,
                },
                Array {
                    element: b,
                    len: rb,
                },
            ) if wa == rb => self.shape(a, b, order, &format!("{path}[]")),
            (Option(a), Option(b)) => self.shape(a, b, order, &format!("{path}.some")),
            (Recover(a), Recover(b)) => {
                self.recovery.push(path.into());
                let result = self.shape(a, b, order, &format!("{path}.some"));
                self.recovery.pop();
                result
            }
            (Result { ok: a, err: ea }, Result { ok: b, err: eb }) => {
                self.shape(a, b, order, &format!("{path}.ok"))?;
                self.shape(ea, eb, order, &format!("{path}.err"))
            }
            (Tuple(a), Tuple(b)) => self.positional(a, b, order, path),
            (FixedRecord(a), FixedRecord(b)) => self.slots(a, b, order, path),
            (FixedRecord(_), Record { .. })
            | (Record { .. }, FixedRecord(_))
            | (Tuple(_), ExtensionTuple { .. })
            | (ExtensionTuple { .. }, Tuple(_)) => fail(path, "extension block framing changed"),
            (ExtensionTuple { core: a, tail: at }, ExtensionTuple { core: b, tail: bt }) => {
                self.positional(a, b, order, path)?;
                self.tuple_tail(a.len(), at, bt, order, path)
            }
            (
                Record {
                    core: a,
                    extensions: ea,
                },
                Record {
                    core: b,
                    extensions: eb,
                },
            ) => {
                self.slots(a, b, order, path)?;
                self.extensions(ea, eb, order, path)
            }
            (OpenVariant(a) | RequiredOpenVariant(a), OpenVariant(b)) => {
                self.cases(a, b, UnknownCase::Optional, order, path)
            }
            (RequiredOpenVariant(a), RequiredOpenVariant(b)) => {
                self.cases(a, b, UnknownCase::Required, order, path)
            }
            (OpenVariant(_), RequiredOpenVariant(_)) => {
                fail(path, "writer can omit a required open variant")
            }
            (ClosedVariant(a), ClosedVariant(b)) => {
                self.cases(a, b, UnknownCase::Closed, order, path)
            }
            _ => fail(path, "wire representation changed"),
        }
    }

    fn slots(
        &mut self,
        writer: &[Slot<'a>],
        reader: &[Slot<'a>],
        order: Chronology,
        path: &str,
    ) -> Check {
        if writer.len() != reader.len() {
            return fail(path, "fixed slot count changed");
        }
        for (a, b) in writer.iter().zip(reader) {
            let path = format!("{path}.{}", a.id);
            if a.id != b.id {
                return fail(&path, "fixed slot was reordered or replaced");
            }
            self.shape(a.shape, b.shape, order, &path)?;
        }
        Ok(())
    }

    fn positional(
        &mut self,
        writer: &Positional<'a>,
        reader: &Positional<'a>,
        order: Chronology,
        path: &str,
    ) -> Check {
        if writer.len() != reader.len() {
            return fail(path, "fixed slot count changed");
        }
        for (position, (a, b)) in writer.iter().zip(reader).enumerate() {
            self.shape(a, b, order, &format!("{path}.{position}"))?;
        }
        Ok(())
    }

    fn tuple_tail(
        &mut self,
        core_len: usize,
        writer: &[Slot<'a>],
        reader: &[Slot<'a>],
        order: Chronology,
        path: &str,
    ) -> Check {
        let (older, newer) = match order {
            Chronology::WriterEarlier => (writer.len(), reader.len()),
            Chronology::ReaderEarlier => (reader.len(), writer.len()),
            Chronology::Unrelated => (0, 0),
        };
        if older > newer {
            let first_removed = core_len + newer;
            return fail(
                &format!("{path}.{first_removed}"),
                "published tuple tail slots cannot be removed",
            );
        }
        for (index, a) in writer.iter().enumerate() {
            step(&mut self.remaining, self.active.len(), path)?;
            let position = core_len + index;
            let path = format!("{path}.{position}");
            if let Some(b) = reader.get(index) {
                if a.id != b.id {
                    return fail(&path, "tuple tail slot changed meaning");
                }
                self.shape(a.shape, b.shape, order, &path)?;
            } else {
                self.drops.push(path);
            }
        }
        Ok(())
    }

    fn extensions(
        &mut self,
        writer: &[Extension<'a>],
        reader: &[Extension<'a>],
        order: Chronology,
        path: &str,
    ) -> Check {
        match order {
            Chronology::Unrelated => (),
            Chronology::WriterEarlier => extension_history(writer, reader, path)?,
            Chronology::ReaderEarlier => extension_history(reader, writer, path)?,
        }

        for a in writer {
            step(&mut self.remaining, self.active.len(), path)?;
            let Some(payload) = a.field.live() else {
                continue;
            };
            let path = extension_path(path, a);
            if let Some(b) = extension_at(reader, a.id) {
                if a.name != b.name {
                    return fail(&path, "extension ID changed meaning");
                }
                if let Some(expected) = b.field.live() {
                    self.shape(payload, expected, order, &path)?;
                    continue;
                }
            }
            self.drops.push(path);
        }

        for b in reader {
            step(&mut self.remaining, self.active.len(), path)?;
            if matches!(b.field, ExtensionField::Required(_))
                && !extension_at(writer, b.id)
                    .is_some_and(|a| matches!(a.field, ExtensionField::Required(_)))
            {
                self.recover_missing(
                    &extension_path(path, b),
                    "writer can omit a required extension",
                )?;
            }
        }
        Ok(())
    }

    fn cases(
        &mut self,
        writer: &[Case<'a>],
        reader: &[Case<'a>],
        unknown: UnknownCase,
        order: Chronology,
        path: &str,
    ) -> Check {
        for a in writer {
            step(&mut self.remaining, self.active.len(), path)?;
            let path = case_path(path, a);
            let Some(b) = reader.iter().find(|b| b.id == a.id) else {
                match unknown {
                    UnknownCase::Closed => {
                        return fail(&path, "reader does not recognize a closed variant case");
                    }
                    UnknownCase::Optional => self.drops.push(path),
                    UnknownCase::Required => {
                        self.recover_missing(
                            &path,
                            "unknown required variant has no recovery scope",
                        )?;
                    }
                }
                continue;
            };
            if a.name != b.name {
                return fail(&path, "variant case ID changed meaning");
            }
            static UNIT: Shape<'static> = Shape::Scalar(Scalar::Unit);
            self.shape(
                a.payload.unwrap_or(&UNIT),
                b.payload.unwrap_or(&UNIT),
                order,
                &path,
            )?;
        }
        Ok(())
    }

    fn upgrade(&mut self, old: &[Method<'a>], new: &[Method<'a>]) -> Check {
        self.remaining = MAX_STEPS;
        for before in old {
            let path = format!("$.{}", before.name);
            let Some(after) = new.iter().find(|method| method.name == before.name) else {
                return fail(&path, "method was removed");
            };
            if before.id != after.id || before.mode != after.mode {
                return fail(&path, "method routing ID or invocation mode changed");
            }
            self.shape(
                before.request,
                after.request,
                Chronology::WriterEarlier,
                &format!("{path}.request"),
            )?;
            self.shape(
                after.response,
                before.response,
                Chronology::ReaderEarlier,
                &format!("{path}.response"),
            )?;
        }
        Ok(())
    }
}

fn bound(writer: u32, reader: u32, path: &str) -> Check {
    if writer > reader {
        return fail(path, "reader bound excludes writer values");
    }
    Ok(())
}

fn number_bounds(
    writer_min: Option<f64>,
    writer_max: Option<f64>,
    reader_min: Option<f64>,
    reader_max: Option<f64>,
    path: &str,
) -> Check {
    if reader_min.is_some_and(|b| writer_min.is_none_or(|a| a < b))
        || reader_max.is_some_and(|b| writer_max.is_none_or(|a| a > b))
    {
        return fail(path, "reader numeric bounds exclude writer values");
    }
    Ok(())
}

fn extension_at<'a, 's>(entries: &'s [Extension<'a>], id: u32) -> Option<&'s Extension<'a>> {
    entries.iter().find(|ext| ext.id == id)
}

fn extension_history(old: &[Extension<'_>], new: &[Extension<'_>], path: &str) -> Check {
    for before in old {
        let path = extension_path(path, before);
        let Some(after) = extension_at(new, before.id) else {
            return fail(&path, "removed extensions need a retained tombstone");
        };
        if matches!(before.field, ExtensionField::Retired) && after.field.live().is_some() {
            return fail(&path, "retired extension ID was reused");
        }
        if before.name != after.name {
            return fail(&path, "extension ID changed meaning");
        }
    }
    Ok(())
}

/// Check writer values against reader expectations without a release lineage.
pub(super) fn check(writer: &Shape<'_>, reader: &Shape<'_>) -> Compatibility {
    let mut comparison = Comparator::new(&[]);
    let result = Admission::new(&[])
        .shape(writer, 0, "$")
        .and_then(|()| Admission::new(&[]).shape(reader, 0, "$"))
        .and_then(|()| comparison.shape(writer, reader, Chronology::Unrelated, "$"));
    comparison.report(result)
}

/// Check old requests and new replies, and keep retired IDs in the lineage.
pub(super) fn check_upgrade(old: &[Method<'_>], new: &[Method<'_>]) -> Compatibility {
    let mut comparison = Comparator::new(&[]);
    let result = Admission::new(&[])
        .methods(old)
        .and_then(|()| Admission::new(&[]).methods(new))
        .and_then(|()| comparison.upgrade(old, new));
    comparison.report(result)
}

/// Check a chronological series of supported baselines and the candidate.
///
/// Adjacent old baselines must have kept their tombstones. Every baseline is
/// then checked directly against the candidate to cover skipped releases.
pub(super) fn check_history(previous: &[&[Method<'_>]], new: &[Method<'_>]) -> Compatibility {
    check_projected_history(previous, new, &[])
}

pub(super) fn check_projected_history<'a>(
    previous: &[&[Method<'a>]],
    new: &[Method<'a>],
    definitions: &'a [Shape<'a>],
) -> Compatibility {
    let mut comparison = Comparator::new(definitions);
    let result = (|| {
        if previous.len() > MAX_HISTORY {
            return unsupported("$", "too many historical baselines to check");
        }
        Admission::new(definitions).methods(new)?;
        for baseline in previous {
            Admission::new(definitions).methods(baseline)?;
        }
        for pair in previous.windows(2) {
            Comparator::new(definitions).upgrade(pair[0], pair[1])?;
        }
        let mut drops = BTreeSet::new();
        for baseline in previous {
            comparison.upgrade(baseline, new)?;
            for path in comparison.drops.drain(..) {
                drops.insert(path);
                if drops.len() > MAX_DROP_PATHS {
                    return unsupported("$", "schema check found too many drop paths to report");
                }
            }
        }
        comparison.drops = drops.into_iter().collect();
        Ok(())
    })();
    comparison.report(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    static U32: Shape<'static> = Shape::Scalar(Scalar::U32);
    static TEXT: Shape<'static> = Shape::Text { max: 20 };
    static CORE: [Slot<'static>; 1] = [Slot {
        id: "id",
        shape: &U32,
    }];

    fn record<'a>(extensions: &'a [Extension<'a>]) -> Shape<'a> {
        Shape::Record {
            core: &CORE,
            extensions,
        }
    }

    fn update<'a>(request: &'a Shape<'a>, response: &'a Shape<'a>) -> [Method<'a>; 1] {
        [Method {
            name: "save",
            id: 7,
            mode: Mode::Update,
            request,
            response,
        }]
    }

    fn breaking(value: Compatibility, path: &str) {
        let Compatibility::Breaking { path: actual, .. } = value else {
            panic!("expected a breaking result at {path}, got {value:?}");
        };
        assert_eq!(actual, path);
    }

    #[test]
    fn nested_extension_and_unknown_tag_have_independent_drop_paths() {
        let old_state = Shape::OpenVariant(&[Case {
            id: 1,
            name: "Active",
            payload: None,
        }]);
        let new_state = Shape::OpenVariant(&[
            Case {
                id: 3,
                name: "Archived",
                payload: None,
            },
            Case {
                id: 1,
                name: "Active",
                payload: None,
            },
        ]);
        let old = Shape::Record {
            core: &[Slot {
                id: "state",
                shape: &old_state,
            }],
            extensions: &[],
        };
        let new = Shape::Record {
            core: &[Slot {
                id: "state",
                shape: &new_state,
            }],
            extensions: &[Extension {
                id: 2,
                name: "note",
                field: ExtensionField::Optional(&TEXT),
            }],
        };
        let old = Shape::List {
            element: &old,
            max: 128,
        };
        let new = Shape::List {
            element: &new,
            max: 128,
        };
        assert_eq!(check(&old, &new), Compatibility::Preserved);
        assert_eq!(
            check(&new, &old),
            Compatibility::Drops(vec!["$[].state<Archived#3>".into(), "$[].note#2".into()])
        );
    }

    #[test]
    fn a_known_open_case_cannot_silently_discard_an_incompatible_body() {
        let u8 = Shape::Scalar(Scalar::U8);
        let bool = Shape::Scalar(Scalar::Bool);
        let left = Shape::List {
            element: &u8,
            max: 16,
        };
        let right = Shape::List {
            element: &bool,
            max: 16,
        };
        let a = Shape::OpenVariant(&[Case {
            id: 1,
            name: "Known",
            payload: Some(&left),
        }]);
        let b = Shape::OpenVariant(&[Case {
            id: 1,
            name: "Known",
            payload: Some(&right),
        }]);
        breaking(check(&a, &b), "$<Known#1>[]");
    }

    #[test]
    fn fixed_slot_order_and_shape_are_part_of_the_contract() {
        let a = Shape::FixedRecord(&[
            Slot {
                id: "first",
                shape: &U32,
            },
            Slot {
                id: "last",
                shape: &U32,
            },
        ]);
        let b = Shape::FixedRecord(&[
            Slot {
                id: "last",
                shape: &U32,
            },
            Slot {
                id: "first",
                shape: &U32,
            },
        ]);
        breaking(check(&a, &b), "$.first");
        let a = Shape::Tuple(&[&U32, &TEXT]);
        let reordered = Shape::Tuple(&[&TEXT, &U32]);
        breaking(check(&a, &reordered), "$.0");
        let optional = Shape::Option(&U32);
        let longer = Shape::Tuple(&[&U32, &TEXT, &optional]);
        breaking(check(&a, &longer), "$");
        breaking(check(&longer, &a), "$");
    }

    #[test]
    fn an_empty_extension_block_changes_the_record_framing() {
        let fixed = Shape::FixedRecord(&CORE);
        let extensible = record(&[]);
        let fixed = Shape::List {
            element: &fixed,
            max: 8,
        };
        let extensible = Shape::List {
            element: &extensible,
            max: 8,
        };
        assert_eq!(check(&fixed, &fixed), Compatibility::Preserved);
        assert_eq!(check(&extensible, &extensible), Compatibility::Preserved);
        for (writer, reader) in [(&fixed, &extensible), (&extensible, &fixed)] {
            assert_eq!(
                check(writer, reader),
                Compatibility::Breaking {
                    path: "$[]".into(),
                    reason: "extension block framing changed",
                },
            );
        }
    }

    #[test]
    fn optional_extension_does_not_satisfy_a_required_extension() {
        let optional_extension = [Extension {
            id: 1,
            name: "epoch",
            field: ExtensionField::Optional(&U32),
        }];
        let required_extension = [Extension {
            id: 1,
            name: "epoch",
            field: ExtensionField::Required(&U32),
        }];
        let optional = record(&optional_extension);
        let required = record(&required_extension);
        breaking(check(&optional, &required), "$.epoch#1");
        breaking(check(&record(&[]), &required), "$.epoch#1");
        assert_eq!(check(&required, &optional), Compatibility::Preserved);
        assert_eq!(
            check(&required, &record(&[])),
            Compatibility::Drops(vec!["$.epoch#1".into()])
        );
    }

    #[test]
    fn method_upgrade_compares_old_requests_and_new_replies() {
        let wide = Shape::Text { max: 100 };
        let narrow = Shape::Text { max: 20 };
        let old = update(&narrow, &wide);
        let new = update(&wide, &narrow);
        assert_eq!(check_upgrade(&old, &new), Compatibility::Preserved);
        breaking(check_upgrade(&new, &old), "$.save.request");
        breaking(
            check_upgrade(&old, &update(&narrow, &Shape::Text { max: 200 })),
            "$.save.response",
        );
        let mut changed = update(&wide, &narrow);
        changed[0].mode = Mode::Query;
        breaking(check_upgrade(&old, &changed), "$.save");
    }

    #[test]
    fn history_keeps_retired_nested_ids_and_detects_a_lost_baseline_marker() {
        let live_extension = [Extension {
            id: 4,
            name: "note",
            field: ExtensionField::Optional(&TEXT),
        }];
        let live = record(&live_extension);
        let retired = record(&[Extension {
            id: 4,
            name: "note",
            field: ExtensionField::Retired,
        }]);
        let dropped = record(&[]);
        let live = Shape::List {
            element: &live,
            max: 8,
        };
        let retired = Shape::List {
            element: &retired,
            max: 8,
        };
        let dropped = Shape::List {
            element: &dropped,
            max: 8,
        };
        let a = update(&live, &U32);
        let b = update(&retired, &U32);
        let missing = update(&dropped, &U32);
        assert_eq!(
            check_history(&[&a], &b),
            Compatibility::Drops(vec!["$.save.request[].note#4".into()])
        );
        breaking(check_history(&[&a, &b], &a), "$.save.request[].note#4");
        breaking(
            check_history(&[&a, &missing], &a),
            "$.save.request[].note#4",
        );
        breaking(
            check_upgrade(&update(&U32, &retired), &update(&U32, &live)),
            "$.save.response[].note#4",
        );
    }

    #[test]
    fn closed_variant_widening_only_works_toward_the_reader() {
        let a = Shape::ClosedVariant(&[Case {
            id: 0,
            name: "Active",
            payload: None,
        }]);
        let b = Shape::ClosedVariant(&[
            Case {
                id: 0,
                name: "Active",
                payload: None,
            },
            Case {
                id: 1,
                name: "Paused",
                payload: None,
            },
        ]);
        assert_eq!(check(&a, &b), Compatibility::Preserved);
        breaking(check(&b, &a), "$<Paused#1>");
    }

    #[test]
    fn recursive_record_shares_shape_and_compares_additions() {
        static A: Shape<'static> = Shape::Record {
            core: &[Slot {
                id: "next",
                shape: &Shape::Option(&A),
            }],
            extensions: &[],
        };
        static B: Shape<'static> = Shape::Record {
            core: &[Slot {
                id: "next",
                shape: &Shape::Option(&B),
            }],
            extensions: &[Extension {
                id: 1,
                name: "note",
                field: ExtensionField::Optional(&TEXT),
            }],
        };
        assert_eq!(check(&A, &B), Compatibility::Preserved);
        assert_eq!(check(&B, &A), Compatibility::Drops(vec!["$.note#1".into()]));
    }

    #[test]
    fn ambiguous_ids_are_invalid_and_unit_payloads_remain_distinct() {
        let duplicate_entries = [
            Extension {
                id: 1,
                name: "new",
                field: ExtensionField::Optional(&U32),
            },
            Extension {
                id: 1,
                name: "old",
                field: ExtensionField::Retired,
            },
        ];
        let duplicate = record(&duplicate_entries);
        breaking(check(&duplicate, &duplicate), "$");
        let empty = Shape::Tuple(&[]);
        let tuple_case = Shape::OpenVariant(&[Case {
            id: 1,
            name: "Unit",
            payload: Some(&empty),
        }]);
        let unit_case = Shape::OpenVariant(&[Case {
            id: 1,
            name: "Unit",
            payload: None,
        }]);
        assert_eq!(check(&tuple_case, &tuple_case), Compatibility::Preserved);
        breaking(check(&tuple_case, &unit_case), "$<Unit#1>");
    }
}
