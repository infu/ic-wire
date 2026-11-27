//! Public exporter qualification for IC Wire #4.
use crate::{message_schema, support::encode};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wire::typescript::{
    Method, PayloadLimits, SchemaLimits, SchemaRegistry, ValidatedRegistry, Wire,
};

#[derive(Serialize, Wire)]
#[wire(crate = "wire")]
enum Authority {
    User,
    Admin,
}

#[derive(Serialize, Wire)]
#[wire(crate = "wire")]
enum ChangedAuthority {
    Admin,
    User,
}

fn method(export: &str) -> Method<'_> {
    let limits = PayloadLimits {
        bytes: 128,
        nodes: 32,
        depth: 8,
    };

    Method {
        export,
        id: wire::method_id(export),
        request: "Request",
        response: "Reply",
        request_limits: limits,
        response_limits: limits,
    }
}

// Keep public names stable across two independently compiled versions of the DTO.
fn graph<T: Wire>(swap_fields: bool) -> ValidatedRegistry {
    let authority =
        ValidatedRegistry::from_type::<T>("Authority", SchemaLimits::default()).unwrap();
    let mut types: BTreeMap<String, Value> =
        serde_json::from_str(&authority.metadata_json().unwrap()).unwrap();
    let mut fields = vec![
        json!({"name":"authority","value":{"k":"ref","name":"Authority"}}),
        json!({"name":"enabled","value":{"k":"bool"}}),
    ];
    if swap_fields {
        fields.reverse();
    }
    types.insert(
        "Reply".into(),
        json!({"k":"extension-record","fields":fields,"extensions":[]}),
    );
    types.insert(
        "Request".into(),
        json!({"k":"extension-tuple","items":[],"extensions":[]}),
    );
    types.insert("Unused".into(), json!({"k":"bool"}));

    ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap()
}

#[test]
fn nested_tag_and_field_changes_are_visible_with_the_same_routing_id() {
    let endpoint = "system_get_user_authority";
    let methods = [(endpoint, method(endpoint))];
    let snapshot = |graph: ValidatedRegistry| {
        let docs = graph.api_manifests(&methods).unwrap();
        serde_json::from_str::<Value>(&docs[endpoint]).unwrap()
    };
    assert_eq!(
        graph::<Authority>(false).api_manifests(&methods).unwrap()[endpoint],
        include_str!("system_get_user_authority.json")
    );
    let before = snapshot(graph::<Authority>(false));
    let after = snapshot(graph::<ChangedAuthority>(false));
    assert_eq!(before["id"], after["id"]);
    assert_eq!(before["response"]["root"], "Reply");
    assert_eq!(
        before["schemas"]["Reply"]["fields"][0]["value"]["name"],
        "Authority"
    );
    assert!(before["schemas"].get("Unused").is_none());
    let tags = |doc: &Value| {
        doc["schemas"]["Authority"]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["name"].as_str().unwrap().to_owned(),
                    v["id"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(tags(&before), [("User".into(), 0), ("Admin".into(), 1)]);
    assert_eq!(tags(&after), [("Admin".into(), 0), ("User".into(), 1)]);
    let swapped = snapshot(graph::<Authority>(true));
    assert_eq!(swapped["schemas"]["Reply"]["fields"][0]["name"], "enabled");
    // Independent byte oracle, including the two versions' opposite meanings.
    let limits = wire::Limits {
        bytes: 8,
        nodes: 32,
        allocation: 4096,
        depth: 8,
    };
    for (old, new, tag) in [
        (Authority::User, ChangedAuthority::Admin, 0),
        (Authority::Admin, ChangedAuthority::User, 1),
    ] {
        assert_eq!(
            encode(&old, wire::Encoding::Payload, limits).unwrap(),
            [tag, 0]
        );
        assert_eq!(
            encode(&new, wire::Encoding::Payload, limits).unwrap(),
            [tag, 0]
        );
    }
}

#[test]
fn registration_order_is_stable_and_removing_a_method_only_removes_its_document() {
    let collect = |reverse| {
        let mut registry = SchemaRegistry::new(SchemaLimits::default()).unwrap();
        if reverse {
            registry.register::<Option<Authority>>("ReplyBody").unwrap();
            registry.register::<()>("RequestBody").unwrap();
        } else {
            registry.register::<()>("RequestBody").unwrap();
            registry.register::<Option<Authority>>("ReplyBody").unwrap();
        }
        let mut types: BTreeMap<String, Value> =
            serde_json::from_str(&registry.finish().unwrap().metadata_json().unwrap()).unwrap();
        types.insert(
            "Request".into(),
            message_schema(json!({"k":"ref","name":"RequestBody"})),
        );
        types.insert(
            "Reply".into(),
            message_schema(json!({"k":"ref","name":"ReplyBody"})),
        );
        ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap()
    };
    let a = ("read_role", method("getRole"));
    let b = ("read_role_again", method("getRoleAgain"));
    let baseline = collect(false).api_manifests(&[a, b]).unwrap();
    assert_eq!(baseline, collect(true).api_manifests(&[b, a]).unwrap());
    let one = collect(false).api_manifests(&[a]).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[a.0], baseline[a.0]);
    assert!(collect(false).api_manifests(&[a, a]).is_err());
}

#[test]
fn recursive_references_terminate_and_unrelated_definitions_are_omitted() {
    let types = BTreeMap::from([
        (
            "Request".into(),
            json!({"k":"extension-tuple","items":[],"extensions":[]}),
        ),
        (
            "Reply".into(),
            message_schema(json!({"k":"ref","name":"Chain"})),
        ),
        (
            "Chain".into(),
            json!({"k":"option","item":{"k":"ref","name":"Chain"}}),
        ),
        ("Other".into(), json!({"k":"bool"})),
    ]);
    let registry = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let mut endpoint = method("read");
    let export = endpoint.export;
    let emit = |method| registry.api_manifests(&[(export, method)]).unwrap();
    let original = emit(endpoint);
    let doc: Value = serde_json::from_str(&original[export]).unwrap();
    assert_eq!(doc["schemas"].as_object().unwrap().len(), 3);
    assert_eq!(doc["schemas"]["Reply"], types["Reply"]);
    endpoint.response_limits.bytes += 1;
    assert_ne!(emit(endpoint), original);
    endpoint.response = "Missing";
    assert!(registry.api_manifests(&[(export, endpoint)]).is_err());
}

#[test]
fn combined_output_budget_and_method_admission_are_enforced() {
    let types = BTreeMap::from([
        (
            "Request".into(),
            json!({"k":"extension-tuple","items":[],"extensions":[]}),
        ),
        ("Reply".into(), message_schema(json!({"k":"bool"}))),
    ]);
    let registry = |output_bytes| {
        ValidatedRegistry::from_metadata(
            &types,
            SchemaLimits {
                output_bytes,
                ..SchemaLimits::default()
            },
        )
        .unwrap()
    };
    let endpoints = [("first", method("a")), ("second", method("b"))];
    let docs = registry(8192).api_manifests(&endpoints).unwrap();
    let exact = docs.values().map(String::len).sum::<usize>();
    assert_eq!(registry(exact).api_manifests(&endpoints).unwrap(), docs);
    assert_eq!(
        registry(exact - 1).api_manifests(&endpoints),
        Err(wire::typescript::GenerationError::Complexity)
    );
    assert!(registry(0).api_manifests(&[]).unwrap().is_empty());
    let mut bad = method("second");
    bad.response_limits.depth = 0;
    assert!(registry(exact).api_manifests(&[("second", bad)]).is_err());
    for duplicate in [
        ("first", method("b")),
        (
            "second",
            Method {
                id: endpoints[0].1.id,
                ..method("b")
            },
        ),
        (
            "second",
            Method {
                export: "a",
                ..method("b")
            },
        ),
    ] {
        assert!(
            registry(exact)
                .api_manifests(&[endpoints[0], duplicate])
                .is_err()
        );
    }
}
