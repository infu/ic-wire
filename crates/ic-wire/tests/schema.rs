//! Schema graph admission and declaration contracts.
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
fn literal_declaration_preserves_order_and_constraints() {
    let value = json!({"k":"record","fields":[
        {"name":"count","value":{"k":"u","bits":16}},
        {"name":"bytes","value":{"k":"bytes","min":1,"max":9}},
        {"name":"packed","value":{"k":"packed","word":"i16","max":4}},
        {"name":"list","value":{"k":"list","max":3,"item":{"k":"i","bits":64}}},
        {"name":"choice","value":{"k":"enum","variants":[
            {"name":"None","id":0,"value":{"k":"unit"}},
            {"name":"Some","id":7,"value":{"k":"option","item":{"k":"text","max":5}}}
        ]}}
    ]});
    assert_eq!(
        registry(value).unwrap().declarations().unwrap(),
        concat!(
            "export type Root = { readonly \"count\": /* u16 */ number;",
            " readonly \"bytes\": /* min 1 */ /* max 9 */ globalThis.Uint8Array;",
            " readonly \"packed\": /* max 4 */ globalThis.Int16Array;",
            " readonly \"list\": /* max 3 */ globalThis.ReadonlyArray</* i64 */ bigint>;",
            " readonly \"choice\": /* id 0 */ { readonly tag: \"None\"; readonly value: null }",
            " | /* id 7 */ { readonly tag: \"Some\"; readonly value: null | { readonly some: /* max 5 */ string } }; };\n"
        )
    );
}

#[test]
fn graph_owns_input_and_quotes_labels() {
    let mut types = BTreeMap::from([(
        "Root".into(),
        json!({"k":"record","fields":[
            {"name":"\"\\\n\u{2028}*/","value":{"k":"bool"}}
        ]}),
    )]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    types.clear();
    assert_eq!(
        graph.declarations().unwrap(),
        "export type Root = { readonly \"\\\"\\\\\\u000a\\u2028*/\": boolean; };\n"
    );
}

#[test]
fn rejects_invalid_shapes_and_constraints() {
    for value in [
        json!({"k":"ref","name":"Missing"}),
        json!({"k":"unknown"}),
        json!({"k":"bool","extra":true}),
        json!({"k":"u","bits":7}),
        json!({"k":"text","max":null}),
        json!({"k":"text","max":-1}),
        json!({"k":"bytes","min":3,"max":2}),
        json!({"k":"packed-i16"}),
        json!({"k":"packed-i16","max":2}),
        json!({"k":"packed","word":"bool","nonzero":true}),
        json!({"k":"packed","word":"i64","nonzero":false}),
        json!({"k":"packed","word":"finite-f32","nonzero":true}),
        json!({"k":"packed","word":"f32"}),
        json!({"k":"enum","variants":[]}),
        json!({"k":"list"}),
        json!({"k":"array","item":{"k":"unit"}}),
        json!({"k":"array","max":-1,"item":{"k":"unit"}}),
        json!({"k":"tuple","items":null}),
        json!({"k":"tuple","items":[{"k":"missing"}]}),
        json!({"k":"result","ok":{"k":"unit"}}),
        json!({"k":"float","bits":16}),
        json!({"k":"enum","variants":[{"name":"A","id":0,"value":{"k":"unit"}}, {"name":"B","id":0,"value":{"k":"unit"}}]}),
        json!({"k":"record","fields":[{"name":"x","value":{"k":"unit"}}, {"name":"x","value":{"k":"bool"}}]}),
    ] {
        assert_eq!(registry(value).unwrap_err(), GenerationError::Schema);
    }
    for name in ["", "0Root", "class", "bad-name"] {
        assert_eq!(
            ValidatedRegistry::from_metadata(
                &BTreeMap::from([(name.into(), json!({"k":"unit"}))]),
                SchemaLimits::default()
            )
            .unwrap_err(),
            GenerationError::Schema
        );
    }
}

#[test]
fn standard_shape_metadata_roundtrips_and_charges_all_children() {
    let value = json!({"k":"result","ok":{"k":"tuple","items":[{"k":"array","max":0,"item":{"k":"u","bits":8}},{"k":"float","bits":64}]},"err":{"k":"unit"}});
    let types = BTreeMap::from([("Root".into(), value)]);
    let limits = SchemaLimits {
        nodes: 7,
        ..SchemaLimits::default()
    };
    let graph = ValidatedRegistry::from_metadata(&types, limits).unwrap();
    let exported: BTreeMap<String, Value> =
        serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(exported, types);
    assert_eq!(
        ValidatedRegistry::from_metadata(&types, SchemaLimits { nodes: 6, ..limits }).unwrap_err(),
        GenerationError::Complexity
    );
    assert_eq!(
        registry(json!({"k":"array","max":9007199254740992_u64,"item":{"k":"unit"}})).unwrap_err(),
        GenerationError::Complexity
    );
}

#[test]
fn alias_cycles_reject_but_productive_recursion_remains_named() {
    for value in [
        json!({"k":"ref","name":"Root"}),
        json!({"k":"newtype","item":{"k":"ref","name":"Root"}}),
    ] {
        assert_eq!(registry(value).unwrap_err(), GenerationError::Schema);
    }
    let types = BTreeMap::from([
        ("A".into(), json!({"k":"ref","name":"B"})),
        ("B".into(), json!({"k":"ref","name":"A"})),
    ]);
    assert_eq!(
        ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap_err(),
        GenerationError::Schema
    );
    assert_eq!(
        registry(json!({"k":"option","item":{"k":"ref","name":"Root"}}))
            .unwrap()
            .declarations()
            .unwrap(),
        "export type Root = null | { readonly some: Root };\n"
    );
}

#[test]
fn exact_host_allowances_and_output_refusal() {
    let types = BTreeMap::from([("Root".into(), json!({"k":"bool"}))]);
    let limits = SchemaLimits {
        nodes: 2,
        depth: 0,
        names_bytes: 4,
        output_bytes: 28,
    };
    let declaration = ValidatedRegistry::from_metadata(&types, limits)
        .unwrap()
        .declarations()
        .unwrap();
    assert_eq!(declaration, "export type Root = boolean;\n");
    let exact = SchemaLimits {
        output_bytes: declaration.len(),
        ..limits
    };
    assert!(
        ValidatedRegistry::from_metadata(&types, exact)
            .unwrap()
            .declarations()
            .is_ok()
    );
    assert_eq!(
        ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes: declaration.len() - 1,
                ..limits
            }
        )
        .unwrap()
        .declarations(),
        Err(GenerationError::Complexity)
    );
    for limits in [
        SchemaLimits { nodes: 1, ..limits },
        SchemaLimits {
            names_bytes: 3,
            ..limits
        },
        SchemaLimits {
            depth: 65,
            ..limits
        },
    ] {
        assert_eq!(
            ValidatedRegistry::from_metadata(&types, limits).unwrap_err(),
            GenerationError::Complexity
        );
    }
    let nested = BTreeMap::from([("Root".into(), json!({"k":"option","item":{"k":"bool"}}))]);
    assert_eq!(
        ValidatedRegistry::from_metadata(&nested, SchemaLimits { nodes: 3, ..limits }).unwrap_err(),
        GenerationError::Complexity
    );
    assert!(
        ValidatedRegistry::from_metadata(
            &nested,
            SchemaLimits {
                nodes: 3,
                depth: 1,
                ..limits
            }
        )
        .is_ok()
    );
    assert_eq!(
        registry(json!({"k":"text","max":9007199254740992_u64})).unwrap_err(),
        GenerationError::Complexity
    );
    assert_eq!(
        registry(json!({"k":"packed","word":"i16","max":4503599627370496_u64})).unwrap_err(),
        GenerationError::Complexity
    );
}

#[test]
fn diamond_graph_does_not_expand_root_paths() {
    let mut types = BTreeMap::from([("N0".into(), json!({"k":"unit"}))]);
    for level in 1..1000 {
        let previous = format!("N{}", level - 1);
        types.insert(
            format!("N{level}"),
            json!({"k":"record","fields":[
                {"name":"left","value":{"k":"ref","name":previous}},
                {"name":"right","value":{"k":"ref","name":previous}}
            ]}),
        );
    }
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let before = graph.declarations().unwrap();
    assert_eq!(before.lines().count(), 1000);
    assert!(before.len() < 100_000);
    types.insert("Unrelated".into(), json!({"k":"bool"}));
    let after = ValidatedRegistry::from_metadata(&types, SchemaLimits::default())
        .unwrap()
        .declarations()
        .unwrap();
    assert!(after.starts_with(&before));
}

#[test]
fn metadata_export_preserves_all_admitted_shapes_and_escaped_names() {
    let mut types = BTreeMap::from([
        ("__proto__".into(), json!({"k":"unit"})),
        (
            "Root".into(),
            json!({"k":"record","fields":[
                {"name":"\"\\\n\u{2028}é","value":{"k":"bool"}},
                {"name":"signed","value":{"k":"i","bits":128}},
                {"name":"unsigned","value":{"k":"u","bits":8}},
                {"name":"text","value":{"k":"text","max":0}},
                {"name":"bytes","value":{"k":"bytes","min":0,"max":9007199254740991_u64}},
                {"name":"packed","value":{"k":"packed","word":"i16","max":2}},
                {"name":"list","value":{"k":"list","max":4,"item":{"k":"ref","name":"__proto__"}}},
                {"name":"optional","value":{"k":"option","item":{"k":"text"}}},
                {"name":"newtype","value":{"k":"newtype","item":{"k":"bytes"}}},
                {"name":"enum","value":{"k":"enum","variants":[{"name":"x","id":4294967295_u64,"value":{"k":"ref","name":"Root"}}]}}
            ]}),
        ),
    ]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let exported = graph.metadata_json().unwrap();
    assert_eq!(
        serde_json::from_str::<BTreeMap<String, Value>>(&exported).unwrap(),
        types
    );
    types.clear();
    assert_eq!(graph.metadata_json().unwrap(), exported);
    let imported = ValidatedRegistry::from_metadata(
        &serde_json::from_str(&exported).unwrap(),
        SchemaLimits::default(),
    )
    .unwrap();
    assert_eq!(
        imported.declarations().unwrap(),
        graph.declarations().unwrap()
    );
    assert_eq!(imported.metadata_json().unwrap(), exported);
}

#[test]
fn metadata_output_is_bounded_in_utf8_bytes() {
    let types = BTreeMap::from([(
        "Root".into(),
        json!({"k":"record","fields":[{"name":"é\n","value":{"k":"unit"}}]}),
    )]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let output = graph.metadata_json().unwrap();
    for bytes in [0, output.len() - 1, output.len()] {
        let graph = ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes: bytes,
                ..SchemaLimits::default()
            },
        )
        .unwrap();
        if bytes < output.len() {
            assert_eq!(graph.metadata_json(), Err(GenerationError::Complexity));
        } else {
            assert_eq!(graph.metadata_json().unwrap(), output);
        }
    }
}

#[test]
fn generated_payload_module_admits_names_policy_and_total_output() {
    use ic_wire::typescript::PayloadLimits;
    let types = BTreeMap::from([
        (
            "Fixed".into(),
            json!({"k":"record","fixed":true,"fields":[
                {"name":"flag","value":{"k":"bool"}}
            ]}),
        ),
        (
            "Message".into(),
            json!({"k":"extension-tuple","items":[{"k":"ref","name":"Fixed"}],"extensions":[]}),
        ),
        (
            "Root".into(),
            json!({"k":"newtype","item":{"k":"ref","name":"Message"}}),
        ),
    ]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let limits = PayloadLimits {
        bytes: 2,
        nodes: 4,
        depth: 4,
    };
    let source = graph
        .payload_module(
            "codec",
            "Root",
            "./runtime.ts",
            limits,
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap();
    assert!(source.contains("ic-wire-json"));
    assert!(source.contains("encode:(value: Root)"));
    assert!(graph.declarations().unwrap().contains("export type Fixed"));
    for name in ["Root", "globalThis", "class", "x;throw"] {
        assert_eq!(
            graph.payload_module(
                name,
                "Root",
                "./runtime.ts",
                limits,
                &Default::default(),
                |report| eprint!("{report}")
            ),
            Err(GenerationError::Schema)
        );
    }
    for invalid_root in ["Missing", "Fixed"] {
        assert_eq!(
            graph.payload_module(
                "codec",
                invalid_root,
                "./runtime.ts",
                limits,
                &Default::default(),
                |_| panic!("rejected clients do not report diagnostics")
            ),
            Err(GenerationError::Schema)
        );
    }
    for module in ["", "bad\0module"] {
        assert_eq!(
            graph.payload_module(
                "codec",
                "Root",
                module,
                limits,
                &Default::default(),
                |report| eprint!("{report}")
            ),
            Err(GenerationError::Schema)
        );
    }
    for invalid in [
        PayloadLimits { nodes: 0, ..limits },
        PayloadLimits { depth: 0, ..limits },
    ] {
        assert_eq!(
            graph.payload_module(
                "codec",
                "Root",
                "./runtime.ts",
                invalid,
                &Default::default(),
                |report| eprint!("{report}")
            ),
            Err(GenerationError::Schema)
        );
    }
    assert_eq!(
        graph.payload_module(
            "codec",
            "Root",
            "./runtime.ts",
            PayloadLimits {
                bytes: 1 << 53,
                ..limits
            },
            &Default::default(),
            |report| eprint!("{report}")
        ),
        Err(GenerationError::Complexity)
    );
    for output_bytes in [source.len() - 1, source.len()] {
        let bounded = ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes,
                ..SchemaLimits::default()
            },
        )
        .unwrap();
        let result = bounded.payload_module(
            "codec",
            "Root",
            "./runtime.ts",
            limits,
            &Default::default(),
            |report| eprint!("{report}"),
        );
        if output_bytes < source.len() {
            assert_eq!(result, Err(GenerationError::Complexity));
        } else {
            assert_eq!(result.unwrap(), source);
        }
    }
}

#[test]
fn method_modules_admit_identities_directions_and_bounded_source() {
    use ic_wire::typescript::{Method, PayloadLimits};
    let types = BTreeMap::from([
        ("Fixed".into(), json!({"k":"bool"})),
        (
            "Request".into(),
            json!({"k":"extension-tuple","items":[],"extensions":[]}),
        ),
        (
            "Response".into(),
            json!({"k":"extension-record","fields":[
            {"name":"accepted","value":{"k":"ref","name":"Fixed"}}
        ],"extensions":[]}),
        ),
    ]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let method = Method {
        export: "call",
        id: u64::MAX,
        request: "Request",
        response: "Response",
        request_limits: PayloadLimits {
            bytes: 1,
            nodes: 1,
            depth: 1,
        },
        response_limits: PayloadLimits {
            bytes: 2,
            nodes: 2,
            depth: 3,
        },
    };
    let source = graph
        .method_module(&[method], "./runtime.ts", &Default::default(), |report| {
            eprint!("{report}")
        })
        .unwrap();
    assert!(source.contains("id:'18446744073709551615'"));
    assert!(source.contains("requestBytes:12, requestNodes:1, requestDepth:1"));
    assert!(source.contains("responseBytes:13, responseNodes:2, responseDepth:3"));
    assert!(source.contains("encodeRequest:(value: Request)"));
    assert!(source.contains("decodeResponse:(bytes: globalThis.Uint8Array): Response"));
    for duplicate in [
        Method {
            export: "other",
            ..method
        },
        Method { id: 7, ..method },
    ] {
        assert_eq!(
            graph.method_module(
                &[method, duplicate],
                "./runtime.ts",
                &Default::default(),
                |report| eprint!("{report}")
            ),
            Err(GenerationError::Schema)
        );
    }
    for invalid in [
        Method {
            request: "Missing",
            ..method
        },
        Method {
            response: "Missing",
            ..method
        },
        Method {
            request: "Fixed",
            ..method
        },
        Method {
            response: "Fixed",
            ..method
        },
        Method {
            export: "globalThis",
            ..method
        },
    ] {
        assert_eq!(
            graph.method_module(
                &[invalid],
                "./runtime.ts",
                &Default::default(),
                |report| eprint!("{report}")
            ),
            Err(GenerationError::Schema)
        );
        #[cfg(feature = "api-manifest")]
        assert_eq!(
            graph.api_manifests(&[("call", invalid)]),
            Err(GenerationError::Schema)
        );
    }
    assert_eq!(
        graph.method_module(&[], "./runtime.ts", &Default::default(), |report| eprint!(
            "{report}"
        )),
        Err(GenerationError::Schema)
    );
    let safe = Method {
        request_limits: PayloadLimits {
            bytes: (1_u64 << 53) - 12,
            ..method.request_limits
        },
        ..method
    };
    assert!(
        graph
            .method_module(
                &[safe],
                "./runtime.ts",
                &Default::default(),
                |report| eprint!("{report}")
            )
            .is_ok()
    );
    let overflow = Method {
        request_limits: PayloadLimits {
            bytes: (1_u64 << 53) - 11,
            ..method.request_limits
        },
        ..method
    };
    assert_eq!(
        graph.method_module(
            &[overflow],
            "./runtime.ts",
            &Default::default(),
            |report| eprint!("{report}")
        ),
        Err(GenerationError::Complexity)
    );
    for output_bytes in [source.len() - 1, source.len()] {
        let graph = ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes,
                ..SchemaLimits::default()
            },
        )
        .unwrap();
        if output_bytes < source.len() {
            assert_eq!(
                graph.method_module(
                    &[method],
                    "./runtime.ts",
                    &Default::default(),
                    |report| eprint!("{report}")
                ),
                Err(GenerationError::Complexity)
            );
        } else {
            assert_eq!(
                graph
                    .method_module(
                        &[method],
                        "./runtime.ts",
                        &Default::default(),
                        |report| eprint!("{report}")
                    )
                    .unwrap(),
                source
            );
        }
    }
}

#[test]
fn quoted_utf8_spans_preserve_escapes_and_every_output_boundary() {
    let controls: String = (0_u8..32).map(char::from).collect();
    let label = format!("plainé😀\"\\{controls}\u{2028}\u{2029}tail");
    let types = BTreeMap::from([(
        "Root".into(),
        json!({"k":"record","fields":[
            {"name":label,"value":{"k":"bool"}}
        ]}),
    )]);
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let metadata = graph.metadata_json().unwrap();
    assert_eq!(
        serde_json::from_str::<BTreeMap<String, Value>>(&metadata).unwrap(),
        types
    );
    assert!(metadata.contains("plainé😀\\\"\\\\\\u0000"));
    assert!(metadata.contains("\\u001f\\u2028\\u2029tail"));
    assert!(!metadata.contains('\u{2028}'));
    let declaration = graph.declarations().unwrap();
    for limit in 0..=metadata.len().max(declaration.len()) {
        let graph = ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes: limit,
                ..SchemaLimits::default()
            },
        )
        .unwrap();
        for (actual, expected) in [
            (graph.metadata_json(), &metadata),
            (graph.declarations(), &declaration),
        ] {
            if limit < expected.len() {
                assert_eq!(actual, Err(GenerationError::Complexity));
            } else {
                assert_eq!(actual.unwrap(), *expected);
            }
        }
    }
}
