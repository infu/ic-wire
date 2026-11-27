#[cfg(feature = "service-client")]
mod service;

use super::{
    ExtensionField, GenerationError, MAX_SAFE_INTEGER, Method, Node, PayloadLimits,
    RUNTIME_SCHEMA_FORMAT, ValidatedRegistry, identifier,
};
use crate::framing::HEADER_BYTES;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    vec::Vec,
};
use core::fmt::{self, Write};

///
/// Output
/// Charges each UTF-8 write against the remaining generation budget before appending it.
///

struct Output {
    text: String,
    remaining: usize,
}

impl Output {
    fn new(remaining: usize) -> Self {
        Self {
            text: String::new(),
            remaining,
        }
    }

    fn scalar_bounds(&mut self, bounds: &super::ScalarBounds, json: bool) -> fmt::Result {
        for (key, bound) in [("min", &bounds.min), ("max", &bounds.max)] {
            if let Some(bound) = bound {
                if json {
                    write!(self, ",\"{key}\":{bound}")?;
                } else {
                    write!(self, "/* {key} {bound} */ ")?;
                }
            }
        }

        Ok(())
    }

    fn quoted(&mut self, value: &str) -> fmt::Result {
        self.write_char('"')?;
        let mut start = 0;
        for (index, character) in value.char_indices() {
            if !matches!(
                character,
                '"' | '\\' | '\u{0000}'..='\u{001f}' | '\u{2028}' | '\u{2029}'
            ) {
                continue;
            }
            // Admit and append ordinary UTF-8 spans together. Only escape
            // boundaries need individual writes; byte accounting stays exact.
            self.write_str(&value[start..index])?;
            match character {
                '"' => self.write_str("\\\"")?,
                '\\' => self.write_str("\\\\")?,
                '\u{0000}'..='\u{001f}' | '\u{2028}' | '\u{2029}' => {
                    write!(self, "\\u{:04x}", u32::from(character))?;
                }
                // The guard above passes only the three escape classes in this match.
                _ => unreachable!(),
            }
            start = index + character.len_utf8();
        }
        self.write_str(&value[start..])?;

        self.write_char('"')
    }

    fn bounds(&mut self, min: Option<u64>, max: Option<u64>) -> fmt::Result {
        if let Some(min) = min {
            write!(self, "/* min {min} */ ")?;
        }
        if let Some(max) = max {
            write!(self, "/* max {max} */ ")?;
        }

        Ok(())
    }

    fn json_bound(&mut self, name: &str, value: Option<u64>) -> fmt::Result {
        if let Some(value) = value {
            write!(self, ",\"{name}\":{value}")?;
        }

        Ok(())
    }
}

impl Write for Output {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.remaining = self.remaining.checked_sub(value.len()).ok_or(fmt::Error)?;
        self.text.push_str(value);

        Ok(())
    }
}

impl ValidatedRegistry {
    /// Generate a typed payload client with privately owned metadata and policy.
    ///
    /// `root` must resolve to an extension record or tuple. `export` names the
    /// exported codec object.
    /// The trusted runtime module must export `schemaFormat` matching
    /// [`RUNTIME_SCHEMA_FORMAT`] and `createPayloadCodec(metadata, registry)`.
    /// The factory returns `encode(unknown): Uint8Array` and `decode(Uint8Array):
    /// unknown`, using the exported JSON grammar. Qualification executes the bundled
    /// runtime against independent literal bytes and Rust decode results.
    /// Runtime code is not bundled, downloaded or selected by this function.
    /// Bulk advice always runs for `/types/ROOT`. The required handler runs once
    /// after successful rendering and linting, including a clean report. It should
    /// display diagnostics in the consumer's build output or enforce `is_clean()`.
    /// Acknowledgements never change generated source.
    ///
    /// # Errors
    /// Returns Schema for invalid names or message layouts, empty module
    /// specifiers, or zero byte, node or depth allowances. Returns Complexity
    /// for unsafe JS integers or excess output. Rooted advice can also return
    /// the errors documented by [`Self::lint_bulk_roots`].
    pub fn payload_module(
        &self,
        export: &str,
        root: &str,
        runtime_module: &str,
        limits: PayloadLimits,
        acknowledgements: &BTreeMap<String, String>,
        diagnostics: impl FnOnce(super::BulkLintReport),
    ) -> Result<String, GenerationError> {
        Self::admit_runtime_module(runtime_module)?;
        let root_index = self.admit_client(export, root, limits)?;
        self.validate_message_roots(&[root_index])?;
        let mut output = Output::new(self.output_bytes);
        self.render_payload_module(&mut output, export, root, runtime_module, limits)
            .map_err(|_| GenerationError::Complexity)?;
        let report = self.lint_roots(
            [Ok((
                format!("/types/{}", super::advice::escape(root)),
                root_index,
            ))],
            acknowledgements,
        )?;
        diagnostics(report);

        Ok(output.text)
    }

    fn admit_runtime_module(module: &str) -> Result<(), GenerationError> {
        if module.is_empty() || module.contains('\0') {
            return Err(GenerationError::Schema);
        }

        Ok(())
    }

    fn admit_client(
        &self,
        export: &str,
        root: &str,
        limits: PayloadLimits,
    ) -> Result<usize, GenerationError> {
        identifier(export)?;
        let root = self.root_index(root)?;
        if export == "globalThis"
            || self
                .names
                .binary_search_by(|name| name.as_str().cmp(export))
                .is_ok()
            || limits.bytes == 0
            || limits.nodes == 0
            || limits.depth == 0
        {
            return Err(GenerationError::Schema);
        }
        if [limits.bytes, limits.nodes, limits.depth]
            .into_iter()
            .any(|n| n > MAX_SAFE_INTEGER)
        {
            return Err(GenerationError::Complexity);
        }

        Ok(root)
    }

    /// Generate typed framed codecs with independent request/response policy.
    ///
    /// The runtime must export `schemaFormat` and `createCodec` with the
    /// bundled framed-codec interface. IDs are emitted as decimal strings,
    /// preserving all u64 values without JavaScript Number rounding. Duplicate
    /// IDs or exports reject before rendering. This emits no routing/execution.
    /// Both message roots must resolve to an extension record or tuple.
    /// Bulk advice always reviews each `/methods/EXPORT/request` and `/response`
    /// independently. The required handler runs once on success, even when clean.
    ///
    /// # Errors
    /// Returns Schema for invalid names, roots, policies or duplicate IDs;
    /// Complexity for unsafe header-inclusive byte allowances or excess output,
    /// and the rooted-advice errors documented by [`Self::lint_bulk_roots`].
    pub fn method_module(
        &self,
        methods: &[Method<'_>],
        runtime_module: &str,
        acknowledgements: &BTreeMap<String, String>,
        diagnostics: impl FnOnce(super::BulkLintReport),
    ) -> Result<String, GenerationError> {
        if methods.is_empty() {
            return Err(GenerationError::Schema);
        }
        // Each method necessarily emits at least one byte; bound admission work
        // before allocating duplicate-detection sets for caller-owned input.
        if methods.len() > self.output_bytes {
            return Err(GenerationError::Complexity);
        }
        Self::admit_runtime_module(runtime_module)?;
        self.admit_methods(methods.iter())?;
        let mut output = Output::new(self.output_bytes);
        self.render_methods(&mut output, methods, runtime_module)
            .map_err(|_| GenerationError::Complexity)?;
        let roots = methods.iter().flat_map(|method| {
            [("request", method.request), ("response", method.response)].map(|(direction, root)| {
                Ok((
                    format!(
                        "/methods/{}/{direction}",
                        super::advice::escape(method.export)
                    ),
                    self.root_index(root)?,
                ))
            })
        });
        let report = self.lint_roots(roots, acknowledgements)?;
        diagnostics(report);

        Ok(output.text)
    }

    fn admit_methods<'a>(
        &self,
        methods: impl Iterator<Item = &'a Method<'a>>,
    ) -> Result<(), GenerationError> {
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut roots = Vec::new();
        for method in methods {
            for (root, limits) in [
                (method.request, method.request_limits),
                (method.response, method.response_limits),
            ] {
                roots.push(self.admit_client(method.export, root, limits)?);
                if limits.bytes > MAX_SAFE_INTEGER - HEADER_BYTES as u64 {
                    return Err(GenerationError::Complexity);
                }
            }
            if !names.insert(method.export) || !ids.insert(method.id) {
                return Err(GenerationError::Schema);
            }
        }

        self.validate_message_roots(&roots)
    }

    fn render_methods(
        &self,
        output: &mut Output,
        methods: &[Method<'_>],
        runtime_module: &str,
    ) -> fmt::Result {
        self.module_prelude(output, runtime_module, "createCodec")?;
        for method in methods {
            writeln!(output, "export const {} = (() => {{", method.export)?;
            write!(
                output,
                "const $codec = $create({{id:'{}',request:{{k:'ref',name:",
                method.id
            )?;
            output.quoted(method.request)?;
            output.write_str("},response:{k:'ref',name:")?;
            output.quoted(method.response)?;
            for (prefix, limits) in [
                ("request", method.request_limits),
                ("response", method.response_limits),
            ] {
                write!(
                    output,
                    "{separator}{prefix}Bytes:{}, {prefix}Nodes:{}, {prefix}Depth:{}",
                    limits.bytes + HEADER_BYTES as u64,
                    limits.nodes,
                    limits.depth,
                    separator = if prefix == "request" { "}," } else { "," }
                )?;
            }
            output.write_str("}, $types);\nreturn globalThis.Object.freeze({\n")?;
            for (direction, root) in [("Request", method.request), ("Response", method.response)] {
                writeln!(
                    output,
                    "encode{direction}:(value: {root}): globalThis.Uint8Array => $codec.encode{direction}(value), decode{direction}:(bytes: globalThis.Uint8Array): {root} => $codec.decode{direction}(bytes) as {root},"
                )?;
            }
            output.write_str("});\n})();\n")?;
        }
        Ok(())
    }

    fn render_payload_module(
        &self,
        output: &mut Output,
        export: &str,
        root: &str,
        runtime_module: &str,
        limits: PayloadLimits,
    ) -> fmt::Result {
        self.module_prelude(output, runtime_module, "createPayloadCodec")?;
        writeln!(output, "export const {export} = (() => {{")?;
        output.write_str("const $codec = $create({schema:{k:'ref',name:")?;
        output.quoted(root)?;
        writeln!(
            output,
            "}},bytes:{},nodes:{},depth:{}}}, $types);",
            limits.bytes, limits.nodes, limits.depth
        )?;
        writeln!(
            output,
            "return globalThis.Object.freeze({{encode:(value: {root}): globalThis.Uint8Array => $codec.encode(value), decode:(bytes: globalThis.Uint8Array): {root} => $codec.decode(bytes) as {root}}});\n}})();"
        )
    }

    fn module_prelude(
        &self,
        output: &mut Output,
        runtime_module: &str,
        factory: &str,
    ) -> fmt::Result {
        write!(
            output,
            "// Generated IC Wire client; no transport or execution.\nimport {{ schemaFormat as $format, {factory} as $create }} from "
        )?;
        output.quoted(runtime_module)?;
        output.write_str(";\nif ($format !== ")?;
        output.quoted(RUNTIME_SCHEMA_FORMAT)?;
        output.write_str(") throw new globalThis.Error('IC Wire runtime format mismatch');\n")?;
        self.render(output)?;
        output.write_str("const $types = globalThis.JSON.parse(")?;
        // A bounded temporary JSON string permits correct JS string escaping,
        // including __proto__ keys, without another schema interpretation path.
        let metadata = self.metadata_json().map_err(|_| fmt::Error)?;
        output.quoted(&metadata)?;
        output.write_str(");\n")
    }

    /// Export the admitted subset in the JSON registry grammar.
    ///
    /// This is data for a trusted metadata consumer, not executable JavaScript.
    /// Parse it as JSON: embedding it as an object literal changes the meaning
    /// of `__proto__` keys. Supplied names and use-site constraints are preserved.
    /// No runtime version or payload policy is implied by this registry alone.
    ///
    /// # Errors
    /// Returns Complexity before extending output beyond its byte allowance.
    pub fn metadata_json(&self) -> Result<String, GenerationError> {
        let mut output = Output::new(self.output_bytes);
        self.render_metadata(&mut output)
            .map_err(|_| GenerationError::Complexity)?;
        Ok(output.text)
    }

    fn render_metadata(&self, output: &mut Output) -> fmt::Result {
        output.write_char('{')?;
        for (index, (name, root)) in self.names.iter().zip(&self.roots).enumerate() {
            if index != 0 {
                output.write_char(',')?;
            }
            output.quoted(name)?;
            output.write_char(':')?;
            self.metadata_node(*root, output, &mut |_| {}, "")?;
        }
        output.write_char('}')
    }

    fn metadata_node(
        &self,
        id: usize,
        output: &mut Output,
        reference: &mut impl FnMut(usize),
        entry_break: &str,
    ) -> fmt::Result {
        output.write_str("{\"k\":")?;
        match &self.nodes[id] {
            Node::Unit => output.quoted("unit")?,
            Node::Bool => output.quoted("bool")?,
            Node::Float { bits } => {
                output.quoted("float")?;
                write!(output, ",\"bits\":{bits}")?;
            }
            Node::Integer { bits, signed } => {
                output.quoted(if *signed { "i" } else { "u" })?;
                write!(output, ",\"bits\":{bits}")?;
            }
            Node::Text { max } => {
                output.quoted("text")?;
                output.json_bound("max", *max)?;
            }
            Node::Bytes { min, max } => {
                output.quoted("bytes")?;
                output.json_bound("min", *min)?;
                output.json_bound("max", *max)?;
            }
            Node::Packed { word, nonzero, max } => {
                output.quoted("packed")?;
                output.json_bound("max", *max)?;
                output.write_str(",\"word\":")?;
                output.quoted(word)?;
                if *nonzero {
                    output.write_str(",\"nonzero\":true")?;
                }
            }
            Node::List { item, max } => {
                output.quoted("list")?;
                output.json_bound("max", *max)?;
                output.write_str(",\"item\":")?;
                self.metadata_node(*item, output, reference, entry_break)?;
            }
            Node::Array { item, len } => {
                output.quoted("array")?;
                output.json_bound("max", Some(*len))?;
                output.write_str(",\"item\":")?;
                self.metadata_node(*item, output, reference, entry_break)?;
            }
            Node::Tuple(items) | Node::ExtensionTuple { items, .. } => {
                let extensions = match &self.nodes[id] {
                    Node::ExtensionTuple { extensions, .. } => Some(extensions),
                    _ => None,
                };
                output.quoted(if extensions.is_some() {
                    "extension-tuple"
                } else {
                    "tuple"
                })?;
                output.write_str(",\"items\":[")?;
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        output.write_char(',')?;
                    }
                    self.metadata_node(*item, output, reference, entry_break)?;
                }
                output.write_char(']')?;
                if let Some(extensions) = extensions {
                    self.metadata_extensions(extensions, output, reference, entry_break)?;
                }
            }
            Node::Result { ok, err } => {
                output.write_str("\"result\",\"ok\":")?;
                self.metadata_node(*ok, output, reference, entry_break)?;
                output.write_str(",\"err\":")?;
                self.metadata_node(*err, output, reference, entry_break)?;
            }
            Node::Option(item) | Node::Recover(item) | Node::Alias(item) => {
                output.quoted(match self.nodes[id] {
                    Node::Option(_) => "option",
                    Node::Recover(_) => "recover",
                    _ => "newtype",
                })?;
                output.write_str(",\"item\":")?;
                self.metadata_node(*item, output, reference, entry_break)?;
            }
            Node::Ref(root) => {
                reference(*root);
                output.write_str("\"ref\",\"name\":")?;
                output.quoted(&self.names[*root])?;
            }
            Node::Record(fields) | Node::ExtensionRecord { fields, .. } => {
                let extensions = match &self.nodes[id] {
                    Node::ExtensionRecord { extensions, .. } => Some(extensions),
                    _ => None,
                };
                output.quoted(if extensions.is_some() {
                    "extension-record"
                } else {
                    "record"
                })?;
                output.write_str(",\"fields\":[")?;
                for (index, (name, child)) in fields.iter().enumerate() {
                    if index != 0 {
                        output.write_char(',')?;
                    }
                    output.write_str(entry_break)?;
                    output.write_str("{\"name\":")?;
                    output.quoted(name)?;
                    output.write_str(",\"value\":")?;
                    self.metadata_node(*child, output, reference, entry_break)?;
                    output.write_char('}')?;
                }
                output.write_char(']')?;
                if let Some(extensions) = extensions {
                    self.metadata_extensions(extensions, output, reference, entry_break)?;
                }
            }
            Node::Enum(variants) | Node::OpenVariant { variants, .. } => {
                if let Node::OpenVariant { required, .. } = &self.nodes[id] {
                    output.quoted("open-variant")?;
                    if *required {
                        output.write_str(",\"required\":true")?;
                    }
                } else {
                    output.quoted("enum")?;
                }
                output.write_str(",\"variants\":[")?;
                for (index, (name, tag, child)) in variants.iter().enumerate() {
                    if index != 0 {
                        output.write_char(',')?;
                    }
                    output.write_str(entry_break)?;
                    output.write_str("{\"name\":")?;
                    output.quoted(name)?;
                    write!(output, ",\"id\":{tag},\"value\":")?;
                    self.metadata_node(*child, output, reference, entry_break)?;
                    output.write_char('}')?;
                }
                output.write_char(']')?;
            }
        }
        if let Some(annotation) = self.annotations.get(&id) {
            if annotation.fixed {
                output.write_str(",\"fixed\":true")?;
            }
            if annotation.projected {
                output.write_str(",\"projected\":true")?;
            }
            if let Some(bounds) = &annotation.bounds {
                output.scalar_bounds(bounds, true)?;
            }
            if let Some(check) = &annotation.check {
                output.write_str(",\"check\":")?;
                output.quoted(check)?;
            }
        }
        output.write_char('}')
    }

    fn metadata_extensions(
        &self,
        extensions: &[ExtensionField],
        output: &mut Output,
        reference: &mut impl FnMut(usize),
        entry_break: &str,
    ) -> fmt::Result {
        output.write_str(",\"extensions\":[")?;
        for (index, entry) in extensions.iter().enumerate() {
            if index != 0 {
                output.write_char(',')?;
            }
            output.write_str(entry_break)?;
            output.write_str("{\"name\":")?;
            output.quoted(&entry.name)?;
            write!(
                output,
                ",\"id\":{},\"required\":{},\"value\":",
                entry.id, entry.required
            )?;
            self.metadata_node(entry.child, output, reference, entry_break)?;
            output.write_char('}')?;
        }

        output.write_char(']')
    }

    /// Render deterministic TypeScript type aliases from admitted nodes.
    ///
    /// Numeric widths, enum IDs and field bounds are informational comments;
    /// TypeScript types cannot enforce these runtime constraints. This produces
    /// declarations only, with no browser codec or executable metadata.
    ///
    /// # Errors
    /// Returns Complexity before extending output beyond its byte allowance.
    pub fn declarations(&self) -> Result<String, GenerationError> {
        let mut output = Output::new(self.output_bytes);
        self.render(&mut output)
            .map_err(|_| GenerationError::Complexity)?;
        Ok(output.text)
    }

    fn render(&self, output: &mut Output) -> fmt::Result {
        for (name, root) in self.names.iter().zip(&self.roots) {
            write!(output, "export type {name} = ")?;
            self.render_node(*root, output)?;
            output.write_str(";\n")?;
        }
        Ok(())
    }

    fn render_node(&self, id: usize, output: &mut Output) -> fmt::Result {
        if let Some(bounds) = self
            .annotations
            .get(&id)
            .and_then(|annotation| annotation.bounds.as_ref())
        {
            output.scalar_bounds(bounds, false)?;
        }
        match &self.nodes[id] {
            Node::Unit => output.write_str("null"),
            Node::Bool => output.write_str("boolean"),
            Node::Packed { word, max, .. } => {
                output.bounds(None, *max)?;
                output.write_str(match *word {
                    "bool" => "globalThis.ReadonlyArray<boolean>",
                    "finite-f32" => "globalThis.Float32Array",
                    "finite-f64" => "globalThis.Float64Array",
                    "i8" => "globalThis.Int8Array",
                    "i16" => "globalThis.Int16Array",
                    "u16" => "globalThis.Uint16Array",
                    "i32" => "globalThis.Int32Array",
                    "u32" => "globalThis.Uint32Array",
                    "i64" => "globalThis.BigInt64Array",
                    "u64" => "globalThis.BigUint64Array",
                    _ => "globalThis.Uint8Array",
                })
            }
            Node::Float { bits } => {
                write!(output, "/* f{bits} */ number")
            }
            Node::Integer { bits, signed } => {
                write!(
                    output,
                    "/* {}{bits} */ {}",
                    if *signed { "i" } else { "u" },
                    if *bits > 32 { "bigint" } else { "number" }
                )
            }
            Node::Text { max } => {
                output.bounds(None, *max)?;
                output.write_str("string")
            }
            Node::Bytes { min, max } => {
                output.bounds(*min, *max)?;
                output.write_str("globalThis.Uint8Array")
            }
            Node::List { item, max } => {
                output.bounds(None, *max)?;
                output.write_str("globalThis.ReadonlyArray<")?;
                self.render_node(*item, output)?;
                output.write_char('>')
            }
            Node::Option(item) | Node::Recover(item) => self.render_option(*item, output),
            Node::Array { item, len } => {
                output.bounds(None, Some(*len))?;
                output.write_str("globalThis.ReadonlyArray<")?;
                self.render_node(*item, output)?;
                output.write_char('>')
            }
            Node::Tuple(items) | Node::ExtensionTuple { items, .. } => {
                output.write_str("readonly [")?;
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        output.write_str(", ")?;
                    }
                    self.render_node(*item, output)?;
                }
                if let Node::ExtensionTuple { extensions, .. } = &self.nodes[id] {
                    for (index, entry) in extensions.iter().enumerate() {
                        if !items.is_empty() || index != 0 {
                            output.write_str(", ")?;
                        }
                        self.render_option(entry.child, output)?;
                    }
                }
                output.write_char(']')
            }
            Node::Result { ok, err } => {
                output.write_str("{ readonly ok: ")?;
                self.render_node(*ok, output)?;
                output.write_str(" } | { readonly err: ")?;
                self.render_node(*err, output)?;
                output.write_str(" }")
            }
            Node::Alias(item) => self.render_node(*item, output),
            Node::Ref(root) => output.write_str(&self.names[*root]),
            Node::Record(fields) | Node::ExtensionRecord { fields, .. } => {
                output.write_char('{')?;
                for (name, child) in fields {
                    output.write_str(" readonly ")?;
                    output.quoted(name)?;
                    output.write_str(": ")?;
                    self.render_node(*child, output)?;
                    output.write_char(';')?;
                }
                if let Node::ExtensionRecord { extensions, .. } = &self.nodes[id] {
                    for entry in extensions {
                        output.write_str(" readonly ")?;
                        output.quoted(&entry.name)?;
                        output.write_str(": ")?;
                        if entry.required {
                            self.render_node(entry.child, output)?;
                        } else {
                            self.render_option(entry.child, output)?;
                        }
                        output.write_char(';')?;
                    }
                }
                output.write_str(" }")
            }
            Node::Enum(variants) | Node::OpenVariant { variants, .. } => {
                let accepts_null = matches!(
                    self.nodes[id],
                    Node::OpenVariant {
                        required: false,
                        ..
                    }
                );
                if accepts_null {
                    output.write_str("null")?;
                } else if variants.is_empty() {
                    output.write_str("never")?;
                }
                for (index, (name, tag, child)) in variants.iter().enumerate() {
                    if accepts_null || index != 0 {
                        output.write_str(" | ")?;
                    }
                    write!(output, "/* id {tag} */ {{ readonly tag: ")?;
                    output.quoted(name)?;
                    output.write_str("; readonly value: ")?;
                    self.render_node(*child, output)?;
                    output.write_str(" }")?;
                }
                Ok(())
            }
        }
    }

    fn render_option(&self, item: usize, output: &mut Output) -> fmt::Result {
        output.write_str("null | { readonly some: ")?;
        self.render_node(item, output)?;

        output.write_str(" }")
    }
}

#[cfg(feature = "api-manifest")]
impl ValidatedRegistry {
    /// Export one format-1 JSON contract per deployed endpoint for offline comparison.
    ///
    /// Supply the deployed name alongside the existing client descriptor. Map keys
    /// are endpoint names, not filesystem paths. Each document contains only named
    /// definitions reachable from its two evolving message roots. The sum of all document lengths
    /// is limited by the registry's output byte allowance. Empty inventories are valid.
    /// Applications own export coverage and the decision to accept baseline changes.
    ///
    /// # Errors
    /// Returns Schema for empty or duplicate endpoint names or inadmissible method
    /// descriptors, and Complexity for excessive method counts, limits or output.
    pub fn api_manifests(
        &self,
        endpoints: &[(&str, Method<'_>)],
    ) -> Result<BTreeMap<String, String>, GenerationError> {
        if endpoints.len() > self.output_bytes {
            return Err(GenerationError::Complexity);
        }
        self.admit_methods(endpoints.iter().map(|(_, method)| method))?;
        let mut documents = BTreeMap::new();
        let mut remaining = self.output_bytes;
        for &(endpoint, method) in endpoints {
            if endpoint.len() > remaining {
                return Err(GenerationError::Complexity);
            }
            if endpoint.is_empty() || documents.contains_key(endpoint) {
                return Err(GenerationError::Schema);
            }
            let roots = [
                self.root_index(method.request)?,
                self.root_index(method.response)?,
            ];
            let mut output = Output::new(remaining);
            self.render_manifest(&mut output, endpoint, &method, roots)
                .map_err(|_| GenerationError::Complexity)?;
            remaining = output.remaining;
            documents.insert(String::from(endpoint), output.text);
        }

        Ok(documents)
    }

    fn render_manifest(
        &self,
        out: &mut Output,
        endpoint: &str,
        method: &Method<'_>,
        roots: [usize; 2],
    ) -> fmt::Result {
        out.write_str("{\n  \"format\":1,\n  \"endpoint\":")?;
        out.quoted(endpoint)?;
        out.write_str(",\n  \"export\":")?;
        out.quoted(method.export)?;
        write!(out, ",\n  \"id\":\"{}\"", method.id)?;
        for (direction, root, limits) in [
            ("request", method.request, method.request_limits),
            ("response", method.response, method.response_limits),
        ] {
            write!(out, ",\n  \"{direction}\":{{\"root\":")?;
            out.quoted(root)?;
            write!(
                out,
                ",\"payload_limits\":{{\"bytes\":{},\"nodes\":{},\"depth\":{}}}}}",
                limits.bytes, limits.nodes, limits.depth
            )?;
        }
        out.write_str(",\n  \"schemas\":{")?;
        let mut pending = BTreeSet::from(roots);
        let mut written = BTreeSet::new();
        while let Some(root) = pending.pop_first() {
            if !written.is_empty() {
                out.write_char(',')?;
            }
            written.insert(root);
            out.write_str("\n    ")?;
            out.quoted(&self.names[root])?;
            out.write_char(':')?;
            self.metadata_node(
                self.roots[root],
                out,
                &mut |target| {
                    if !written.contains(&target) {
                        pending.insert(target);
                    }
                },
                "\n      ",
            )?;
        }

        out.write_str("\n  }\n}\n")
    }
}
