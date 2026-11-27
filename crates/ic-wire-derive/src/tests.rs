use super::expand;

#[test]
fn rejects_layout_attributes_and_unqualified_shapes() {
    for input in [
        "#[serde(transparent)] struct Record { value: u8 }",
        "struct Record { #[serde(skip)] value: u8 }",
        "struct Record { #[serde(with = \"custom_bytes\")] value: Vec<u8> }",
        "struct Record { #[serde(with = \"other::serde_packed::bools\")] value: Vec<bool> }",
        "struct Record { #[serde(with = \"ic_wire::serde_packed::unknown\")] value: Vec<bool> }",
        "struct Record { #[serde(with = \"ic_wire::serde_packed::bools\", with = \"serde_bytes\")] value: Vec<bool> }",
        "struct Record { #[serde(with = \"serde_bytes\", with = \"serde_bytes\")] value: Vec<u8> }",
        "enum Choice { #[serde(with = \"serde_bytes\")] Value(u8) }",
        "enum Choice { #[serde(rename(serialize = \"x\", deserialize = \"y\"))] Value(u8) }",
        "struct Record { #[serde(rename = \"x\", rename = \"y\")] value: u8 }",
        "#[serde(rename_all = \"camelCase\")] struct Record { some_value: u8 }",
        "enum Choice { #[serde(borrow)] Value(u8) }",
        "struct Record(#[serde(rename = \"ignored\")] u8);",
        "#[serde(untagged)] enum Choice { Value(u8) }",
        "struct Record<'a> { #[serde(borrow, skip)] value: &'a str }",
        "struct Record<'a> { #[serde(borrow = 1)] value: &'a str }",
        "enum Choice { Value = 7 }",
        "#[wire(unknown)] struct Record;",
        "#[wire(crate = \"one\", crate = \"two\")] struct Record;",
    ] {
        let input = syn::parse_str(input).unwrap();
        assert!(expand(&input).is_err());
    }
}

#[test]
fn supports_ordinary_shapes_and_renamed_dependency() {
    for input in [
        "struct Unit;",
        "struct Newtype(u16);",
        "struct Pair(u8, u8);",
        "struct Empty();",
        "struct Bools { #[serde(with = \"::ic_wire::serde_packed::bools\")] value: Vec<bool> }",
        "#[wire(crate = \"renamed\")] struct Floats(#[serde(with = \"renamed::serde_packed::finite_f32\")] Vec<f32>);",
        "struct Bytes { #[serde(with = \"serde_bytes\")] value: Option<Vec<u8>> }",
        "enum Choice { Pair(u8, u8) }",
        "struct Record { r#type: u8, value: Option<u16> }",
        "enum Choice { Empty, Value(u16), Record { flag: bool } }",
        "#[wire(crate = \"candidate\")] struct Record { value: u8 }",
        "struct Generic<T>(T);",
        "enum Choice { #[serde(rename = \"x\")] Value(u8) }",
        "struct Record<'a> { #[serde(rename = \"text-value\", borrow)] value: &'a str }",
        "struct Generic<'a, T: Clone = u8, const N: usize = 4> where T: Copy { value: T, text: &'a str, bytes: Bytes<N> }",
        "struct Borrowed<'a> { #[serde(borrow = \"'a\")] bytes: BytesRef<'a, 8> }",
    ] {
        let fixed = format!("#[wire(fixed)] {input}");
        assert!(expand(&syn::parse_str(&fixed).unwrap()).is_ok());
    }
}

#[test]
fn rejects_ambiguous_evolution_declarations() {
    for input in [
        "struct Fields { value: Option<u8> }",
        "struct Fields { #[wire(id=0)] value: Option<u8> }",
        "struct Fields { #[wire(id=7)] value: u8 }",
        "struct Fields { #[wire(id=1)] one: Option<u8>, #[wire(id=1)] two: Option<u8> }",
        "struct Fields { #[wire(id=1, rename=\"x\")] one: Option<u8>, #[wire(id=2, rename=\"x\")] two: Option<u8> }",
        "struct Fields { #[wire(id=1, required, required)] epoch: u32 }",
        "struct Fields { #[wire(id=1)] #[serde(with=\"custom\")] value: Option<u8> }",
        "enum Cases { #[wire(id=0)] A }",
        "enum Cases { #[wire(id=2147483648)] A }",
        "enum Cases { #[wire(id=1, required)] A }",
        "enum Cases { #[wire(id=1)] A(u8, u8) }",
        "enum Cases { #[wire(id=1)] A = 3 }",
    ] {
        let input = syn::parse_str(input).unwrap();
        assert!(super::evolve::expand(&input).is_err());
    }
}

#[test]
fn checks_inline_record_and_tuple_positions() {
    for input in [
        "struct Message { tag: u16, #[wire(id = 7)] note: Option<u8> }",
        "struct Message { tag: u16, #[wire(id = 7, required)] value: Nested }",
        "#[serde(crate = \"reexport::serde\")] struct Message<'a> { #[serde(rename = \"tag\", borrow = \"'a\")] data: &'a str }",
        "#[wire(crate = \"renamed\")] struct Message(u16, #[wire(id = 2)] Option<u8>);",
        "struct Empty {}",
        "struct EmptyTuple();",
        "struct EmptyUnit;",
    ] {
        let input = syn::parse_str(input).unwrap();
        assert!(expand(&input).is_ok());
    }
    for input in [
        "struct Message { #[wire(id = 7)] note: u8 }",
        "struct Message { #[wire(id = 0)] note: Option<u8> }",
        "struct Message { #[wire(id = 7)] note: Option<u8>, #[wire(id = 7)] extra: Option<bool> }",
        "struct Message { key: u8, #[wire(id = 7, rename = \"key\")] note: Option<u8> }",
        "struct Message { #[serde(rename = \"key\")] value: u8, #[wire(id = 7, rename = \"key\")] note: Option<u8> }",
        "struct Message { #[serde(rename = \"key\")] #[wire(rename = \"other\")] value: u8 }",
        "struct Message { #[serde(with = \"serde_bytes\")] key: Vec<u8> }",
        "#[serde(crate = \"serde\", crate = \"other\")] struct Message;",
        "#[serde(crate = \"{ not a crate }\")] struct Message;",
        "struct Message { #[wire(id = 7, required)] note: Option<u8> }",
        "struct Message { #[wire(required)] value: u8 }",
        "struct Message { #[serde(skip)] key: u8 }",
        "struct Message(u8, #[wire(id = 3)] Option<u16>);",
        "struct Message(u8, #[wire(id = 2, required)] u16);",
        "struct Message(#[wire(id = 1)] Option<u16>, u8);",
        "#[wire(fixed)] struct Message { #[wire(id = 7)] note: Option<u8> }",
        "#[wire(record)] struct Message { tag: u16 }",
    ] {
        let input = syn::parse_str(input).unwrap();
        assert!(expand(&input).is_err());
    }
}

#[test]
fn checks_extension_carrier_position_in_wire_derive() {
    for input in [
        "struct Physical { #[wire(extensions)] fields: ExtensionSlot, after: u8 }",
        "struct Physical { #[wire(extensions)] #[serde(rename=\"renamed\")] fields: ExtensionSlot }",
        "struct Physical { #[wire(extensions)] #[wire(extensions)] fields: ExtensionSlot }",
        "enum Physical { Variant { #[wire(extensions)] fields: ExtensionSlot } }",
        "struct Physical(#[wire(extensions)] ExtensionSlot, u8);",
        "struct Physical(u8, #[wire(extensions)] #[serde(with=\"serde_bytes\")] ExtensionSlot);",
        "enum Physical { Variant(u8, #[wire(extensions)] ExtensionSlot) }",
        "enum Physical { Variant(#[wire(extensions)] ExtensionSlot) }",
    ] {
        assert!(expand(&syn::parse_str(input).unwrap()).is_err());
    }
    for input in [
        "struct Physical<'a> { core: u32, #[wire(extensions)] #[serde(borrow)] fields: ExtensionSlot<'a, Fields<'a>> }",
        "struct Physical<'a>(u32, #[wire(extensions)] #[serde(borrow)] ExtensionSlot<'a, Fields<'a>>);",
        "struct Physical<'a>(#[wire(extensions)] #[serde(borrow)] ExtensionSlot<'a, Fields<'a>>);",
    ] {
        assert!(expand(&syn::parse_str(input).unwrap()).is_ok());
    }
}
