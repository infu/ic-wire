//! Offline checks for chronological API manifest baselines.
#![forbid(unsafe_code)]

mod compatibility;
#[allow(
    dead_code,
    reason = "Only the fixture declarations are needed to export manifests"
)]
mod fixture;
mod manifest_check;
#[allow(
    dead_code,
    reason = "The upgrade fixture can select request and response independently"
)]
mod schema;

use compatibility::Compatibility;
use manifest_check::Service;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};
use wire::typescript::{Method, PayloadLimits, SchemaRegistry, Wire};

const USAGE: &str = "Usage: evolution-upgrade check BASELINE_DIR [BASELINE_DIR ...] CANDIDATE_DIR\n       evolution-upgrade fixture A|B|C OUTPUT_DIR\n\nList supported baselines in chronological order. Each directory contains per-method API manifests and modes.json. Add an optional retired.json sidecar for stable extension tombstones.";

fn method_manifest<Request: Wire, Response: Wire>() -> Result<String, String> {
    let mut registry =
        SchemaRegistry::new(Default::default()).map_err(|error| format!("{error:?}"))?;
    registry
        .register::<Request>("Request")
        .map_err(|error| format!("{error:?}"))?;
    registry
        .register::<Response>("Reply")
        .map_err(|error| format!("{error:?}"))?;
    let registry = registry.finish().map_err(|error| format!("{error:?}"))?;
    let limits = PayloadLimits {
        bytes: (fixture::LIMITS.bytes - 11) as u64,
        nodes: fixture::LIMITS.nodes as u64,
        depth: fixture::LIMITS.depth as u64,
    };
    let method = Method {
        export: "packet",
        id: fixture::METHOD,
        request: "Request",
        response: "Reply",
        request_limits: limits,
        response_limits: limits,
    };
    let mut manifests = registry
        .api_manifests(&[("packet", method)])
        .map_err(|error| format!("{error:?}"))?;
    manifests
        .remove("packet")
        .ok_or_else(|| "missing packet manifest".into())
}

fn fixture_manifest<const REQUEST: u8, const RESPONSE: u8>() -> Result<String, String> {
    use fixture::{Batch, Row};

    method_manifest::<Batch<'_, Row<'_, REQUEST>>, Batch<'_, Row<'_, RESPONSE>>>()
}

fn write_fixture(revision: &str, directory: &Path) -> Result<(), String> {
    let manifest = match revision {
        "A" => fixture_manifest::<0, 0>(),
        "B" => fixture_manifest::<1, 1>(),
        "C" => fixture_manifest::<2, 2>(),
        _ => return Err("expected fixture revision A, B or C".into()),
    }?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    for (name, contents) in [
        ("packet.json", manifest.as_str()),
        ("modes.json", "{\"packet\":\"update\"}\n"),
    ] {
        let path = directory.join(name);
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(contents.as_bytes()))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn run() -> Result<ExitCode, String> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|value| value.to_str()) {
        Some("check") => {
            let mut paths: Vec<_> = args.map(PathBuf::from).collect();
            let path = paths.pop().ok_or(USAGE)?;
            if paths.is_empty() {
                return Err(USAGE.into());
            }
            if paths.len() > manifest_check::MAX_HISTORY {
                return Err("too many baseline directories".into());
            }
            let history = paths
                .iter()
                .map(|path| Service::load(path))
                .collect::<Result<Vec<_>, _>>()?;
            let current = Service::load(&path)?;
            match manifest_check::check(&history, &current)? {
                Compatibility::Preserved => println!("Compatible. Declared values are preserved."),
                Compatibility::Drops(paths) => {
                    println!("Compatible with data dropped at these paths.");
                    for path in paths {
                        println!("  {path}");
                    }
                }
                Compatibility::Breaking { path, reason } => {
                    eprintln!("Breaking at {path}. {reason}.");
                    return Ok(ExitCode::FAILURE);
                }
                Compatibility::Unsupported { path, reason } => {
                    return Err(format!("Cannot check {path}. {reason}."));
                }
            }
        }
        Some("fixture") => {
            let revision = args
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or(USAGE)?;
            let directory = args.next().ok_or(USAGE)?;
            if args.next().is_some() {
                return Err(USAGE.into());
            }
            write_fixture(&revision, Path::new(&directory))?;
        }
        Some("--help" | "-h") => println!("{USAGE}"),
        _ => return Err(USAGE.into()),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wire::evolution::{Evolve, ExtensionSlot, OpenSlot, RecoverSlot};
    use wire::typescript::__private::{Value, json};
    use wire::typescript::{GenerationError, SchemaContext};

    // These fixtures compare declarations inside the same sparse message root.
    struct Message<T>(std::marker::PhantomData<T>);

    impl<T: Wire> Wire for Message<T> {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({
                "k": "extension-tuple",
                "items": [context.describe::<T>()?],
                "extensions": []
            }))
        }
    }

    struct EmptyMessage;

    impl Wire for EmptyMessage {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            Ok(json!({"k": "extension-tuple", "items": [], "extensions": []}))
        }
    }

    struct ClosedChoice<const REV: u8>;

    impl<const REV: u8> Wire for ClosedChoice<REV> {
        fn schema(_: &mut SchemaContext) -> Result<Value, GenerationError> {
            let mut variants = vec![
                json!({"name":"Ready","id":5,"value":{"k":"unit"}}),
                json!({"name":"Paused","id":u32::MAX,"value":{"k":"bool"}}),
            ];
            match REV {
                1 => variants.reverse(),
                2 => variants[1]["id"] = json!(5),
                3 => variants[1]["id"] = json!(u32::MAX - 1),
                _ => (),
            }
            Ok(json!({"k":"enum","variants":variants}))
        }
    }

    struct WideTuple;

    impl Wire for WideTuple {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            let byte = context.describe::<u8>()?;
            Ok(json!({"k":"tuple","items":vec![byte; 192]}))
        }
    }

    struct RepeatedWideTuple;

    impl Wire for RepeatedWideTuple {
        fn schema(context: &mut SchemaContext) -> Result<Value, GenerationError> {
            let inner = context.describe::<WideTuple>()?;
            Ok(json!({"k":"tuple","items":vec![inner; 16]}))
        }
    }

    #[derive(Evolve)]
    #[wire(crate = "wire")]
    enum OldState {
        #[wire(id = 1)]
        Ready(u8),
    }

    #[derive(Evolve)]
    #[wire(crate = "wire")]
    enum NewState {
        #[wire(id = 1)]
        Ready(u8),
        #[wire(id = 9)]
        Future,
    }

    #[derive(Evolve)]
    #[wire(crate = "wire")]
    enum IncompatibleState {
        #[wire(id = 1)]
        Ready(u16),
        #[wire(id = 9)]
        Future,
    }

    #[derive(Evolve)]
    #[wire(crate = "wire")]
    struct OldExtensions {}

    #[allow(
        dead_code,
        reason = "Only the Wire declaration is needed for this test"
    )]
    #[derive(Evolve)]
    #[wire(crate = "wire")]
    struct NewExtensions {
        #[wire(id = 7, required)]
        epoch: u32,
    }

    #[allow(
        dead_code,
        reason = "Only the Wire declaration is needed for this test"
    )]
    #[derive(Wire)]
    #[wire(crate = "wire")]
    struct OldExtended {
        core: bool,
        #[wire(extensions)]
        extension: ExtensionSlot<'static, OldExtensions>,
    }

    #[allow(
        dead_code,
        reason = "Only the Wire declaration is needed for this test"
    )]
    #[derive(Wire)]
    #[wire(crate = "wire")]
    struct NewExtended {
        core: bool,
        #[wire(extensions)]
        extension: ExtensionSlot<'static, NewExtensions>,
    }

    fn read_fixture<const REQUEST: u8, const RESPONSE: u8>() -> Value {
        fixture_manifest::<REQUEST, RESPONSE>()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn service(document: Value, mode: &str, retired: Value) -> Service {
        Service::parse(vec![document], json!({"packet": mode}), retired).unwrap()
    }

    fn emitted<Request: Wire, Response: Wire>() -> Service {
        let document = method_manifest::<Request, Response>()
            .unwrap()
            .parse()
            .unwrap();
        service(document, "update", json!({}))
    }

    fn emitted_evolving<
        Request: wire::evolution::__private::EvolvingSchema,
        Reply: wire::evolution::__private::EvolvingSchema,
    >() -> Service {
        let method = wire::evolution::Method::<Request, Reply>::new(
            "packet",
            fixture::LIMITS,
            fixture::LIMITS,
        );
        let limits = PayloadLimits {
            bytes: 128,
            nodes: 256,
            depth: 16,
        };
        let mut registry = SchemaRegistry::new(Default::default()).unwrap();
        let codec = registry
            .register_evolving_method(&method, "packet", ("Request", limits), ("Reply", limits))
            .unwrap();
        let manifests = registry
            .finish()
            .unwrap()
            .api_manifests(&[("packet", codec)])
            .unwrap();
        service(manifests["packet"].parse().unwrap(), "update", json!({}))
    }

    #[test]
    fn inline_additions_require_an_id_to_leave_the_positional_core_unchanged() {
        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct First {
            count: u16,
        }

        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct TaggedAddition {
            count: u16,
            #[wire(id = 7)]
            cached: Option<bool>,
        }

        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct UntaggedAddition {
            count: u16,
            cached: Option<bool>,
        }

        assert_eq!(
            manifest_check::check(
                &[emitted_evolving::<First, First>()],
                &emitted_evolving::<TaggedAddition, TaggedAddition>()
            )
            .unwrap(),
            Compatibility::Drops(vec!["$.packet.response.cached#7".into()]),
        );
        assert!(matches!(
            manifest_check::check(
                &[emitted_evolving::<First, First>()],
                &emitted_evolving::<UntaggedAddition, UntaggedAddition>()
            )
            .unwrap(),
            Compatibility::Breaking { .. },
        ));
    }

    #[test]
    fn nested_evolution_is_checked_in_the_method_direction() {
        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct First {
            count: u16,
        }

        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct Optional {
            count: u16,
            #[wire(id = 7)]
            cached: Option<bool>,
        }

        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct Required {
            count: u16,
            #[wire(id = 7, required)]
            cached: bool,
        }

        #[derive(wire::Wire)]
        #[wire(crate = "wire")]
        struct Batch<T> {
            items: wire::List<Option<T>, 4>,
        }

        type V1 = Batch<First>;
        type V2 = Batch<Optional>;
        type V3 = Batch<Required>;
        let baseline = [emitted_evolving::<V1, V1>()];
        let dropped = Compatibility::Drops(vec!["$.packet.response.items[].some.cached#7".into()]);

        assert_eq!(
            manifest_check::check(&baseline, &emitted_evolving::<V2, V2>()).unwrap(),
            dropped,
        );
        assert_eq!(
            manifest_check::check(&baseline, &emitted_evolving::<V2, V1>()).unwrap(),
            Compatibility::Preserved,
        );
        assert_eq!(
            manifest_check::check(&baseline, &emitted_evolving::<V1, V3>()).unwrap(),
            dropped,
        );
        expect_break(
            manifest_check::check(&baseline, &emitted_evolving::<V3, V1>()).unwrap(),
            "$.packet.request.items[].some.cached#7",
        );

        let optional_baseline = [emitted_evolving::<V2, V2>()];
        assert_eq!(
            manifest_check::check(&optional_baseline, &emitted_evolving::<V2, V3>()).unwrap(),
            Compatibility::Preserved,
        );
        expect_break(
            manifest_check::check(&optional_baseline, &emitted_evolving::<V3, V2>()).unwrap(),
            "$.packet.request.items[].some.cached#7",
        );

        let required_baseline = [emitted_evolving::<V3, V3>()];
        assert_eq!(
            manifest_check::check(&required_baseline, &emitted_evolving::<V2, V3>()).unwrap(),
            Compatibility::Preserved,
        );
        expect_break(
            manifest_check::check(&required_baseline, &emitted_evolving::<V3, V2>()).unwrap(),
            "$.packet.response.items[].some.cached#7",
        );
    }

    fn expect_break(report: Compatibility, path: &str) {
        let Compatibility::Breaking { path: actual, .. } = report else {
            panic!("expected a break at {path}, received {report:?}");
        };
        assert_eq!(actual, path);
    }

    #[test]
    fn emitted_closed_enums_compare_ids_independent_of_declaration_order() {
        type Old = Message<ClosedChoice<0>>;
        type Reordered = Message<ClosedChoice<1>>;
        let old = emitted::<Old, Old>();
        let history = std::slice::from_ref(&old);
        assert_eq!(
            manifest_check::check(history, &emitted::<Old, Old>()).unwrap(),
            Compatibility::Preserved,
        );
        assert_eq!(
            manifest_check::check(history, &emitted::<Reordered, Reordered>()).unwrap(),
            Compatibility::Preserved,
        );
        expect_break(
            manifest_check::check(history, &emitted::<Message<ClosedChoice<3>>, Old>()).unwrap(),
            "$.packet.request.0<Paused#4294967295>",
        );
        assert!(method_manifest::<Message<ClosedChoice<2>>, EmptyMessage>().is_err());
    }

    #[test]
    fn gives_each_emitted_comparison_its_own_bounded_work() {
        let history: Vec<_> = (0..4)
            .map(|_| emitted::<Message<WideTuple>, Message<WideTuple>>())
            .collect();
        assert_eq!(
            manifest_check::check(
                &history,
                &emitted::<Message<WideTuple>, Message<WideTuple>>()
            )
            .unwrap(),
            Compatibility::Preserved,
        );

        let complex = emitted::<Message<RepeatedWideTuple>, EmptyMessage>();
        assert!(matches!(
            manifest_check::check(
                &[emitted::<Message<RepeatedWideTuple>, EmptyMessage>()],
                &complex
            )
            .unwrap(),
            Compatibility::Unsupported {
                reason: "schema check exceeds its complexity limit",
                ..
            },
        ));
    }

    #[test]
    fn emitted_manifests_check_real_old_requests_and_new_replies() {
        let a_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/evolution_contract/baselines/a");
        assert_eq!(
            fs::read_to_string(a_path.join("packet.json")).unwrap(),
            fixture_manifest::<0, 0>().unwrap(),
        );
        let a = Service::load(&a_path).unwrap();
        let b = service(read_fixture::<1, 1>(), "update", json!({}));
        let c = service(read_fixture::<2, 2>(), "update", json!({}));
        assert_eq!(
            manifest_check::check(std::slice::from_ref(&a), &b).unwrap(),
            Compatibility::Drops(vec![
                "$.packet.response.groups[].rows[].marker#9".into(),
                "$.packet.response.groups[].rows[].note#7".into(),
            ]),
        );
        expect_break(
            manifest_check::check(std::slice::from_ref(&a), &c).unwrap(),
            "$.packet.request.groups[].rows[].epoch#2",
        );

        let mixed = service(read_fixture::<1, 2>(), "update", json!({}));
        let Compatibility::Drops(paths) = manifest_check::check(&[a, b], &mixed).unwrap() else {
            panic!("the new response must report what an old client discards");
        };
        assert!(
            paths
                .iter()
                .all(|path| path.starts_with("$.packet.response."))
        );
        for path in [
            "$.packet.response.groups[].rows[].epoch#2",
            "$.packet.response.groups[].rows[].state<Archived#3>",
            "$.packet.response.groups[].rows[].state<Detail#2>.audit#4",
        ] {
            assert!(paths.iter().any(|found| found == path), "{paths:?}");
        }
    }

    #[test]
    fn checks_method_mode_and_directional_published_limits() {
        let a_document = read_fixture::<0, 0>();
        let a = service(a_document.clone(), "update", json!({}));
        let history = std::slice::from_ref(&a);
        expect_break(
            manifest_check::check(history, &service(a_document.clone(), "query", json!({})))
                .unwrap(),
            "$.packet",
        );
        let mut too_small = a_document.clone();
        too_small["request"]["payload_limits"]["bytes"] = json!(8);
        expect_break(
            manifest_check::check(history, &service(too_small, "update", json!({}))).unwrap(),
            "$.packet.request.payload_limits.bytes",
        );
        let mut too_large = a_document;
        too_large["response"]["payload_limits"]["depth"] = json!(fixture::LIMITS.depth + 1);
        expect_break(
            manifest_check::check(history, &service(too_large, "update", json!({}))).unwrap(),
            "$.packet.response.payload_limits.depth",
        );
    }

    #[test]
    fn derives_recovery_at_the_nearest_explicit_reader_scope() {
        type Old = OpenSlot<'static, OldState, true>;
        type New = OpenSlot<'static, NewState, true>;
        type Changed = OpenSlot<'static, IncompatibleState, true>;
        type Optional = OpenSlot<'static, OldState>;
        type Nested<T> = RecoverSlot<'static, (u8, RecoverSlot<'static, Option<T>>)>;

        expect_break(
            manifest_check::check(
                &[emitted::<EmptyMessage, Message<Old>>()],
                &emitted::<EmptyMessage, Message<New>>(),
            )
            .unwrap(),
            "$.packet.response.0<Future#9>",
        );
        assert_eq!(
            manifest_check::check(
                &[emitted::<EmptyMessage, Message<Nested<Old>>>()],
                &emitted::<EmptyMessage, Message<Nested<New>>>()
            )
            .unwrap(),
            Compatibility::Drops(vec!["$.packet.response.0.some.1".into()]),
        );
        expect_break(
            manifest_check::check(
                &[emitted::<EmptyMessage, Message<Nested<Old>>>()],
                &emitted::<EmptyMessage, Message<Nested<Changed>>>(),
            )
            .unwrap(),
            "$.packet.response.0.some.1.some.some<Ready#1>",
        );
        assert_eq!(
            manifest_check::check(
                &[emitted::<EmptyMessage, Message<Optional>>()],
                &emitted::<EmptyMessage, Message<New>>()
            )
            .unwrap(),
            Compatibility::Drops(vec!["$.packet.response.0<Future#9>".into()]),
        );
        expect_break(
            manifest_check::check(
                &[emitted::<Message<Optional>, EmptyMessage>()],
                &emitted::<Message<Old>, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.0",
        );
    }

    #[test]
    fn derives_recovery_for_missing_required_extensions() {
        type Before = OldExtended;
        type After = NewExtended;
        type Nested<T, Suffix> = RecoverSlot<'static, (u8, RecoverSlot<'static, (T, Suffix)>)>;

        expect_break(
            manifest_check::check(
                &[emitted::<Before, EmptyMessage>()],
                &emitted::<After, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.epoch#7",
        );
        assert_eq!(
            manifest_check::check(
                &[emitted::<Message<Nested<Before, bool>>, EmptyMessage>()],
                &emitted::<Message<Nested<After, bool>>, EmptyMessage>(),
            )
            .unwrap(),
            Compatibility::Drops(vec!["$.packet.request.0.some.1".into()]),
        );
        expect_break(
            manifest_check::check(
                &[emitted::<Message<Nested<Before, bool>>, EmptyMessage>()],
                &emitted::<Message<Nested<After, u8>>, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.0.some.1.some.1",
        );
        expect_break(
            manifest_check::check(
                &[emitted::<Message<Option<Before>>, EmptyMessage>()],
                &emitted::<Message<Option<After>>, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.0.some.epoch#7",
        );
    }

    #[allow(
        dead_code,
        reason = "Only the emitted schemas are needed for this test"
    )]
    #[test]
    fn emitted_extension_tuples_evolve_only_by_appending_optional_tail_slots() {
        #[derive(Evolve)]
        #[wire(crate = "wire")]
        struct BSlots {
            #[wire(id = 3)]
            note: Option<u32>,
        }
        #[derive(Evolve)]
        #[wire(crate = "wire")]
        struct CSlots {
            #[wire(id = 3)]
            note: Option<u32>,
            #[wire(id = 4)]
            marker: Option<()>,
        }
        #[derive(Wire)]
        #[wire(crate = "wire")]
        struct A(
            bool,
            u16,
            #[wire(extensions)] ExtensionSlot<'static, OldExtensions>,
        );
        #[derive(Wire)]
        #[wire(crate = "wire")]
        struct B(
            bool,
            u16,
            #[wire(extensions)] ExtensionSlot<'static, BSlots>,
        );
        #[derive(Wire)]
        #[wire(crate = "wire")]
        struct C(
            bool,
            u16,
            #[wire(extensions)] ExtensionSlot<'static, CSlots>,
        );

        let a = emitted::<Message<Option<A>>, Message<Option<A>>>();
        let b = emitted::<Message<Option<B>>, Message<Option<B>>>();
        let c = emitted::<Message<Option<C>>, Message<Option<C>>>();
        assert_eq!(
            manifest_check::check(
                &[emitted::<Message<Option<A>>, EmptyMessage>()],
                &emitted::<Message<Option<C>>, EmptyMessage>()
            )
            .unwrap(),
            Compatibility::Preserved,
        );
        assert_eq!(
            manifest_check::check(&[a, b], &c).unwrap(),
            Compatibility::Drops(vec![
                "$.packet.response.0.some.2".into(),
                "$.packet.response.0.some.3".into(),
            ]),
        );
        expect_break(
            manifest_check::check(
                &[emitted::<EmptyMessage, B>()],
                &emitted::<EmptyMessage, A>(),
            )
            .unwrap(),
            "$.packet.response.2",
        );
        expect_break(
            manifest_check::check(
                &[emitted::<B, EmptyMessage>()],
                &emitted::<A, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.2",
        );
        expect_break(
            manifest_check::check(
                &[emitted::<Message<(bool, u16)>, EmptyMessage>()],
                &emitted::<Message<A>, EmptyMessage>(),
            )
            .unwrap(),
            "$.packet.request.0",
        );
        expect_break(
            manifest_check::check(
                &[emitted::<EmptyMessage, Message<A>>()],
                &emitted::<EmptyMessage, Message<(bool, u16)>>(),
            )
            .unwrap(),
            "$.packet.response.0",
        );

        let candidate: Value = method_manifest::<C, EmptyMessage>()
            .unwrap()
            .parse()
            .unwrap();
        let tuple = candidate["schemas"]["Request"]["name"]
            .as_str()
            .unwrap()
            .to_string();
        let history = [emitted::<A, EmptyMessage>(), emitted::<B, EmptyMessage>()];
        let check_candidate =
            |document| manifest_check::check(&history, &service(document, "update", json!({})));

        let mut repurposed = candidate.clone();
        repurposed["schemas"][&tuple]["extensions"][0]["name"] = json!("replacement");
        expect_break(check_candidate(repurposed).unwrap(), "$.packet.request.2");

        let mut shifted = candidate.clone();
        shifted["schemas"][&tuple]["extensions"][0]["name"] = json!("marker");
        shifted["schemas"][&tuple]["extensions"][1]["name"] = json!("note");
        expect_break(check_candidate(shifted).unwrap(), "$.packet.request.2");

        let mut retyped = candidate.clone();
        retyped["schemas"][&tuple]["extensions"][0]["value"] = json!({"k":"bool"});
        expect_break(check_candidate(retyped).unwrap(), "$.packet.request.2");

        let mut reordered = candidate.clone();
        reordered["schemas"][&tuple]["items"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        expect_break(check_candidate(reordered).unwrap(), "$.packet.request.0");

        let mut different_arity = candidate.clone();
        let node = &mut different_arity["schemas"][&tuple];
        node["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({"k":"unit"}));
        node["extensions"][0]["id"] = json!(4);
        node["extensions"][1]["id"] = json!(5);
        expect_break(
            check_candidate(different_arity).unwrap(),
            "$.packet.request",
        );

        let mut required = candidate.clone();
        required["schemas"][&tuple]["extensions"][0]["required"] = json!(true);
        assert!(check_candidate(required).is_err());
        let mut gap = candidate;
        gap["schemas"][&tuple]["extensions"][1]["id"] = json!(5);
        assert!(check_candidate(gap).is_err());
    }

    #[test]
    fn validates_nested_tombstones_across_loaded_manifest_revisions() {
        let live_document = read_fixture::<1, 1>();
        let mut next_document = live_document.clone();
        let types = next_document["schemas"].as_object_mut().unwrap();
        let mut record_name = None;
        for (name, shape) in types {
            if let Some(entries) = shape.get_mut("extensions").and_then(Value::as_array_mut) {
                if let Some(index) = entries.iter().position(|ext| ext["id"].as_u64() == Some(7)) {
                    entries.remove(index);
                    assert!(record_name.replace(name.to_string()).is_none());
                }
            }
        }
        let record = record_name.unwrap();
        let tombstones = json!({"packet": {record: [{"id": 7, "name":"note"}]}});
        let baseline = service(live_document.clone(), "update", json!({}));
        let removed_without_tombstone = service(next_document.clone(), "update", json!({}));
        let retired = service(next_document, "update", tombstones);
        expect_break(
            manifest_check::check(std::slice::from_ref(&baseline), &removed_without_tombstone)
                .unwrap(),
            "$.packet.request.groups[].rows[].note#7",
        );
        assert_eq!(
            manifest_check::check(std::slice::from_ref(&baseline), &retired).unwrap(),
            Compatibility::Drops(vec!["$.packet.request.groups[].rows[].note#7".into()]),
        );
        let recycled = service(live_document, "update", json!({}));
        expect_break(
            manifest_check::check(&[baseline, retired], &recycled).unwrap(),
            "$.packet.request.groups[].rows[].note#7",
        );
    }
}
