//! Rooted bulk advice contracts on admitted shapes.
use crate::{grouped_messages, message_schema};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wire::typescript::{GenerationError, Method, PayloadLimits, SchemaLimits, ValidatedRegistry};
fn lint_roots(
    types: &BTreeMap<String, Value>,
    roots: &BTreeMap<String, String>,
    acks: &BTreeMap<String, String>,
) -> wire::typescript::BulkLintReport {
    let graph = ValidatedRegistry::from_metadata(types, SchemaLimits::default()).unwrap();
    graph.lint_bulk_roots(roots, acks).unwrap()
}
#[test]
fn rooted_advice_preserves_shared_recursive_paths_and_acknowledgements() {
    let types = BTreeMap::from([
        (
            "Words".into(),
            json!({"k":"list","item":{"k":"u","bits":32}}),
        ),
        (
            "Tree".into(),
            json!({"k":"record","fields":[
                {"name":"a/b~c","value":{"k":"ref","name":"Words"}},
                {"name":"other","value":{"k":"ref","name":"Words"}},
                {"name":"next","value":{"k":"option","item":{"k":"ref","name":"Tree"}}}
            ]}),
        ),
    ]);
    let roots = BTreeMap::from([("Reply/~".into(), "Tree".into())]);
    let report = lint_roots(&types, &roots, &BTreeMap::new());
    assert_eq!(
        report
            .warnings
            .iter()
            .map(|w| w.path.as_str())
            .collect::<Vec<_>>(),
        [
            "/Reply~1~0/fields/a~1b~0c/list/u32",
            "/Reply~1~0/fields/other/list/u32"
        ]
    );
    let mut acks = BTreeMap::from([(
        report.warnings[0].path.clone(),
        "Small bounded values; retain varints".into(),
    )]);
    assert_eq!(lint_roots(&types, &roots, &acks).warnings.len(), 1);
    acks.insert("/removed/list/u32".into(), "Former field".into());
    assert!(!lint_roots(&types, &roots, &acks).is_clean());
    let changed = types
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                if name == "Words" {
                    json!({"k":"list","item":{"k":"u","bits":64}})
                } else {
                    value.clone()
                },
            )
        })
        .collect();
    assert_eq!(
        lint_roots(&changed, &roots, &acks)
            .stale_acknowledgements
            .len(),
        2
    );
    acks.insert("/blank".into(), "  ".into());
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    assert_eq!(
        graph.lint_bulk_roots(&roots, &acks),
        Err(GenerationError::Schema)
    );
    assert_eq!(
        graph.lint_bulk_roots(
            &BTreeMap::from([("Bad".into(), "Missing".into())]),
            &BTreeMap::new()
        ),
        Err(GenerationError::Schema)
    );
}
#[test]
fn advice_covers_primitive_and_zero_width_lists_without_flagging_bulk_fields() {
    let mut types: BTreeMap<String, Value> = BTreeMap::new();
    for (name, item) in [
        ("Bytes", json!({"k":"u","bits":8})),
        ("Wide", json!({"k":"i","bits":128})),
        ("Bool", json!({"k":"bool"})),
        ("F32", json!({"k":"float","bits":32})),
        ("Unit", json!({"k":"unit"})),
        ("EmptyRecord", json!({"k":"record","fields":[]})),
        ("EmptyTuple", json!({"k":"tuple","items":[]})),
        (
            "EmptyArray",
            json!({"k":"array","max":0,"item":{"k":"unit"}}),
        ),
        (
            "Hash",
            json!({"k":"newtype","item":{"k":"array","max":16,"item":{"k":"u","bits":8}}}),
        ),
    ] {
        types.insert(name.into(), json!({"k":"list","item":item}));
    }
    types.insert("Bulk".into(),json!({"k":"tuple","items":[{"k":"bytes"},{"k":"packed","word":"hash32"},{"k":"array","max":16,"item":{"k":"u","bits":8}}]}));
    let roots = types
        .keys()
        .map(|name| (name.clone(), name.clone()))
        .collect();
    let report = lint_roots(&types, &roots, &BTreeMap::new());
    assert_eq!(report.warnings.len(), 9);
    assert_eq!(report.to_string().lines().count(), 9);
    assert!(report.to_string().contains("1 more bulk diagnostics"));
    let acks = report
        .warnings
        .into_iter()
        .map(|w| {
            (
                w.path,
                "Retain explicit bound and measured small-value representation".into(),
            )
        })
        .collect();
    assert!(lint_roots(&types, &roots, &acks).is_clean());
}
#[test]
fn generation_requires_one_report_and_acknowledgements_do_not_change_source() {
    let graph = ValidatedRegistry::from_metadata(
        &BTreeMap::from([(
            "Data".into(),
            message_schema(json!({"k":"list","item":{"k":"u","bits":8}})),
        )]),
        SchemaLimits::default(),
    )
    .unwrap();
    let limits = PayloadLimits {
        bytes: 64,
        nodes: 64,
        depth: 8,
    };
    let mut calls = 0;
    let source = graph
        .payload_module(
            "data",
            "Data",
            "./runtime.ts",
            limits,
            &BTreeMap::new(),
            |report| {
                calls += 1;
                assert_eq!(report.warnings[0].path, "/types/Data/items/0/list/u8");
            },
        )
        .unwrap();
    assert_eq!(calls, 1);
    let acks = BTreeMap::from([(
        "/types/Data/items/0/list/u8".into(),
        "Bounded ordinary fixture".into(),
    )]);
    assert_eq!(
        source,
        graph
            .payload_module(
                "data",
                "Data",
                "./runtime.ts",
                limits,
                &acks,
                |report| assert!(report.is_clean())
            )
            .unwrap()
    );
    let methods = [Method {
        export: "upload",
        id: 7,
        request: "Data",
        response: "Data",
        request_limits: limits,
        response_limits: limits,
    }];
    let mut paths = Vec::new();
    graph
        .method_module(&methods, "./runtime.ts", &BTreeMap::new(), |report| {
            paths = report.warnings.into_iter().map(|w| w.path).collect()
        })
        .unwrap();
    assert_eq!(
        paths,
        [
            "/methods/upload/request/items/0/list/u8",
            "/methods/upload/response/items/0/list/u8"
        ]
    );
    let bad = BTreeMap::from([("/x".into(), " ".into())]);
    assert_eq!(
        graph.payload_module("data", "Data", "./runtime.ts", limits, &bad, |_| panic!(
            "failed generation called handler"
        )),
        Err(GenerationError::Schema)
    );
}
#[test]
fn composed_services_and_exponential_graphs_respect_work_limits() {
    let mut types = BTreeMap::from([("N0".into(), json!({"k":"unit"}))]);
    for level in 1..=15 {
        types.insert(format!("N{level}"),json!({"k":"tuple","items":[{"k":"ref","name":format!("N{}",level-1)},{"k":"ref","name":format!("N{}",level-1)}]}));
    }
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    for (level, count, accepted) in [(14, 1, true), (15, 1, false), (14, 16, false)] {
        let roots = (0..count)
            .map(|n| (format!("root{n}"), format!("N{level}")))
            .collect();
        if accepted {
            assert!(lint_roots(&types, &roots, &BTreeMap::new()).is_clean());
        } else {
            assert_eq!(
                graph.lint_bulk_roots(&roots, &BTreeMap::new()),
                Err(GenerationError::Complexity)
            );
        }
    }
    let types = BTreeMap::from([
        (
            "Leaf".into(),
            json!({"k":"record","fields":[{"name":"ids","value":{"k":"list","item":{"k":"u","bits":64}}},{"name":"next","value":{"k":"option","item":{"k":"ref","name":"Leaf"}}}]}),
        ),
        (
            "Page".into(),
            json!({"k":"tuple","items":vec![json!({"k":"ref","name":"Leaf"});25]}),
        ),
        (
            "Large".into(),
            json!({"k":"tuple","items":vec![json!({"k":"ref","name":"Page"});44]}),
        ),
    ]);
    let roots = (0..297)
        .flat_map(|n| {
            ["request", "response"].map(move |direction| {
                (
                    format!("m{n}/{direction}"),
                    if n == 0 && direction == "request" {
                        "Large"
                    } else {
                        "Page"
                    }
                    .into(),
                )
            })
        })
        .collect();
    assert_eq!(
        lint_roots(&types, &roots, &BTreeMap::new()).warnings.len(),
        15_925
    );
}

#[test]
fn diagnostic_paths_are_bounded_and_clean_handlers_are_not_skipped() {
    let graph = ValidatedRegistry::from_metadata(
        &BTreeMap::from([(
            "Empty".into(),
            json!({"k":"extension-tuple","items":[],"extensions":[]}),
        )]),
        SchemaLimits::default(),
    )
    .unwrap();
    for label in ["x".repeat(4096), "/".repeat(2048)] {
        assert_eq!(
            graph.lint_bulk_roots(&BTreeMap::from([(label, "Empty".into())]), &BTreeMap::new()),
            Err(GenerationError::Complexity)
        );
    }
    assert_eq!(
        graph.lint_bulk_roots(
            &BTreeMap::new(),
            &BTreeMap::from([("x".repeat(4097), "old path".into())])
        ),
        Err(GenerationError::Complexity)
    );
    let mut called = false;
    graph
        .payload_module(
            "empty",
            "Empty",
            "./runtime.ts",
            PayloadLimits {
                bytes: 1,
                nodes: 1,
                depth: 1,
            },
            &BTreeMap::new(),
            |report| {
                called = true;
                assert!(report.is_clean());
            },
        )
        .unwrap();
    assert!(called);
}

#[test]
fn advice_uses_reachable_list_bounds() {
    let words = json!({"k":"list","item":{"k":"u","bits":32},"max":3});
    let types = BTreeMap::from([(
        "Root".into(),
        json!({"k":"tuple","items":[
            {"k":"list","max":0,"item":{"k":"u","bits":32}},
            {"k":"array","max":0,"item":words},
            {"k":"list","max":1,"item":{"k":"array","max":0,"item":words}},
            words
        ]}),
    )]);
    let report = lint_roots(
        &types,
        &BTreeMap::from([("Reply".into(), "Root".into())]),
        &BTreeMap::new(),
    );
    let warnings: Vec<_> = report
        .warnings
        .iter()
        .map(|w| (w.path.as_str(), w.max_elements))
        .collect();
    assert_eq!(
        warnings,
        [
            ("/Reply/items/2/list/empty-array", Some(1)),
            ("/Reply/items/3/list/u32", Some(3))
        ]
    );
    assert!(
        report
            .to_string()
            .contains("/Reply/items/3/list/u32 (max 3 elements)")
    );
}

#[test]
fn bitmap_advice_depends_on_the_flag_count_bound() {
    let types = BTreeMap::from([(
        "Flags".into(),
        json!({"k":"tuple","items":[
            {"k":"list","max":2,"item":{"k":"bool"}},
            {"k":"list","max":3,"item":{"k":"bool"}},
            {"k":"list","item":{"k":"bool"}}
        ]}),
    )]);
    let report = lint_roots(
        &types,
        &BTreeMap::from([("Flags".into(), "Flags".into())]),
        &BTreeMap::new(),
    );
    let [two, three, unbounded] = report.warnings.as_slice() else {
        panic!("expected advice for each boolean list");
    };
    assert!(two.representation.contains("cannot save bytes"));
    assert!(
        three
            .representation
            .contains("compare a bounded flag count")
    );
    assert_eq!(three.representation, unbounded.representation);
}

#[test]
fn scalar_bounds_identify_one_byte_integer_lists() {
    for bits in [16, 32, 64] {
        for (item, fits) in [
            (json!({"k":"u","bits":bits,"max":127}), true),
            (json!({"k":"u","bits":bits,"max":128}), false),
            (json!({"k":"i","bits":bits,"min":-64,"max":63}), true),
            (json!({"k":"i","bits":bits,"min":-65,"max":63}), false),
            (json!({"k":"i","bits":bits,"min":-64,"max":64}), false),
            (json!({"k":"i","bits":bits,"max":63}), false),
            (json!({"k":"i","bits":bits,"min":-64}), false),
        ] {
            let types =
                BTreeMap::from([("Values".into(), json!({"k":"list","max":1,"item":item}))]);
            let report = lint_roots(
                &types,
                &BTreeMap::from([("Values".into(), "Values".into())]),
                &BTreeMap::new(),
            );
            assert_eq!(
                report.warnings[0]
                    .representation
                    .contains("one varint byte"),
                fits,
                "{item}"
            );
        }
    }
}

#[test]
fn hash_list_advice_accounts_for_the_first_byte_prefix_boundary() {
    for (width, last_single_byte) in [(16, 7), (32, 3)] {
        for max in [Some(last_single_byte), Some(last_single_byte + 1), None] {
            let mut list =
                json!({"k":"list","item":{"k":"array","max":width,"item":{"k":"u","bits":8}}});
            if let Some(max) = max {
                list["max"] = json!(max);
            }
            let report = lint_roots(
                &BTreeMap::from([("Hashes".into(), list)]),
                &BTreeMap::from([("Hashes".into(), "Hashes".into())]),
                &BTreeMap::new(),
            );
            assert_eq!(report.warnings.len(), 1);
            let warning = &report.warnings[0];
            assert_eq!(warning.path, format!("/Hashes/list/hash{width}"));
            assert_eq!(
                warning.representation.contains("same payload size"),
                max == Some(last_single_byte)
            );
        }
    }
}

#[test]
fn application_policy_matches_declarations_across_modules_and_bare_roots() {
    use wire::typescript::{BulkAdvisory, BulkPolicy, BulkSite, SchemaRegistry, Wire};

    #[derive(serde::Serialize, Wire)]
    #[wire(crate = "wire", fixed)]
    struct Identifier([u8; 16]);
    #[derive(serde::Serialize, Wire)]
    #[wire(crate = "wire", fixed)]
    struct Other([u8; 16]);
    #[derive(serde::Serialize, Wire)]
    #[wire(crate = "wire", fixed)]
    struct Policy {
        unlocked_rarity_tiers: Vec<u8>,
        cursor: Vec<u8>,
    }

    let mut schemas = SchemaRegistry::new(Default::default()).unwrap();
    schemas.register::<Vec<Identifier>>("Ids").unwrap();
    schemas.register::<Vec<Other>>("OtherIds").unwrap();
    schemas.register::<Policy>("Policy").unwrap();
    let types = serde_json::from_str(&schemas.finish().unwrap().metadata_json().unwrap()).unwrap();
    let graph =
        ValidatedRegistry::from_metadata(&grouped_messages(types), Default::default()).unwrap();
    let baseline = graph.metadata_json().unwrap();
    let mut policy = BulkPolicy::default();
    policy
        .acknowledge(
            BulkSite::list_element::<Identifier>(),
            BulkAdvisory::PackedList,
            "Keep typed IDs",
        )
        .unwrap();
    let tiers = BulkSite::field::<Policy>("unlocked_rarity_tiers");
    assert_eq!(
        policy.acknowledge(tiers.clone(), BulkAdvisory::OpaqueBytes, " "),
        Err(GenerationError::Schema)
    );
    policy
        .acknowledge(
            tiers.clone(),
            BulkAdvisory::OpaqueBytes,
            "Numeric tier values",
        )
        .unwrap();
    assert_eq!(
        policy.acknowledge(tiers.clone(), BulkAdvisory::OpaqueBytes, "duplicate"),
        Err(GenerationError::Schema)
    );

    let limits = PayloadLimits {
        bytes: 200,
        nodes: 400,
        depth: 10,
    };
    for (req, rep, remaining) in [
        ("MessageIds", "MessagePolicy", 1),
        ("MessageOtherIds", "MessageIds", 1),
    ] {
        let methods = [Method {
            export: "codec",
            id: 1,
            request: req,
            response: rep,
            request_limits: limits,
            response_limits: limits,
        }];
        let render = |f: &mut dyn FnMut(wire::typescript::BulkLintReport)| {
            graph
                .method_module(&methods, "./runtime.ts", &Default::default(), f)
                .unwrap()
        };
        let original = render(&mut |_| {});
        let filtered = render(&mut |mut report| {
            policy.apply(&mut report);
            assert_eq!(report.warnings.len(), remaining);
            let w = &report.warnings[0];
            if req == "MessageIds" {
                assert!(w.path.ends_with("/fields/cursor/list/u8"));
            } else {
                assert_eq!(w.advisory, BulkAdvisory::PackedList);
                assert!(w.sites.contains(&BulkSite::list_element::<Other>()));
            }
        });
        assert_eq!(original, filtered);
    }
    assert_eq!(baseline, graph.metadata_json().unwrap());
    assert_eq!(policy.stale().count(), 0);

    // Same serialized field, a different advisory after a declaration change.
    let mut definitions: BTreeMap<String, Value> = serde_json::from_str(&baseline).unwrap();
    definitions
        .get_mut(&SchemaRegistry::type_label::<Policy>())
        .unwrap()["fields"][0]["value"] = json!({"k":"list","item":{"k":"u","bits":16}});
    let changed = ValidatedRegistry::from_metadata(&definitions, Default::default()).unwrap();
    let mut report = changed
        .lint_bulk_roots(
            &[("reply".into(), "Policy".into())].into(),
            &Default::default(),
        )
        .unwrap();
    let mut next_export = BulkPolicy::default();
    next_export
        .acknowledge(
            tiers.clone(),
            BulkAdvisory::OpaqueBytes,
            "Numeric tier values",
        )
        .unwrap();
    next_export.apply(&mut report);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.sites.contains(&tiers) && w.advisory == BulkAdvisory::PackedList)
    );
    assert_eq!(
        next_export.stale().collect::<Vec<_>>(),
        [(&tiers, BulkAdvisory::OpaqueBytes, "Numeric tier values")]
    );
}
