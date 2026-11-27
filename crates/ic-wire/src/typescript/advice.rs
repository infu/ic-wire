//! Rooted advice over the admitted graph. Reference contracts: docs/design/parity.md.
use super::{BulkAdvisory, BulkSite, GenerationError, Node, ValidatedRegistry};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec::Vec,
};
use core::fmt;

// Keep the width-specific type name and the selection rule in the same diagnostic.
macro_rules! integer_list {
    ($word:literal, $one_byte:expr) => {
        Some((
            $word,
            if $one_byte {
                "Keep the ordinary list for payload size. Each value within the declared scalar bounds uses one varint byte. Measure packed traversal separately if instructions matter, and preserve the scalar and element bounds."
            } else {
                concat!(
                    "Start with an ordinary list for mostly small values. Try Packed<",
                    $word,
                    ", N> for wide values or an IC instruction bottleneck. ",
                    "Compare message sizes on typical and worst-case inputs, preserve the field bound, ",
                    "and choose one layout for the schema."
                )
            },
        ))
    };
}

/// A primitive-list use site requiring an explicit representation review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BulkWarning {
    /// Stable advisory kind for policy matching.
    pub advisory: BulkAdvisory,
    /// Declared sites known from named metadata. Anonymous layouts have no nominal identity.
    pub sites: Vec<BulkSite>,
    /// JSON-pointer path rooted in a public consumer label.
    pub path: String,
    /// Maximum element count declared at this list use site.
    pub max_elements: Option<u64>,
    /// Suggested representation or resource policy; never an automatic rewrite.
    pub representation: &'static str,
}
impl fmt::Display for BulkWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)?;
        if let Some(max) = self.max_elements {
            write!(f, " (max {max} elements)")?;
        }
        write!(f, " [{}]: {}", self.advisory.as_str(), self.representation)
    }
}
/// Complete bounded advice. Warnings never change generated source or wire bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use]
pub struct BulkLintReport {
    /// Candidates without an exact acknowledgement.
    pub warnings: Vec<BulkWarning>,
    /// Acknowledgements that no longer identify a candidate.
    pub stale_acknowledgements: Vec<String>,
}
impl BulkLintReport {
    /// True only when every candidate is acknowledged and no stale path remains.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty() && self.stale_acknowledgements.is_empty()
    }
}
impl fmt::Display for BulkLintReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut shown = 0;
        for warning in self.warnings.iter().take(8) {
            writeln!(f, "warning: {warning}")?;
            shown += 1;
        }
        for path in self.stale_acknowledgements.iter().take(8 - shown) {
            writeln!(f, "warning: {path}: stale acknowledgement")?;
            shown += 1;
        }
        let omitted = self.warnings.len() + self.stale_acknowledgements.len() - shown;
        if omitted > 0 {
            writeln!(
                f,
                "warning: {omitted} more bulk diagnostics; inspect BulkLintReport for all paths"
            )?;
        }
        Ok(())
    }
}

pub(super) fn escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}
impl ValidatedRegistry {
    /// Review public labels mapped to admitted type names, independent of generation.
    /// References retain use-site paths; recursion stops only on the active path.
    /// Exact acknowledgements need nonempty reasons. Bounds and domain checks
    /// remain consumer-owned; no wildcard suppression or representation rewriting.
    ///
    /// # Errors
    /// Schema for unknown roots or empty reasons; Complexity for depth >=64,
    /// more than 65,536 visits/root or 1,000,000 visits/report, paths over 4 KiB
    /// or retained diagnostic path and declaration text over 16 MiB. No partial report is returned.
    pub fn lint_bulk_roots(
        &self,
        roots: &BTreeMap<String, String>,
        acknowledgements: &BTreeMap<String, String>,
    ) -> Result<BulkLintReport, GenerationError> {
        let roots = roots.iter().map(|(label, name)| {
            if label.len() > 4095 {
                return Err(GenerationError::Complexity);
            }
            Ok((format!("/{}", escape(label)), self.root_index(name)?))
        });
        self.lint_roots(roots, acknowledgements)
    }

    pub(super) fn root_index(&self, name: &str) -> Result<usize, GenerationError> {
        self.names
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .map_err(|_| GenerationError::Schema)
    }

    pub(super) fn lint_roots(
        &self,
        roots: impl IntoIterator<Item = Result<(String, usize), GenerationError>>,
        acks: &BTreeMap<String, String>,
    ) -> Result<BulkLintReport, GenerationError> {
        if acks.len() > 1_000_000 {
            return Err(GenerationError::Complexity);
        }
        if acks.values().any(|reason| reason.trim().is_empty()) {
            return Err(GenerationError::Schema);
        }
        let mut walk = Walk {
            graph: self,
            active: BTreeSet::new(),
            remaining: 1_000_000,
            root_remaining: 65_536,
            path: String::new(),
            bytes: 16 * 1024 * 1024,
            warnings: Vec::new(),
        };
        for entry in roots {
            let (path, root) = entry?;
            walk.root_remaining = 65_536;
            walk.path = path;
            // Model the public named root as a reference, just like nested refs.
            walk.charge(0)?;
            walk.active.insert(root);
            walk.visit(self.roots[root], 1, Some(root), None)?;
            walk.active.remove(&root);
        }
        let paths: BTreeSet<_> = walk
            .warnings
            .iter()
            .map(|warning| warning.path.as_str())
            .collect();
        let mut stale = Vec::new();
        for path in acks.keys().filter(|path| !paths.contains(path.as_str())) {
            if path.len() > 4096 {
                return Err(GenerationError::Complexity);
            }
            walk.bytes = walk
                .bytes
                .checked_sub(path.len())
                .ok_or(GenerationError::Complexity)?;
            stale.push(path.clone());
        }
        walk.warnings
            .retain(|warning| !acks.contains_key(&warning.path));
        Ok(BulkLintReport {
            warnings: walk.warnings,
            stale_acknowledgements: stale,
        })
    }

    fn representation(
        &self,
        mut id: usize,
        max_elements: Option<u64>,
    ) -> Result<Option<(&'static str, &'static str)>, GenerationError> {
        let mut hash = None;
        for _ in 0..64 {
            match &self.nodes[id] {
                Node::Ref(root) => id = self.roots[*root],
                Node::Alias(item) => id = *item,
                Node::Array { item, len } if hash.is_none() && [16, 32].contains(len) => {
                    hash = Some(*len);
                    id = *item;
                }
                node => {
                    if let Some(width) = hash {
                        if !matches!(
                            node,
                            Node::Integer {
                                bits: 8,
                                signed: false,
                                ..
                            }
                        ) {
                            return Ok(None);
                        }
                        let word = if width == 16 { "hash16" } else { "hash32" };
                        if max_elements.is_some_and(|max| max <= 127 / width) {
                            return Ok(Some((
                                word,
                                "At this bound both length prefixes use one byte. Packing keeps the same payload size and reduces traversal. Preserve the hash width and element bound.",
                            )));
                        }
                        return Ok(Some(if width == 16 {
                            (
                                "hash16",
                                "Try Packed<[u8; 16], N> to reduce traversal. Its byte-count prefix can be larger than the ordinary hash-count prefix. Preserve the element bound.",
                            )
                        } else {
                            (
                                "hash32",
                                "Try Packed<[u8; 32], N> to reduce traversal. Its byte-count prefix can be larger than the ordinary hash-count prefix. Preserve the element bound.",
                            )
                        }));
                    }
                    let one_byte = |signed: bool| {
                        let Some(bounds) =
                            self.annotations.get(&id).and_then(|a| a.bounds.as_ref())
                        else {
                            return false;
                        };
                        let min = bounds.min.as_ref().and_then(|n| n.as_f64());
                        let max = bounds.max.as_ref().and_then(|n| n.as_f64());
                        if signed {
                            min.is_some_and(|n| n >= -64.0) && max.is_some_and(|n| n <= 63.0)
                        } else {
                            max.is_some_and(|n| n <= 127.0)
                        }
                    };
                    return Ok(match node {
                        Node::Integer {
                            bits: 8,
                            signed: false,
                            ..
                        } => Some((
                            "u8",
                            "For opaque bytes use Bytes<N> for a field bound, ByteBuf for owned bytes, or serde_bytes on an existing byte field.",
                        )),
                        Node::Integer {
                            bits: 8,
                            signed: true,
                            ..
                        } => Some((
                            "i8",
                            "Use Packed<i8, N> for bulk signed bytes when element traversal costs matter. Preserve the element bound.",
                        )),
                        Node::Integer {
                            bits: 16,
                            signed: false,
                            ..
                        } => integer_list!("u16", one_byte(false)),
                        Node::Integer {
                            bits: 16,
                            signed: true,
                            ..
                        } => integer_list!("i16", one_byte(true)),
                        Node::Integer {
                            bits: 32,
                            signed: false,
                            ..
                        } => integer_list!("u32", one_byte(false)),
                        Node::Integer {
                            bits: 32,
                            signed: true,
                            ..
                        } => integer_list!("i32", one_byte(true)),
                        Node::Integer {
                            bits: 64,
                            signed: false,
                            ..
                        } => integer_list!("u64", one_byte(false)),
                        Node::Integer {
                            bits: 64,
                            signed: true,
                            ..
                        } => integer_list!("i64", one_byte(true)),
                        Node::Integer {
                            bits: 128,
                            signed: false,
                            ..
                        } => Some((
                            "u128",
                            "bounded u128 lists and workload-specific node limits; Packed<u128> is unsupported",
                        )),
                        Node::Integer {
                            bits: 128,
                            signed: true,
                            ..
                        } => Some((
                            "i128",
                            "bounded i128 lists and workload-specific node limits; Packed<i128> is unsupported",
                        )),
                        Node::Bool if matches!(max_elements, Some(1 | 2)) => Some((
                            "bool",
                            "Keep the ordinary list for payload size. At this bound a flag count plus a length-prefixed bitmap cannot save bytes.",
                        )),
                        Node::Bool => Some((
                            "bool",
                            "Ordinary lists and serde_packed::bools use one byte per flag. Choose the adapter for bulk traversal. For smaller payloads, compare a bounded flag count plus bitmap bytes at typical list lengths.",
                        )),
                        Node::Float { bits: 32, .. } => Some((
                            "f32",
                            "Keep the ordinary list if NaN or infinity is valid. For finite-only fields, serde_packed::finite_f32 enables bulk traversal at the same payload size through 31 values. At 32 values its prefix costs one extra byte. Preserve the field bound.",
                        )),
                        Node::Float { bits: 64, .. } => Some((
                            "f64",
                            "Keep the ordinary list if NaN or infinity is valid. For finite-only fields, serde_packed::finite_f64 enables bulk traversal at the same payload size through 15 values. At 16 values its prefix costs one extra byte. Preserve the field bound.",
                        )),
                        Node::Unit => Some(("unit", ZERO)),
                        Node::Record(fields) if fields.is_empty() => Some(("empty-record", ZERO)),
                        Node::Tuple(items) if items.is_empty() => Some(("empty-tuple", ZERO)),
                        Node::Array { len: 0, .. } => Some(("empty-array", ZERO)),
                        _ => None,
                    });
                }
            }
        }
        Err(GenerationError::Complexity)
    }
}
const ZERO: &str =
    "an explicit element bound and node limit; zero-byte elements still require traversal";
struct Walk<'a> {
    graph: &'a ValidatedRegistry,
    active: BTreeSet<usize>,
    remaining: usize,
    root_remaining: usize,
    path: String,
    bytes: usize,
    warnings: Vec<BulkWarning>,
}
impl Walk<'_> {
    fn charge(&mut self, depth: usize) -> Result<(), GenerationError> {
        if depth >= 64 || self.path.len() > 4096 {
            return Err(GenerationError::Complexity);
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(GenerationError::Complexity)?;
        self.root_remaining = self
            .root_remaining
            .checked_sub(1)
            .ok_or(GenerationError::Complexity)?;
        Ok(())
    }

    fn child(
        &mut self,
        item: usize,
        suffix: &str,
        depth: usize,
        field: Option<(usize, &str)>,
    ) -> Result<(), GenerationError> {
        let length = self.path.len();
        if length + suffix.len() > 4096 {
            return Err(GenerationError::Complexity);
        }
        self.path.push_str(suffix);
        let result = self.visit(item, depth + 1, None, field);
        self.path.truncate(length);
        result
    }

    fn warn(
        &mut self,
        item: usize,
        max_elements: Option<u64>,
        field: Option<(usize, &str)>,
        word: &str,
        representation: &'static str,
    ) -> Result<(), GenerationError> {
        let length = self.path.len() + 6 + word.len();
        if length > 4096 {
            return Err(GenerationError::Complexity);
        }
        self.bytes = self
            .bytes
            .checked_sub(length)
            .ok_or(GenerationError::Complexity)?;
        let mut sites = Vec::new();
        if let Node::Ref(root) = self.graph.nodes[item] {
            let label = &self.graph.names[root];
            self.bytes = self
                .bytes
                .checked_sub(label.len())
                .ok_or(GenerationError::Complexity)?;
            sites.push(BulkSite::ListElement(label.clone()));
        }
        if let Some((record, name)) = field {
            let label = &self.graph.names[record];
            self.bytes = self
                .bytes
                .checked_sub(label.len() + name.len())
                .ok_or(GenerationError::Complexity)?;
            sites.push(BulkSite::Field(label.clone(), name.into()));
        }
        self.warnings.push(BulkWarning {
            advisory: match word {
                "u8" => BulkAdvisory::OpaqueBytes,
                "unit" | "empty-record" | "empty-tuple" | "empty-array" => {
                    BulkAdvisory::ZeroByteList
                }
                "bool" => BulkAdvisory::BooleanList,
                "i128" | "u128" => BulkAdvisory::WideIntegerList,
                _ => BulkAdvisory::PackedList,
            },
            sites,
            path: format!("{}/list/{word}", self.path),
            max_elements,
            representation,
        });
        Ok(())
    }

    fn visit(
        &mut self,
        id: usize,
        depth: usize,
        owner: Option<usize>,
        field: Option<(usize, &str)>,
    ) -> Result<(), GenerationError> {
        self.charge(depth)?;
        match &self.graph.nodes[id] {
            Node::Ref(root) => {
                if self.active.insert(*root) {
                    self.visit(self.graph.roots[*root], depth + 1, Some(*root), field)?;
                    self.active.remove(root);
                }
            }
            Node::List { max: Some(0), .. } | Node::Array { len: 0, .. } => {}
            Node::List { item, max } => {
                if let Some((word, representation)) = self.graph.representation(*item, *max)? {
                    self.warn(*item, *max, field, word, representation)?;
                }
                self.child(*item, "/item", depth, None)?;
            }
            Node::Array { item, .. }
            | Node::Option(item)
            | Node::Recover(item)
            | Node::Alias(item) => self.child(*item, "/item", depth, None)?,
            Node::Record(fields) | Node::ExtensionRecord { fields, .. } => {
                for (name, item) in fields {
                    self.child(
                        *item,
                        &format!("/fields/{}", escape(name)),
                        depth,
                        owner.map(|record| (record, name.as_str())),
                    )?;
                }
                if let Node::ExtensionRecord { extensions, .. } = &self.graph.nodes[id] {
                    for entry in extensions {
                        self.child(
                            entry.child,
                            &format!("/extensions/{}", escape(&entry.name)),
                            depth,
                            owner.map(|record| (record, entry.name.as_str())),
                        )?;
                    }
                }
            }
            Node::Enum(variants) | Node::OpenVariant { variants, .. } => {
                for (name, _, item) in variants {
                    self.child(*item, &format!("/variants/{}", escape(name)), depth, None)?;
                }
            }
            Node::Tuple(items) | Node::ExtensionTuple { items, .. } => {
                for (index, item) in items.iter().enumerate() {
                    self.child(*item, &format!("/items/{index}"), depth, None)?;
                }
                if let Node::ExtensionTuple { extensions, .. } = &self.graph.nodes[id] {
                    for entry in extensions {
                        self.child(
                            entry.child,
                            &format!("/items/{}", entry.id - 1),
                            depth,
                            None,
                        )?;
                    }
                }
            }
            Node::Result { ok, err } => {
                self.child(*ok, "/ok", depth, None)?;
                self.child(*err, "/err", depth, None)?;
            }
            _ => {}
        }
        Ok(())
    }
}
