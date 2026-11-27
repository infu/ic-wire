//! Host-side input for the directional checker.
//!
//! Each directory contains format-1 per-endpoint manifests and `modes.json`,
//! a map from deployed method name to its declared invocation mode. An optional
//! `retired.json` maps endpoint names to named extension records and lists their
//! permanently retired `{ "id": 7, "name": "oldField" }` entries. These
//! tombstones belong to the API baseline and are absent from the wire schema.

use crate::compatibility::{
    self, Case, Compatibility, Extension, ExtensionField, Method, Mode, Scalar, Shape, Slot,
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::Path,
};
use wire::typescript::{__private::Value, SchemaLimits, ValidatedRegistry};

const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_METHODS: usize = 256;
pub(super) const MAX_HISTORY: usize = 32;
const MAX_PROJECTED_SHAPES: usize = 65_536;

type Result<T> = std::result::Result<T, String>;
type Types<'a> = BTreeMap<&'a str, usize>;

pub(super) struct Service {
    documents: BTreeMap<String, Value>,
    modes: Value,
    retired: Value,
}

impl Service {
    pub(super) fn parse(documents: Vec<Value>, modes: Value, retired: Value) -> Result<Self> {
        if documents.len() > MAX_METHODS {
            return Err("too many method manifests".into());
        }
        let mut indexed = BTreeMap::new();
        for document in documents {
            let endpoint = text(&document, "endpoint")?.to_string();
            if indexed.insert(endpoint.clone(), document).is_some() {
                return Err(format!("duplicate manifest for endpoint {endpoint}"));
            }
        }
        let modes_by_endpoint = modes
            .as_object()
            .ok_or("modes.json must be a JSON object")?;
        for endpoint in indexed.keys() {
            let value = modes_by_endpoint.get(endpoint).and_then(Value::as_str);
            mode(value).map_err(|error| format!("{endpoint}: {error}"))?;
        }
        let retired_by_endpoint = retired
            .as_object()
            .ok_or("retired.json must be a JSON object")?;
        for endpoint in modes_by_endpoint.keys().chain(retired_by_endpoint.keys()) {
            if !indexed.contains_key(endpoint) {
                return Err(format!(
                    "sidecar declares endpoint {endpoint} without a manifest"
                ));
            }
        }
        Ok(Self {
            documents: indexed,
            modes,
            retired,
        })
    }

    pub(super) fn load(path: &Path) -> Result<Self> {
        let modes = read_json(&path.join("modes.json"))?;
        let retired_path = path.join("retired.json");
        let retired = if retired_path.exists() {
            read_json(&retired_path)?
        } else {
            Value::Object(Default::default())
        };
        let mut files = Vec::new();
        for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
            let file = entry
                .map_err(|error| format!("{}: {error}", path.display()))?
                .path();
            if file
                .extension()
                .is_some_and(|extension| extension == "json")
                && !file
                    .file_name()
                    .is_some_and(|name| name == "modes.json" || name == "retired.json")
            {
                if files.len() >= MAX_METHODS {
                    return Err(format!("{}: too many method manifests", path.display()));
                }
                files.push(file);
            }
        }
        files.sort_unstable();
        let documents = files
            .iter()
            .map(|path| read_json(path))
            .collect::<Result<_>>()?;
        Self::parse(documents, modes, retired)
            .map_err(|error| format!("{}: {error}", path.display()))
    }
}

fn read_json(path: &Path) -> Result<Value> {
    let read = || -> Result<Value> {
        let mut source = String::new();
        File::open(path)
            .and_then(|file| {
                file.take(MAX_DOCUMENT_BYTES + 1)
                    .read_to_string(&mut source)
            })
            .map_err(|error| error.to_string())?;
        if source.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err("manifest exceeds the input size limit".into());
        }
        source.parse::<Value>().map_err(|error| error.to_string())
    };
    read().map_err(|error| format!("{}: {error}", path.display()))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("missing or invalid {key}"))
}

fn unsigned(value: &Value, key: &str) -> Result<u64> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("missing or invalid {key}"))
}

fn u32_at(value: &Value, key: &str) -> Result<u32> {
    unsigned(value, key)?
        .try_into()
        .map_err(|_| format!("{key} exceeds u32"))
}

fn bound_at(value: &Value, key: &str, default: u32) -> Result<u32> {
    if value.get(key).is_some() {
        u32_at(value, key)
    } else {
        Ok(default)
    }
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value[key]
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("missing or invalid {key}"))
}

fn mode(value: Option<&str>) -> Result<Mode> {
    match value {
        Some("query") => Ok(Mode::Query),
        Some("composite_query") => Ok(Mode::CompositeQuery),
        Some("update") => Ok(Mode::Update),
        _ => Err("expected query, composite_query or update in modes.json".into()),
    }
}

struct RawMethod<'a> {
    name: &'a str,
    id: u64,
    mode: Mode,
    request: usize,
    response: usize,
}

struct RawSlot<'a> {
    name: &'a str,
    node: usize,
}
struct RawExtension<'a> {
    id: u32,
    name: &'a str,
    field: Option<(usize, bool)>,
}
struct RawCase<'a> {
    id: u32,
    name: &'a str,
    node: usize,
}

enum VariantKind {
    Closed,
    Open,
    RequiredOpen,
}

enum Raw<'a> {
    Leaf(Shape<'a>),
    Option(usize),
    Recover(usize),
    Result { ok: usize, err: usize },
    List { item: usize, max: u32 },
    Array { item: usize, len: u32 },
    Tuple(Vec<usize>),
    ExtensionTuple(Vec<usize>, Vec<RawSlot<'a>>),
    FixedRecord(Vec<RawSlot<'a>>),
    Record(Vec<RawSlot<'a>>, Vec<RawExtension<'a>>),
    Variant(Vec<RawCase<'a>>, VariantKind),
}

enum Members<'a> {
    None,
    Tuple(Vec<&'a Shape<'a>>),
    ExtensionTuple(Vec<&'a Shape<'a>>, Vec<Slot<'a>>),
    Slots(Vec<Slot<'a>>),
    Record(Vec<Slot<'a>>, Vec<Extension<'a>>),
    Cases(Vec<Case<'a>>),
}

impl<'a> Raw<'a> {
    fn members(&self, references: &'a [Shape<'a>]) -> Members<'a> {
        let items = |raw: &[usize]| raw.iter().map(|&index| &references[index]).collect();
        let slots = |raw: &[RawSlot<'a>]| {
            raw.iter()
                .map(|slot| Slot {
                    id: slot.name,
                    shape: &references[slot.node],
                })
                .collect()
        };
        match self {
            Self::Tuple(core) => Members::Tuple(items(core)),
            Self::ExtensionTuple(core, tail) => Members::ExtensionTuple(items(core), slots(tail)),
            Self::FixedRecord(fields) => Members::Slots(slots(fields)),
            Self::Record(core, extensions) => Members::Record(
                slots(core),
                extensions
                    .iter()
                    .map(|ext| Extension {
                        id: ext.id,
                        name: ext.name,
                        field: match ext.field {
                            None => ExtensionField::Retired,
                            Some((node, false)) => ExtensionField::Optional(&references[node]),
                            Some((node, true)) => ExtensionField::Required(&references[node]),
                        },
                    })
                    .collect(),
            ),
            Self::Variant(variants, _) => Members::Cases(
                variants
                    .iter()
                    .map(|variant| Case {
                        id: variant.id,
                        name: variant.name,
                        payload: Some(&references[variant.node]),
                    })
                    .collect(),
            ),
            _ => Members::None,
        }
    }

    fn shape(&self, references: &'a [Shape<'a>], members: &'a Members<'a>) -> Shape<'a> {
        match (self, members) {
            (Self::Leaf(shape), Members::None) => *shape,
            (Self::Option(item), Members::None) => Shape::Option(&references[*item]),
            (Self::Recover(item), Members::None) => Shape::Recover(&references[*item]),
            (Self::Result { ok, err }, Members::None) => Shape::Result {
                ok: &references[*ok],
                err: &references[*err],
            },
            (Self::List { item, max }, Members::None) => Shape::List {
                element: &references[*item],
                max: *max,
            },
            (Self::Array { item, len }, Members::None) => Shape::Array {
                element: &references[*item],
                len: *len,
            },
            (Self::Tuple(_), Members::Tuple(items)) => Shape::Tuple(items),
            (Self::ExtensionTuple(_, _), Members::ExtensionTuple(core, tail)) => {
                Shape::ExtensionTuple { core, tail }
            }
            (Self::FixedRecord(_), Members::Slots(slots)) => Shape::FixedRecord(slots),
            (Self::Record(_, _), Members::Record(core, extensions)) => {
                Shape::Record { core, extensions }
            }
            (Self::Variant(_, VariantKind::Open), Members::Cases(cases)) => {
                Shape::OpenVariant(cases)
            }
            (Self::Variant(_, VariantKind::RequiredOpen), Members::Cases(cases)) => {
                Shape::RequiredOpenVariant(cases)
            }
            (Self::Variant(_, VariantKind::Closed), Members::Cases(cases)) => {
                Shape::ClosedVariant(cases)
            }
            _ => unreachable!("projection members follow the raw shape"),
        }
    }
}

struct Projector<'a> {
    nodes: Vec<Raw<'a>>,
}

impl<'a> Projector<'a> {
    fn push(&mut self, raw: Raw<'a>) -> Result<usize> {
        let index = self.nodes.len();
        if index >= MAX_PROJECTED_SHAPES {
            return Err("manifest projection exceeds its shape limit".into());
        }
        self.nodes.push(raw);
        Ok(index)
    }

    fn child(&mut self, value: &'a Value, types: &Types<'a>) -> Result<usize> {
        let raw = self.node(value, types)?;
        self.push(raw)
    }

    fn items(&mut self, value: &'a Value, types: &Types<'a>) -> Result<Vec<usize>> {
        array(value, "items")?
            .iter()
            .map(|item| self.child(item, types))
            .collect()
    }

    fn slots(&mut self, values: &'a [Value], types: &Types<'a>) -> Result<Vec<RawSlot<'a>>> {
        values
            .iter()
            .map(|slot| {
                Ok(RawSlot {
                    name: text(slot, "name")?,
                    node: self.child(&slot["value"], types)?,
                })
            })
            .collect()
    }

    fn node(&mut self, value: &'a Value, types: &Types<'a>) -> Result<Raw<'a>> {
        if value.get("check").is_some() {
            return Err(
                "schema declares a custom check and needs a manual compatibility review".into(),
            );
        }
        let kind = text(value, "k")?;
        let default_max = u32::MAX;
        Ok(match kind {
            "unit" => Raw::Leaf(Shape::Scalar(Scalar::Unit)),
            "bool" => Raw::Leaf(Shape::Scalar(Scalar::Bool)),
            "u" | "i" | "float" => {
                let scalar = scalar(kind, u32_at(value, "bits")?)?;
                Raw::Leaf(Shape::Number {
                    scalar,
                    min: value["min"].as_f64(),
                    max: value["max"].as_f64(),
                })
            }
            "text" => Raw::Leaf(Shape::Text {
                max: bound_at(value, "max", default_max)?,
            }),
            "bytes" => Raw::Leaf(Shape::BoundedBytes {
                min: bound_at(value, "min", 0)?,
                max: bound_at(value, "max", default_max)?,
            }),
            "packed" => Raw::Leaf(Shape::Packed {
                word: text(value, "word")?,
                nonzero: value["nonzero"].as_bool().unwrap_or(false),
                max: bound_at(value, "max", default_max)?,
            }),
            "option" => Raw::Option(self.child(&value["item"], types)?),
            "recover" => Raw::Recover(self.child(&value["item"], types)?),
            "list" => Raw::List {
                item: self.child(&value["item"], types)?,
                max: bound_at(value, "max", default_max)?,
            },
            "array" => Raw::Array {
                item: self.child(&value["item"], types)?,
                len: u32_at(value, "max")?,
            },
            "tuple" => Raw::Tuple(self.items(value, types)?),
            // The registry has already admitted sorted, contiguous optional tail IDs.
            "extension-tuple" => Raw::ExtensionTuple(
                self.items(value, types)?,
                self.slots(array(value, "extensions")?, types)?,
            ),
            "record" => Raw::FixedRecord(self.slots(array(value, "fields")?, types)?),
            "extension-record" => Raw::Record(
                self.slots(array(value, "fields")?, types)?,
                array(value, "extensions")?
                    .iter()
                    .map(|ext| {
                        Ok(RawExtension {
                            id: u32_at(ext, "id")?,
                            name: text(ext, "name")?,
                            field: Some((
                                self.child(&ext["value"], types)?,
                                ext["required"]
                                    .as_bool()
                                    .ok_or("invalid extension required flag")?,
                            )),
                        })
                    })
                    .collect::<Result<_>>()?,
            ),
            "enum" | "open-variant" => Raw::Variant(
                array(value, "variants")?
                    .iter()
                    .map(|case| {
                        Ok(RawCase {
                            id: u32_at(case, "id")?,
                            name: text(case, "name")?,
                            node: self.child(&case["value"], types)?,
                        })
                    })
                    .collect::<Result<_>>()?,
                match (kind, value["required"].as_bool()) {
                    ("enum", _) => VariantKind::Closed,
                    (_, Some(true)) => VariantKind::RequiredOpen,
                    _ => VariantKind::Open,
                },
            ),
            "result" => Raw::Result {
                ok: self.child(&value["ok"], types)?,
                err: self.child(&value["err"], types)?,
            },
            "ref" => {
                let name = text(value, "name")?;
                Raw::Leaf(Shape::Reference(
                    *types
                        .get(name)
                        .ok_or_else(|| format!("undefined schema {name}"))?,
                ))
            }
            "newtype" => Raw::Leaf(Shape::Reference(self.child(&value["item"], types)?)),
            _ => return Err(format!("unsupported manifest kind {kind}")),
        })
    }

    fn method(
        &mut self,
        document: &'a Value,
        mode: Mode,
        retired: Option<&'a Value>,
    ) -> Result<RawMethod<'a>> {
        if unsigned(document, "format")? != 1 {
            return Err("expected a format-1 API manifest".into());
        }
        let name = text(document, "endpoint")?;
        let id = text(document, "id")?
            .parse()
            .map_err(|_| "invalid decimal method ID")?;
        for position in ["request", "response"] {
            let limits = &document[position]["payload_limits"];
            for key in ["bytes", "nodes", "depth"] {
                let cap = unsigned(limits, key)?;
                if cap > ((1_u64 << 53) - 1) || (key != "bytes" && cap == 0) {
                    return Err(format!("invalid {position} payload limit {key}"));
                }
            }
        }
        let definitions = document["schemas"]
            .as_object()
            .ok_or("missing schema definitions")?;
        let admitted = definitions
            .iter()
            .map(|(key, shape)| (key.clone(), shape.clone()))
            .collect();
        ValidatedRegistry::from_metadata(&admitted, SchemaLimits::default())
            .map_err(|error| format!("manifest schema rejected: {error:?}"))?;
        let mut types = Types::new();
        for key in definitions.keys() {
            let index = self.push(Raw::Leaf(Shape::Scalar(Scalar::Unit)))?;
            types.insert(key.as_str(), index);
        }
        for (key, shape) in definitions {
            self.nodes[types[key.as_str()]] = self.node(shape, &types)?;
        }
        if let Some(retired) = retired {
            let named = retired
                .as_object()
                .ok_or("retired.json endpoint must be an object")?;
            for (record, entries) in named {
                let index = *types
                    .get(record.as_str())
                    .ok_or_else(|| format!("retired entry names missing schema {record}"))?;
                let Raw::Record(_, extensions) = &mut self.nodes[index] else {
                    return Err(format!(
                        "retired entries must target a named extension record: {record}"
                    ));
                };
                for entry in entries
                    .as_array()
                    .ok_or("retired extension entries must be an array")?
                {
                    extensions.push(RawExtension {
                        id: u32_at(entry, "id")?,
                        name: text(entry, "name")?,
                        field: None,
                    });
                }
            }
        }
        let root = |position| {
            let name = text(&document[position], "root")?;
            types
                .get(name)
                .copied()
                .ok_or_else(|| format!("undefined root {name}"))
        };
        Ok(RawMethod {
            name,
            id,
            mode,
            request: root("request")?,
            response: root("response")?,
        })
    }

    fn service(&mut self, service: &'a Service) -> Result<Vec<RawMethod<'a>>> {
        service
            .documents
            .iter()
            .map(|(endpoint, document)| {
                let invocation = mode(service.modes[endpoint].as_str())?;
                self.method(document, invocation, service.retired.get(endpoint))
                    .map_err(|error| format!("{endpoint}: {error}"))
            })
            .collect()
    }
}

fn scalar(kind: &str, bits: u32) -> Result<Scalar> {
    Ok(match (kind, bits) {
        ("u", 8) => Scalar::U8,
        ("u", 16) => Scalar::U16,
        ("u", 32) => Scalar::U32,
        ("u", 64) => Scalar::U64,
        ("u", 128) => Scalar::U128,
        ("i", 8) => Scalar::I8,
        ("i", 16) => Scalar::I16,
        ("i", 32) => Scalar::I32,
        ("i", 64) => Scalar::I64,
        ("i", 128) => Scalar::I128,
        ("float", 32) => Scalar::F32,
        ("float", 64) => Scalar::F64,
        _ => return Err(format!("unsupported {kind} scalar width {bits}")),
    })
}

fn limit_changes(old: &Service, new: &Service) -> Result<Option<Compatibility>> {
    for (endpoint, before) in &old.documents {
        let Some(after) = new.documents.get(endpoint) else {
            continue;
        };
        for (position, writer, reader) in [("request", before, after), ("response", after, before)]
        {
            for key in ["bytes", "nodes", "depth"] {
                let cap = |document: &Value| unsigned(&document[position]["payload_limits"], key);
                if cap(writer)? > cap(reader)? {
                    return Ok(Some(Compatibility::Breaking {
                        path: format!("$.{endpoint}.{position}.payload_limits.{key}"),
                        reason: "reader advertised limit is below the writer advertised limit",
                    }));
                }
            }
        }
    }
    Ok(None)
}

fn project_methods<'a>(methods: &[RawMethod<'a>], references: &'a [Shape<'a>]) -> Vec<Method<'a>> {
    methods
        .iter()
        .map(|method| Method {
            name: method.name,
            id: method.id,
            mode: method.mode,
            request: &references[method.request],
            response: &references[method.response],
        })
        .collect()
}

pub(super) fn check(history: &[Service], candidate: &Service) -> Result<Compatibility> {
    if history.len() > MAX_HISTORY {
        return Err("too many baseline directories".into());
    }
    let mut projector = Projector { nodes: Vec::new() };
    let historical = history
        .iter()
        .map(|old| projector.service(old))
        .collect::<Result<Vec<_>>>()?;
    let current = projector.service(candidate)?;
    let references: Vec<_> = (0..projector.nodes.len()).map(Shape::Reference).collect();
    let members: Vec<_> = projector
        .nodes
        .iter()
        .map(|node| node.members(&references))
        .collect();
    let definitions: Vec<_> = projector
        .nodes
        .iter()
        .zip(&members)
        .map(|(node, members)| node.shape(&references, members))
        .collect();
    let previous: Vec<_> = historical
        .iter()
        .map(|methods| project_methods(methods, &references))
        .collect();
    let previous: Vec<_> = previous.iter().map(Vec::as_slice).collect();
    let result = compatibility::check_projected_history(
        &previous,
        &project_methods(&current, &references),
        &definitions,
    );
    if matches!(result, Compatibility::Preserved | Compatibility::Drops(_)) {
        for old in history {
            if let Some(change) = limit_changes(old, candidate)? {
                return Ok(change);
            }
        }
    }
    Ok(result)
}
