//! Derived metadata goes through the same graph admission as explicit schemas.
#![cfg(all(feature = "derive", feature = "typescript"))]
use ic_wire::typescript::{GenerationError, SchemaContext, SchemaLimits, ValidatedRegistry, Wire};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Unit;
#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Newtype(u16);
#[derive(Serialize, Deserialize, Wire)]
enum Choice {
    Empty,
    Value(u16),
    Record { flag: bool },
}
#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Record {
    r#type: u8,
    choice: Choice,
    ordinary: Vec<u8>,
    bytes: ic_wire::Bytes<3>,
}
#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Recursive {
    next: Option<Box<Recursive>>,
}

#[test]
fn derives_exact_ordinary_layouts_without_changing_serde() {
    let graph = ValidatedRegistry::from_type::<Record>("Record", SchemaLimits::default()).unwrap();
    let metadata: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(
        metadata["Record"],
        json!({"k":"record","fixed":true,"fields":[
            {"name":"type","value":{"k":"u","bits":8}},
            {"name":"choice","value":{"k":"enum","variants":[
                {"name":"Empty","id":0,"value":{"k":"unit"}},
                {"name":"Value","id":1,"value":{"k":"u","bits":16}},
                {"name":"Record","id":2,"value":{"k":"record","fields":[{"name":"flag","value":{"k":"bool"}}]}}
            ]}},
            {"name":"ordinary","value":{"k":"list","item":{"k":"u","bits":8}}},
            {"name":"bytes","value":{"k":"bytes","max":3}}
        ]})
    );
    for (name, actual, expected) in [
        (
            "Unit",
            ValidatedRegistry::from_type::<Unit>("Unit", SchemaLimits::default()).unwrap(),
            json!({"k":"unit"}),
        ),
        (
            "Newtype",
            ValidatedRegistry::from_type::<Newtype>("Newtype", SchemaLimits::default()).unwrap(),
            json!({"k":"newtype","item":{"k":"u","bits":16}}),
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<Value>(&actual.metadata_json().unwrap()).unwrap()[name],
            expected
        );
    }
}

#[test]
fn construction_limits_and_recursion_refuse_without_partial_registry() {
    assert_eq!(
        ValidatedRegistry::from_type::<Recursive>("Recursive", SchemaLimits::default())
            .unwrap_err(),
        GenerationError::Schema
    );
    assert_eq!(
        ValidatedRegistry::from_type::<Record>(
            "Record",
            SchemaLimits {
                nodes: 1,
                ..SchemaLimits::default()
            }
        )
        .unwrap_err(),
        GenerationError::Complexity
    );
    assert_eq!(
        ValidatedRegistry::from_type::<Record>(
            "Record",
            SchemaLimits {
                depth: 0,
                ..SchemaLimits::default()
            }
        )
        .unwrap_err(),
        GenerationError::Complexity
    );
    assert!(
        ValidatedRegistry::from_type::<bool>(
            "Flag",
            SchemaLimits {
                nodes: 2,
                depth: 0,
                ..SchemaLimits::default()
            }
        )
        .is_ok()
    );
}

struct SwallowsFailure;
impl Wire for SwallowsFailure {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        let _error = context.describe::<Recursive>();
        Ok(json!({"k":"bool"}))
    }
}

#[test]
fn manual_projection_cannot_hide_a_nested_admission_failure() {
    assert_eq!(
        ValidatedRegistry::from_type::<SwallowsFailure>("Root", SchemaLimits::default())
            .unwrap_err(),
        GenerationError::Schema
    );
}

#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Generic<Result: Clone = u16, const N: usize = 3>
where
    Result: PartialEq,
{
    value: Result,
    bytes: ic_wire::Bytes<N>,
}

#[derive(Serialize, Deserialize, Wire)]
enum GenericChoice<T> {
    Empty,
    Value(T),
    Named { value: T },
}

#[derive(Serialize, Deserialize, Wire)]
#[wire(fixed)]
struct Borrowed<'a, T, const N: usize> {
    text: &'a str,
    #[serde(borrow = "'a")]
    bytes: ic_wire::BytesRef<'a, N>,
    value: T,
}

#[test]
fn concrete_generics_preserve_bounds_defaults_and_variant_layout() {
    fn metadata<T: Wire>() -> Value {
        serde_json::from_str(
            &ValidatedRegistry::from_type::<T>("Root", SchemaLimits::default())
                .unwrap()
                .metadata_json()
                .unwrap(),
        )
        .unwrap()
    }
    let small = metadata::<Generic>();
    let large = metadata::<Generic<u64, 16>>();
    assert_eq!(
        small["Root"]["fields"][0]["value"],
        json!({"k":"u","bits":16})
    );
    assert_eq!(
        large["Root"]["fields"][0]["value"],
        json!({"k":"u","bits":64})
    );
    assert_eq!(small["Root"]["fields"][1]["value"]["max"], 3);
    assert_eq!(large["Root"]["fields"][1]["value"]["max"], 16);
    let choice = metadata::<GenericChoice<u16>>();
    assert_eq!(
        choice["Root"]["variants"][1]["value"],
        json!({"k":"u","bits":16})
    );
    assert_eq!(
        choice["Root"]["variants"][2]["value"]["fields"][0]["value"],
        json!({"k":"u","bits":16})
    );
}

#[test]
fn borrowed_generic_projection_accepts_nonstatic_lifetimes_and_preserves_custody() {
    use ic_wire::{Encoding, Limits, decode};

    #[derive(Wire)]
    struct Message<'a>(Borrowed<'a, u16, 3>);

    fn project<'a>(_: &'a str) -> ValidatedRegistry {
        let mut named = ic_wire::typescript::SchemaRegistry::new(SchemaLimits::default()).unwrap();
        named.register::<Borrowed<'a, u16, 3>>("Borrowed").unwrap();
        assert!(named.finish().is_ok());
        ValidatedRegistry::from_type::<Borrowed<'a, u16, 3>>("Borrowed", SchemaLimits::default())
            .unwrap()
    }
    let text = String::from("hi");
    let graph = project(&text);
    let value: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(value["Borrowed"]["fields"][0]["value"], json!({"k":"text"}));
    assert_eq!(
        value["Borrowed"]["fields"][1]["value"],
        json!({"k":"bytes","max":3})
    );
    let literal = [2, b'h', b'i', 2, 170, 187, 128, 1, 0];
    let Message(decoded) = decode(
        &literal,
        Encoding::Payload,
        Limits {
            bytes: 128,
            allocation: 32768,
            nodes: 512,
            depth: 32,
        },
    )
    .unwrap();
    assert_eq!(decoded.text, "hi");
    assert_eq!(decoded.value, 128);
    assert_eq!(decoded.text.as_ptr(), literal[1..].as_ptr());
    assert_eq!(decoded.bytes.as_slice().as_ptr(), literal[4..].as_ptr());
}

#[derive(Serialize, Wire)]
#[wire(fixed)]
struct DuplicateNames {
    #[serde(rename = "same")]
    first: u8,
    #[serde(rename = "same")]
    second: u8,
}

#[test]
fn renamed_field_collisions_fail_graph_admission() {
    assert_eq!(
        ValidatedRegistry::from_type::<DuplicateNames>("Root", SchemaLimits::default())
            .unwrap_err(),
        GenerationError::Schema
    );
}

#[test]
fn named_collection_handles_recursion_and_is_order_independent() {
    use ic_wire::typescript::SchemaRegistry;
    let mut first = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    first.register::<Recursive>("Recursive").unwrap();
    first.register::<Generic<u64, 16>>("Generic").unwrap();
    let mut second = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    second.register::<Generic<u64, 16>>("Generic").unwrap();
    second.register::<Recursive>("Recursive").unwrap();
    assert_eq!(
        first.finish().unwrap().metadata_json().unwrap(),
        second.finish().unwrap().metadata_json().unwrap()
    );
}

#[test]
fn named_collection_rejects_duplicate_roots_and_stays_failed() {
    use ic_wire::typescript::SchemaRegistry;
    let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    registry.register::<bool>("Root").unwrap();
    assert_eq!(
        registry.register::<bool>("Root"),
        Err(GenerationError::Schema)
    );
    assert_eq!(
        registry.register::<u8>("Other"),
        Err(GenerationError::Schema)
    );
    assert_eq!(registry.finish().unwrap_err(), GenerationError::Schema);
    let mut small = SchemaRegistry::new(SchemaLimits {
        nodes: 0,
        ..SchemaLimits::default()
    })
    .unwrap();
    assert_eq!(
        small.register::<bool>("Root"),
        Err(GenerationError::Complexity)
    );
    assert_eq!(small.finish().unwrap_err(), GenerationError::Schema);
    let mut exact = SchemaRegistry::new(SchemaLimits {
        nodes: 4,
        ..SchemaLimits::default()
    })
    .unwrap();
    exact.register::<bool>("Root").unwrap();
    let graph = exact.finish().unwrap();
    let metadata: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    let generated = metadata["Root"]["name"].as_str().unwrap();
    let mut collision = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    assert_eq!(
        collision.register::<bool>(generated),
        Err(GenerationError::Schema)
    );
}

struct AliasCycle;
impl Wire for AliasCycle {
    fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
        context.describe::<Self>()
    }
}

#[test]
fn named_collection_rejects_alias_cycles_and_unstable_projections() {
    use ic_wire::typescript::SchemaRegistry;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    struct Unstable;
    impl Wire for Unstable {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
                json!({"k":"bool"})
            } else {
                json!({"k":"unit"})
            })
        }
    }
    let mut cyclic = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    cyclic.register::<AliasCycle>("Root").unwrap();
    assert_eq!(cyclic.finish().unwrap_err(), GenerationError::Schema);
    let mut unstable = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    unstable.register::<Unstable>("Root").unwrap();
    assert_eq!(unstable.finish().unwrap_err(), GenerationError::Schema);
}

#[test]
fn distinct_types_with_identical_diagnostic_names_are_rechecked() {
    use ic_wire::typescript::SchemaRegistry;
    let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    let first_name = {
        #[derive(Serialize, Wire)]
        #[wire(fixed)]
        struct Same {
            value: bool,
        }
        registry.register::<Same>("First").unwrap();
        std::any::type_name::<Same>()
    };
    let second_name = {
        #[derive(Serialize, Wire)]
        #[wire(fixed)]
        struct Same {
            value: u8,
        }
        registry.register::<Same>("Second").unwrap();
        std::any::type_name::<Same>()
    };
    assert_eq!(first_name, second_name);
    assert_eq!(registry.finish().unwrap_err(), GenerationError::Schema);
}

#[test]
fn many_public_roots_share_one_definition_with_exact_graph_admission() {
    use ic_wire::typescript::SchemaRegistry;
    const ROOTS: usize = 1024;
    // Each public alias and the one shared bool definition contributes a named
    // definition and one inline node. Aliases must not clone the shared shape.
    let exact = SchemaLimits {
        nodes: 2 * (ROOTS + 1),
        ..SchemaLimits::default()
    };
    let collect = |reverse: bool, limits| {
        let mut registry = SchemaRegistry::new(limits).unwrap();
        for offset in 0..ROOTS {
            let index = if reverse { ROOTS - offset - 1 } else { offset };
            registry
                .register::<bool>(&format!("Root{index:04}"))
                .unwrap();
        }
        registry.finish()
    };
    let forward = collect(false, exact).unwrap();
    let reverse = collect(true, exact).unwrap();
    let metadata = forward.metadata_json().unwrap();
    assert_eq!(metadata, reverse.metadata_json().unwrap());
    assert_eq!(
        forward.declarations().unwrap(),
        reverse.declarations().unwrap()
    );
    let definitions: std::collections::BTreeMap<String, Value> =
        serde_json::from_str(&metadata).unwrap();
    assert_eq!(definitions.len(), ROOTS + 1);
    let target = definitions["Root0000"]["name"].as_str().unwrap();
    assert_eq!(definitions[target], json!({"k":"bool"}));
    for index in 0..ROOTS {
        assert_eq!(
            definitions[&format!("Root{index:04}")],
            json!({"k":"ref","name":target})
        );
    }
    assert!(matches!(
        collect(
            false,
            SchemaLimits {
                nodes: exact.nodes - 1,
                ..exact
            }
        ),
        Err(GenerationError::Complexity)
    ));
}

#[test]
fn many_recursive_roots_keep_a_shared_finite_graph() {
    use ic_wire::typescript::SchemaRegistry;
    let collect = |roots: usize| {
        let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
        for index in 0..roots {
            registry
                .register::<Recursive>(&format!("Root{index:04}"))
                .unwrap();
        }
        registry.finish().unwrap()
    };
    let baseline: std::collections::BTreeMap<String, Value> =
        serde_json::from_str(&collect(1).metadata_json().unwrap()).unwrap();
    let large = collect(1024);
    let expanded: std::collections::BTreeMap<String, Value> =
        serde_json::from_str(&large.metadata_json().unwrap()).unwrap();
    assert_eq!(expanded.len(), baseline.len() + 1023);
    for (name, definition) in &baseline {
        assert_eq!(&expanded[name], definition);
    }
    for index in 1..1024 {
        assert_eq!(expanded[&format!("Root{index:04}")], baseline["Root0000"]);
    }
    assert!(large.declarations().unwrap().len() < 128 * 1024);
}
