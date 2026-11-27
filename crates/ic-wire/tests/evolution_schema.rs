//! Metadata and TypeScript declarations for bounded branch recovery.
#![cfg(feature = "typescript")]

use ic_wire::typescript::{GenerationError, SchemaLimits, ValidatedRegistry};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn registry(value: Value) -> Result<ValidatedRegistry, GenerationError> {
    ValidatedRegistry::from_metadata(
        &BTreeMap::from([("Root".into(), value)]),
        SchemaLimits::default(),
    )
}

#[test]
fn bounded_optional_and_required_open_roundtrip_with_different_presence_semantics() {
    let value = json!({"k":"recover","item":{"k":"open-variant","required":true,"variants":[
        {"name":"Active","id":1,"value":{"k":"unit"}}
    ]}});
    let graph = registry(value.clone()).unwrap();
    let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(exported, json!({"Root":value}));
    assert_eq!(
        graph.declarations().unwrap(),
        "export type Root = null | { readonly some: /* id 1 */ { readonly tag: \"Active\"; readonly value: null } };\n"
    );
    assert_eq!(
        registry(json!({"k":"open-variant","variants":[]}))
            .unwrap()
            .declarations()
            .unwrap(),
        "export type Root = null;\n"
    );
    assert_eq!(
        registry(json!({"k":"open-variant","required":true,"variants":[]}))
            .unwrap()
            .declarations()
            .unwrap(),
        "export type Root = never;\n"
    );
}

#[test]
fn recovery_metadata_requires_explicit_well_formed_kinds() {
    for value in [
        json!({"k":"recover"}),
        json!({"k":"option","item":{"k":"bool"},"required":true}),
        json!({"k":"open-variant","required":"true","variants":[]}),
        json!({"k":"open-variant","required":true,"variants":[
            {"name":"Zero","id":0,"value":{"k":"unit"}}
        ]}),
        json!({"k":"open-variant","required":true,"variants":[
            {"name":"TooLarge","id":2147483648_u32,"value":{"k":"unit"}}
        ]}),
    ] {
        assert_eq!(registry(value).unwrap_err(), GenerationError::Schema);
    }
}

#[test]
fn extension_tuple_admits_only_contiguous_optional_tail_slots() {
    let slot = |id: u32, name: &str, required: bool| json!({"id":id,"name":name,"required":required,"value":{"k":"unit"}});
    let schema = |extensions| {
        json!({
            "k": "extension-tuple",
            "items": [{"k": "u", "bits": 8}],
            "extensions": extensions,
        })
    };
    let value = schema(vec![slot(2, "seen", false), slot(3, "confirmed", false)]);
    let graph = registry(value.clone()).unwrap();
    let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(exported, json!({"Root":value}));
    assert_eq!(
        graph.declarations().unwrap(),
        "export type Root = readonly [/* u8 */ number, null | { readonly some: null }, null | { readonly some: null }];\n"
    );
    for extensions in [
        vec![slot(1, "overlap", false)],
        vec![slot(3, "gap", false)],
        vec![slot(2, "required", true)],
        vec![slot(2, "duplicate_id", false), slot(2, "other", false)],
        vec![
            slot(2, "duplicate_name", false),
            slot(3, "duplicate_name", false),
        ],
        vec![slot(2, "first", false), slot(4, "gap", false)],
    ] {
        assert_eq!(
            registry(schema(extensions)).unwrap_err(),
            GenerationError::Schema
        );
    }
    assert_eq!(
        registry(json!({"k":"extension-tuple","items":[],"extensions":[slot(1, "unit", false)]}))
            .unwrap()
            .declarations()
            .unwrap(),
        "export type Root = readonly [null | { readonly some: null }];\n"
    );
}

#[test]
fn fixed_annotations_roundtrip_without_changing_type_declarations() {
    for mut value in [
        json!({"k":"record","fields":[{"name":"flag","value":{"k":"bool"}}]}),
        json!({"k":"tuple","items":[{"k":"bool"},{"k":"unit"}]}),
    ] {
        let declaration = registry(value.clone()).unwrap().declarations().unwrap();
        value["fixed"] = json!(true);
        let graph = registry(value.clone()).unwrap();
        assert_eq!(graph.declarations().unwrap(), declaration);
        let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
        assert_eq!(exported, json!({"Root":value}));
        for malformed in [json!(false), json!("true")] {
            value["fixed"] = malformed;
            assert_eq!(
                registry(value.clone()).unwrap_err(),
                GenerationError::Schema
            );
        }
    }
    assert_eq!(
        registry(json!({"k":"extension-record","fields":[],"extensions":[],"fixed":true}))
            .unwrap_err(),
        GenerationError::Schema
    );
}

#[test]
fn projected_annotations_apply_only_to_extension_layouts() {
    for mut value in [
        json!({"k":"extension-record","fields":[],"extensions":[]}),
        json!({"k":"extension-tuple","items":[],"extensions":[]}),
    ] {
        let declarations = registry(value.clone()).unwrap().declarations().unwrap();
        value["projected"] = json!(true);
        let graph = registry(value.clone()).unwrap();
        assert_eq!(graph.declarations().unwrap(), declarations);
        let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
        assert_eq!(exported, json!({"Root":value}));
        value["projected"] = json!(false);
        assert_eq!(registry(value).unwrap_err(), GenerationError::Schema);
    }
    assert_eq!(
        registry(json!({"k":"record","fields":[],"projected":true})).unwrap_err(),
        GenerationError::Schema
    );
}

#[cfg(feature = "derive")]
#[test]
fn fixed_derive_marks_local_aggregates_without_implicit_claims_about_children() {
    use ic_wire::typescript::Wire;

    struct Child;
    impl Wire for Child {
        fn schema(_: &mut ic_wire::typescript::SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"record","fields":[{"name":"value","value":{"k":"bool"}}]}))
        }
    }

    #[allow(dead_code)]
    #[derive(Wire)]
    #[wire(fixed)]
    enum Layout {
        Named { child: Child },
        Tuple(bool, ()),
        Wrapped(Child),
    }

    let graph = ValidatedRegistry::from_type::<Layout>("Root", SchemaLimits::default()).unwrap();
    let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    let variants = exported["Root"]["variants"].as_array().unwrap();
    assert_eq!(variants[0]["value"]["fixed"], true);
    assert_eq!(variants[1]["value"]["fixed"], true);
    assert!(
        variants[0]["value"]["fields"][0]["value"]
            .get("fixed")
            .is_none()
    );
    assert!(variants[2]["value"].get("fixed").is_none());
}

#[cfg(feature = "derive")]
#[test]
fn serde_dto_annotations_keep_names_and_borrows_in_nested_wire_records() {
    use ic_wire::{Encoding, Limits, Wire, decode, encode};
    use serde as dto_serde;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, Wire)]
    #[serde(crate = "dto_serde")]
    struct Item<'a> {
        #[serde(rename = "displayName", borrow)]
        name: &'a str,
        #[wire(id = 1)]
        #[serde(rename = "ratingScore")]
        rating: Option<u8>,
    }

    #[derive(Debug, PartialEq, Wire)]
    struct Envelope<'a> {
        item: Item<'a>,
    }

    let graph =
        ValidatedRegistry::from_type::<Envelope<'_>>("Root", SchemaLimits::default()).unwrap();
    let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    let item = &exported["Root"]["fields"][0]["value"];
    assert_eq!(item["fields"][0]["name"], "displayName");
    assert_eq!(item["extensions"][0]["name"], "ratingScore");
    assert_eq!(item["extensions"][0]["id"], 1);

    let value = Envelope {
        item: Item {
            name: "hi",
            rating: Some(8),
        },
    };
    let limits = Limits {
        bytes: 64,
        allocation: 4096,
        nodes: 64,
        depth: 8,
    };
    let bytes = encode(&value, Encoding::Payload, limits).unwrap();
    assert_eq!(bytes, [2, b'h', b'i', 3, 1, 1, 8, 0]);
    let decoded: Envelope<'_> = decode(&bytes, Encoding::Payload, limits).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(decoded.item.name.as_ptr(), bytes[1..].as_ptr());
    assert_eq!(
        postcard::to_allocvec(&value.item).unwrap(),
        [2, b'h', b'i', 1, 8]
    );
}

mod typed_methods {
    use super::*;
    use core::marker::PhantomData;
    use ic_wire::{
        Limits,
        evolution::{__private::EvolvingSchema, Method},
        typescript::{PayloadLimits, SchemaContext, SchemaRegistry, Wire},
    };

    const RUST_LIMITS: Limits = Limits {
        bytes: 42,
        allocation: 1024,
        nodes: 80,
        depth: 6,
    };
    const BROWSER_LIMITS: PayloadLimits = PayloadLimits {
        bytes: 31,
        nodes: 8,
        depth: 3,
    };

    struct Root<T>(PhantomData<T>);
    impl<T: Wire> EvolvingSchema for Root<T> {
        fn describe(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"extension-record","fields":[],"extensions":[{
                "name":"details","id":2,"required":false,"value":context.describe::<T>()?
            }]}))
        }
    }

    struct PlainRoot;
    impl EvolvingSchema for PlainRoot {
        fn describe(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            context.describe::<Fixed>()
        }
    }

    struct Nested<T>(PhantomData<T>);
    impl<T: Wire> Wire for Nested<T> {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"enum","variants":[{
                "name":"Values","id":0,"value":context.describe::<Result<Vec<Option<T>>, ()>>()?
            }]}))
        }
    }

    struct Fixed;
    impl Wire for Fixed {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"record","fixed":true,"fields":[{
                "name":"next","value":context.describe::<Option<Box<Self>>>()?
            }]}))
        }
    }

    struct Unmarked;
    impl Wire for Unmarked {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"tuple","items":[{"k":"bool"},{"k":"unit"}]}))
        }
    }

    struct Opaque;
    impl Wire for Opaque {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k":"record","fixed":true,"fields":[{
                "name":"recover","value":{"k":"recover","item":{"k":"unit"}}
            }]}))
        }
    }

    struct Extension<const PROJECTED: bool>;
    impl<const PROJECTED: bool> Wire for Extension<PROJECTED> {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            let mut shape = json!({"k":"extension-record","fields":[],"extensions":[]});
            if PROJECTED {
                shape["projected"] = json!(true);
            }
            Ok(shape)
        }
    }

    struct BothPaths;
    impl EvolvingSchema for BothPaths {
        fn describe(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            let item = context.describe::<Vec<Option<Extension<true>>>>()?;
            Ok(json!({"k":"extension-record","fields":[
                {"name":"fixed","value":{"k":"record","fixed":true,"fields":[
                    {"name":"items","value":item}
                ]}},
                {"name":"projected","value":item}
            ],"extensions":[]}))
        }
    }

    fn collected<T: EvolvingSchema>(
        method: &Method<T, T>,
    ) -> Result<ValidatedRegistry, GenerationError> {
        let mut registry = SchemaRegistry::new(SchemaLimits::default())?;
        // Ordinary registration remains available and does not impose typed rules.
        registry.register::<Unmarked>("Unrelated")?;
        let descriptor = registry.register_evolving_method(
            method,
            "client",
            ("Both", BROWSER_LIMITS),
            ("Both", BROWSER_LIMITS),
        )?;
        assert_eq!(descriptor.export, "client");
        assert_eq!(descriptor.id, method.id());
        assert_eq!(descriptor.request_limits.nodes, BROWSER_LIMITS.nodes);
        let graph = registry.finish()?;
        assert!(
            graph
                .method_module(&[descriptor], "./runtime.ts", &BTreeMap::new(), |_| {})?
                .contains("export const client")
        );
        Ok(graph)
    }

    #[test]
    fn typed_roots_allow_fixed_recursion_and_reject_unprojected_nested_layouts() {
        collected(&Method::<Root<Nested<Fixed>>, _>::new(
            "canister_export",
            RUST_LIMITS,
            RUST_LIMITS,
        ))
        .unwrap();
        assert_eq!(
            collected(&Method::<Root<Nested<Unmarked>>, _>::new(
                "a",
                RUST_LIMITS,
                RUST_LIMITS
            ))
            .unwrap_err(),
            GenerationError::Schema
        );
        assert_eq!(
            collected(&Method::<Root<Nested<Opaque>>, _>::new(
                "a",
                RUST_LIMITS,
                RUST_LIMITS
            ))
            .unwrap_err(),
            GenerationError::Schema
        );
        assert_eq!(
            collected(&Method::<PlainRoot, _>::new("a", RUST_LIMITS, RUST_LIMITS)).unwrap_err(),
            GenerationError::Schema
        );
    }

    #[test]
    fn typed_roots_project_nested_containers_but_preserve_fixed_boundaries() {
        type Containers<const PROJECTED: bool> =
            Root<Result<Vec<Option<Extension<PROJECTED>>>, ()>>;
        collected(&Method::<Containers<true>, _>::new(
            "a",
            RUST_LIMITS,
            RUST_LIMITS,
        ))
        .unwrap();

        assert_eq!(
            collected(&Method::<Containers<false>, _>::new(
                "a",
                RUST_LIMITS,
                RUST_LIMITS,
            ))
            .unwrap_err(),
            GenerationError::Schema
        );
        assert_eq!(
            collected(&Method::<BothPaths, _>::new("a", RUST_LIMITS, RUST_LIMITS)).unwrap_err(),
            GenerationError::Schema
        );
        assert_eq!(
            collected(&Method::<Root<Nested<Extension<true>>>, _>::new(
                "a",
                RUST_LIMITS,
                RUST_LIMITS,
            ))
            .unwrap_err(),
            GenerationError::Schema
        );
    }

    #[test]
    fn typed_method_aliases_cannot_bind_different_layouts() {
        let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
        let method = Method::<Root<bool>, Root<()>>::new("a", RUST_LIMITS, RUST_LIMITS);
        assert_eq!(
            registry
                .register_evolving_method(
                    &method,
                    "client",
                    ("Both", BROWSER_LIMITS),
                    ("Both", BROWSER_LIMITS),
                )
                .unwrap_err(),
            GenerationError::Schema
        );
        assert_eq!(registry.finish().unwrap_err(), GenerationError::Schema);
    }

    #[cfg(all(feature = "platform-types", feature = "derive"))]
    #[test]
    fn typed_method_uses_the_published_account_layout_as_fixed() {
        use ic_platform_types::{Account, Principal};
        #[derive(ic_wire::Wire)]
        struct Payment {
            account: Account,
        }

        let limits = Limits {
            allocation: 8192,
            ..RUST_LIMITS
        };
        let method = Method::<Payment, Payment>::new("pay", limits, limits);
        let graph = collected(&method).unwrap();
        let metadata: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
        assert_eq!(
            metadata[SchemaRegistry::type_label::<Account>()]["fixed"],
            true
        );

        let payment = Payment {
            account: Account {
                owner: Principal::anonymous(),
                subaccount: None,
            },
        };
        let mut output = Vec::new();
        method.encode_request(&payment, &mut output).unwrap();
        assert_eq!(&output[11..], &[1, 4, 0, 0]);
        let decoded = method.decode_request(&output).unwrap();
        assert_eq!(decoded.account.owner, payment.account.owner);
        assert_eq!(decoded.account.subaccount, None);
    }
}
