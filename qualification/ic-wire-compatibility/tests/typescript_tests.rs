//! Current Rust/TypeScript exchange against independent byte and schema contracts.

mod advice_cases;
mod bulk_cases;
mod consumer_cases;
mod domain_cases;
mod manifest_cases;
mod renamed_cases;
mod scalar_cases;
mod service_cases;
mod standard_cases;
mod support;
mod validation_cases;

use crate::support::{decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use wire::typescript::{Method, PayloadLimits, SchemaLimits, SchemaRegistry, ValidatedRegistry};
use wire::{Bytes, Direction, Encoding, Limits, List, Packed};

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire")]
enum Choice {
    Ready,
    Value(u16),
}

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire", fixed)]
struct Record<T = u8, const N: usize = 3> {
    number: T,
    choice: Choice,
    optional: Option<u8>,
    bytes: Bytes<N>,
    packed: Packed<i16, 2>,
    ordinary: List<i16, 2>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire", fixed)]
struct Chain {
    value: u16,
    next: Option<Box<Chain>>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, wire::typescript::Wire)]
#[wire(crate = "wire", fixed)]
struct Shared {
    left: Chain,
    right: Chain,
}

// These cases qualify fixed fields within a genuine sparse message root.
fn message_schema(core: Value) -> Value {
    let mut schema = json!({"k":"extension-tuple","extensions":[]});
    schema["items"] = Value::Array(vec![core]);
    schema
}

fn grouped_messages(mut types: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let roots = types.keys().cloned().collect::<Vec<_>>();
    for name in roots {
        types.insert(
            format!("Message{name}"),
            message_schema(json!({"k":"ref","name":name})),
        );
    }
    types
}

fn message_type<T: wire::typescript::Wire + ?Sized>(name: &str) -> ValidatedRegistry {
    let schema = ValidatedRegistry::from_type::<T>(name, SchemaLimits::default()).unwrap();
    let definitions = serde_json::from_str(&schema.metadata_json().unwrap()).unwrap();
    ValidatedRegistry::from_metadata(&grouped_messages(definitions), SchemaLimits::default())
        .unwrap()
}

fn limits() -> Limits {
    Limits {
        bytes: 128,
        allocation: 32_768,
        nodes: 512,
        depth: 32,
    }
}

#[test]
fn typescript_exchanges_payloads_and_frames() {
    let mut types = BTreeMap::from([(
        "Record".into(),
        json!({"k":"record","fixed":true,"fields":[
            {"name":"number","value":{"k":"u","bits":8}},
            {"name":"choice","value":{"k":"enum","variants":[
                {"name":"Ready","id":0,"value":{"k":"unit"}},
                {"name":"Value","id":1,"value":{"k":"u","bits":16}}
            ]}},
            {"name":"optional","value":{"k":"option","item":{"k":"u","bits":8}}},
            {"name":"bytes","value":{"k":"bytes","max":3}},
            {"name":"packed","value":{"k":"packed","word":"i16","max":2}},
            {"name":"ordinary","value":{"k":"list","max":2,"item":{"k":"i","bits":16}}}
        ]}),
    )]);
    types.insert(
        "Chain".into(),
        json!({"k":"record","fixed":true,"fields":[
            {"name":"value","value":{"k":"u","bits":16}},
            {"name":"next","value":{"k":"option","item":{"k":"ref","name":"Chain"}}}
        ]}),
    );
    types.insert(
        "Shared".into(),
        json!({"k":"record","fixed":true,"fields":[
            {"name":"left","value":{"k":"ref","name":"Chain"}},
            {"name":"right","value":{"k":"ref","name":"Chain"}}
        ]}),
    );
    let graph = ValidatedRegistry::from_metadata(&types, SchemaLimits::default()).unwrap();
    let derived =
        ValidatedRegistry::from_type::<Record<u8, 3>>("Record", SchemaLimits::default()).unwrap();
    let derived_metadata: Value = serde_json::from_str(&derived.metadata_json().unwrap()).unwrap();
    assert_eq!(derived_metadata["Record"], types["Record"]);
    let exported: Value = serde_json::from_str(&graph.metadata_json().unwrap()).unwrap();
    assert_eq!(exported, serde_json::to_value(&types).unwrap());

    let mut records: Vec<Record> = Vec::new();
    let mut literals = Vec::new();
    for (words, literal) in [
        (vec![], vec![0, 0, 0, 0, 0, 0]),
        (vec![0], vec![0, 0, 0, 0, 2, 0, 0, 1, 0]),
        (vec![32767], vec![0, 0, 0, 0, 2, 255, 127, 1, 254, 255, 3]),
    ] {
        records.push(Record {
            number: 0,
            choice: Choice::Ready,
            optional: None,
            bytes: Bytes::new(vec![]).unwrap(),
            packed: Packed::new(words.clone()).unwrap(),
            ordinary: List::new(words).unwrap(),
        });
        literals.push(literal);
    }
    records.push(Record {
        number: 7,
        choice: Choice::Value(128),
        optional: Some(9),
        bytes: Bytes::new(vec![0xaa, 0xbb]).unwrap(),
        packed: Packed::new(vec![1, -1]).unwrap(),
        ordinary: List::new(vec![1, -1]).unwrap(),
    });
    literals.push(vec![
        7, 1, 128, 1, 1, 9, 2, 170, 187, 4, 1, 0, 255, 255, 2, 2, 1,
    ]);

    for literal in &mut literals {
        literal.push(0);
    }
    let mut cases = Vec::new();
    for (record, literal) in records.iter().zip(&literals) {
        let payload = encode(record, Encoding::Payload, limits()).unwrap();
        assert_eq!(&payload, literal);
        let request = encode(
            record,
            Encoding::Frame {
                method: 7,
                direction: Direction::Request,
            },
            limits(),
        )
        .unwrap();
        let response = encode(
            record,
            Encoding::Frame {
                method: 7,
                direction: Direction::Response,
            },
            limits(),
        )
        .unwrap();
        assert_eq!(&request[..11], &[73, 87, 7, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&response[..11], &[73, 87, 7, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&request[11..], literal);
        assert_eq!(&response[11..], literal);
        cases.push(json!({"value":{
            "number":record.number,
            "choice":match record.choice { Choice::Ready => json!({"tag":"Ready","value":null}), Choice::Value(value) => json!({"tag":"Value","value":value}) },
            "optional":record.optional.map(|value| json!({"some":value})),
            "bytes":record.bytes.as_slice(), "packed":record.packed.as_slice(), "ordinary":record.ordinary.as_slice()
        }, "payload":payload, "request":request, "response":response}));
    }
    let policy = limits();
    let chain = Chain {
        value: 1,
        next: Some(Box::new(Chain {
            value: 128,
            next: None,
        })),
    };
    let shared = Shared {
        left: Chain {
            value: 7,
            next: None,
        },
        right: Chain {
            value: 7,
            next: None,
        },
    };
    let chain_bytes = encode(&chain, Encoding::Payload, policy).unwrap();
    let shared_bytes = encode(&shared, Encoding::Payload, policy).unwrap();
    assert_eq!(chain_bytes, [1, 1, 128, 1, 0, 0]);
    assert_eq!(shared_bytes, [7, 0, 7, 0, 0]);
    let fixture = json!({"types":exported,"cases":cases,"chain":chain_bytes,"shared":shared_bytes,"policy":{"bytes":policy.bytes,"nodes":policy.nodes,"depth":policy.depth}});
    let browser_limits = PayloadLimits {
        bytes: policy.bytes as u64,
        nodes: policy.nodes as u64,
        depth: policy.depth as u64,
    };
    let mut fixture = fixture;
    fixture["typeAssertions"] = include_str!("typescript_contract.ts").into();
    let messages =
        ValidatedRegistry::from_metadata(&grouped_messages(types), SchemaLimits::default())
            .unwrap();
    fixture["module"] = messages
        .payload_module(
            "recordCodec",
            "MessageRecord",
            "./runtime-adapter.ts",
            browser_limits,
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap()
        .into();
    fixture["badModule"] = messages
        .payload_module(
            "recordCodec",
            "MessageRecord",
            "./bad-runtime.ts",
            browser_limits,
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap()
        .into();
    let framed_limits = PayloadLimits {
        bytes: browser_limits.bytes - 11,
        ..browser_limits
    };
    let methods = [
        Method {
            export: "roundtrip",
            id: 7,
            request: "MessageRecord",
            response: "MessageRecord",
            request_limits: framed_limits,
            response_limits: framed_limits,
        },
        Method {
            export: "highId",
            id: u64::MAX,
            request: "MessageChain",
            response: "MessageShared",
            request_limits: PayloadLimits {
                bytes: 6,
                nodes: 7,
                depth: 5,
            },
            response_limits: PayloadLimits {
                bytes: 5,
                nodes: 8,
                depth: 4,
            },
        },
    ];
    let mut collection = SchemaRegistry::new(SchemaLimits::default()).unwrap();
    collection.register::<Record<u8, 3>>("Record").unwrap();
    collection.register::<Chain>("Chain").unwrap();
    collection.register::<Shared>("Shared").unwrap();
    let definitions =
        serde_json::from_str(&collection.finish().unwrap().metadata_json().unwrap()).unwrap();
    let collected =
        ValidatedRegistry::from_metadata(&grouped_messages(definitions), SchemaLimits::default())
            .unwrap();
    fixture["frameModule"] = collected
        .method_module(
            &methods,
            "./runtime-adapter.ts",
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap()
        .into();
    fixture["badFrameModule"] = collected
        .method_module(
            &methods,
            "./bad-runtime.ts",
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap()
        .into();
    let result = run_typescript(&fixture, include_str!("typescript_exchange.mjs"));
    let high_request: Vec<u8> = serde_json::from_value(result["highRequest"].clone()).unwrap();
    let high_response: Vec<u8> = serde_json::from_value(result["highResponse"].clone()).unwrap();
    assert_eq!(
        decode::<Chain>(
            &high_request,
            Encoding::Frame {
                method: u64::MAX,
                direction: Direction::Request
            },
            policy
        )
        .unwrap(),
        chain
    );
    assert_eq!(
        decode::<Shared>(
            &high_response,
            Encoding::Frame {
                method: u64::MAX,
                direction: Direction::Response
            },
            policy
        )
        .unwrap(),
        shared
    );
    let returned_chain: Vec<u8> = serde_json::from_value(result["chain"].clone()).unwrap();
    let returned_shared: Vec<u8> = serde_json::from_value(result["shared"].clone()).unwrap();
    assert_eq!(returned_chain, chain_bytes);
    assert_eq!(returned_shared, shared_bytes);
    assert_eq!(
        decode::<Chain>(&returned_chain, Encoding::Payload, policy).unwrap(),
        chain
    );
    assert_eq!(
        decode::<Shared>(&returned_shared, Encoding::Payload, policy).unwrap(),
        shared
    );
    let outputs: Vec<Vec<Vec<u8>>> = serde_json::from_value(result["records"].clone()).unwrap();
    assert_eq!(outputs.len(), records.len());
    for ((output, record), literal) in outputs.iter().zip(&records).zip(&literals) {
        assert_eq!(output.len(), 3);
        for (bytes, encoding) in output.iter().zip([
            Encoding::Payload,
            Encoding::Frame {
                method: 7,
                direction: Direction::Request,
            },
            Encoding::Frame {
                method: 7,
                direction: Direction::Response,
            },
        ]) {
            assert_eq!(
                &decode::<Record>(bytes, encoding, limits()).unwrap(),
                record
            );
        }
        assert_eq!(&output[0], literal);
    }
}

#[test]
fn runtime_primitive_acceptance_matches_literal_rules() {
    let result = run_typescript(&json!({}), include_str!("runtime_primitives.mjs"));
    assert!(result["checked"].as_u64().unwrap() > 130_000);
}

#[test]
fn wire_errors_are_recognisable_across_runtime_copies() {
    assert_eq!(
        run_typescript(&json!({}), include_str!("runtime_errors.mjs")),
        json!(true)
    );
}

fn run_typescript(fixture: &Value, script: &str) -> Value {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("ic-wire-interop-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(directory.join("message.mjs"), include_str!("message.mjs")).unwrap();
    let runtime = directory.join("codec-runtime.ts");
    fs::write(&runtime, wire::typescript::RUNTIME).unwrap();
    if let Some(module) = fixture["module"].as_str() {
        fs::write(directory.join("client.ts"), module).unwrap();
        for (key, file) in [
            ("frameModule", "frame-client.ts"),
            ("serviceModule", "service.ts"),
            ("badFrameModule", "bad-frame-client.ts"),
            ("badModule", "bad-client.ts"),
        ] {
            if let Some(source) = fixture[key].as_str() {
                fs::write(directory.join(file), source).unwrap();
            }
        }
        let marker = "export {schemaFormat} from './codec-runtime.ts';";
        fs::write(directory.join("runtime-adapter.ts"), format!(
            "import {{createPayloadCodec as create, createCodec as frame}} from './codec-runtime.ts';\n{marker}\nexport let observed: unknown;\nexport const observedMethods: unknown[] = [];\nexport function createPayloadCodec(meta: unknown, types: unknown) {{ observed = structuredClone(meta); return create(meta, types); }}\nexport function createCodec(meta: unknown, types: unknown) {{ observedMethods.push(structuredClone(meta)); return frame(meta, types); }}\n"
        )).unwrap();
        if let Some(adapter) = fixture["runtimeAdapter"].as_str() {
            fs::write(
                directory.join("runtime-adapter.ts"),
                format!("{marker}\n{adapter}"),
            )
            .unwrap();
        }
        fs::write(directory.join("bad-runtime.ts"), "export const schemaFormat = 'incompatible'; export function createPayloadCodec() { throw new Error('factory must not run'); } export const createCodec = createPayloadCodec;").unwrap();
        if std::env::var_os("IC_WIRE_CHECK_TYPESCRIPT").is_some() {
            check_typescript(&directory, fixture);
        }
    }
    let mut child = Command::new("node")
        .args([
            "--experimental-transform-types",
            "--input-type=module",
            "--eval",
            script,
        ])
        .arg(&runtime)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node with --experimental-transform-types must be installed explicitly");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(fixture).unwrap().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    fs::remove_dir_all(&directory).unwrap();
    assert!(
        result.status.success(),
        "TypeScript exchange failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn check_typescript(directory: &std::path::Path, fixture: &Value) {
    let compiler = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../typescript/node_modules/typescript/bin/tsc");
    assert!(
        compiler.is_file(),
        "Install qualification/typescript with npm ci --ignore-scripts first"
    );
    let mut files = vec!["client.ts"];
    if fixture["frameModule"].is_string() {
        files.push("frame-client.ts");
    }
    if let Some(source) = fixture["typeAssertions"].as_str() {
        fs::write(directory.join("type-assertions.ts"), source).unwrap();
        files.push("type-assertions.ts");
    }
    let result = Command::new("node")
        .arg(compiler)
        .args([
            "--strict",
            "--noUncheckedIndexedAccess",
            "--exactOptionalPropertyTypes",
            "--noEmit",
            "--target",
            "es2022",
            "--module",
            "nodenext",
            "--allowImportingTsExtensions",
        ])
        .args(files)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "TypeScript static check failed: {}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
