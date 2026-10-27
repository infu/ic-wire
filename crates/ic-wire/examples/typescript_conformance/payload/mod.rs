//! Rust payload vectors, generated method clients and consumer scalar wrappers.
use ic_wire::{Bytes, Limits, Wire, native, typescript};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt::Write as _, fs, path::Path};

const LIMITS: Limits = Limits {
    bytes: 4096,
    allocation: 1024 * 1024,
    nodes: 8192,
    depth: 32,
};

#[derive(Debug, Deserialize, Wire, PartialEq, Serialize)]
struct Record {
    label: String,
    data: ic_wire::ByteBuf,
    values: Vec<u64>,
    optional: Option<Option<()>>,
}

#[derive(Debug, Deserialize, Wire, PartialEq, Serialize)]
enum Node {
    End,
    Next(Box<Self>),
}

fn add<
    T: Serialize + serde::de::DeserializeOwned + typescript::Wire + PartialEq + std::fmt::Debug,
>(
    directory: &Path,
    checks: &mut String,
    index: usize,
    value: T,
    javascript: &str,
) {
    let mut registry = typescript::SchemaRegistry::new();
    let schema = registry.register::<T>("Payload").unwrap();
    let types = registry.finish().unwrap();
    typescript::validate_payload_schema(&types, &schema).unwrap();
    let mut method = typescript::payload_method_limits(LIMITS, LIMITS).unwrap();
    method["name"] = json!("codec");
    method["mode"] = json!("update");
    method["request"] = schema.clone();
    method["response"] = schema;
    let source = typescript::generate_payload_registry(
        &types,
        &[method],
        "./wire-runtime.js",
        &Default::default(),
        |report| eprint!("{report}"),
    )
    .unwrap();
    fs::write(directory.join(format!("payload-{index}.ts")), source).unwrap();
    let encoded = native::encode(&value, LIMITS).unwrap();
    assert_eq!(native::decode::<T>(&encoded, LIMITS).unwrap(), value);
    let mut candidates = Vec::new();
    for length in 0..encoded.len() {
        candidates.push(encoded[..length].to_vec());
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    candidates.push(trailing);
    for index in 0..encoded.len().min(32) {
        let mut mutated = encoded.clone();
        mutated[index] ^= 128;
        candidates.push(mutated);
    }
    let mutations: Vec<_> = candidates
        .into_iter()
        .map(|bytes| {
            let accepted = native::decode::<T>(&bytes, LIMITS).is_ok();
            json!({"bytes":bytes,"accepted":accepted})
        })
        .collect();
    writeln!(checks, "checkPayload((await import('./compiled/payload-{index}.js')).codec, {javascript}, {encoded:?}, {});", serde_json::to_string(&mutations).unwrap()).unwrap();
}

pub(super) fn generate(directory: &Path) -> String {
    let mut checks = String::from("{\n");
    checks.push_str(include_str!("checks.mjs"));
    add(directory, &mut checks, 0, (), "null");
    add(directory, &mut checks, 1, 7u64, "7n");
    add(directory, &mut checks, 2, (7u64,), "[7n]");
    add(
        directory,
        &mut checks,
        3,
        vec![0i16, i16::MIN, i16::MAX],
        "[0,-32768,32767]",
    );
    add(
        directory,
        &mut checks,
        4,
        [73u8, 87, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "[73,87,0,0,0,0,0,0,0,0,0]",
    );
    add(
        directory,
        &mut checks,
        5,
        Record {
            label: "é🦀".into(),
            data: ic_wire::ByteBuf::from(vec![0, 127, 255]),
            values: vec![0, 128, u64::MAX],
            optional: Some(None),
        },
        "{label:'é🦀',data:new Uint8Array([0,127,255]),values:[0n,128n,18446744073709551615n],optional:{some:null}}",
    );
    add(
        directory,
        &mut checks,
        6,
        Node::Next(Box::new(Node::Next(Box::new(Node::End)))),
        "{tag:'Next',value:{tag:'Next',value:{tag:'End',value:null}}}",
    );
    add(
        directory,
        &mut checks,
        7,
        ic_wire::ByteBuf::from(vec![42; 256]),
        "new Uint8Array(256).fill(42)",
    );
    fs::write(
        directory.join("scalar-runtime.ts"),
        include_str!("scalar-runtime.ts"),
    )
    .unwrap();
    let method = json!({"name":"checked","mode":"query", "request":{"k":"bytes","max":64,"check":"magnitude"}, "response":{"k":"bytes","max":64,"check":"magnitude"}, "requestBytes":4096,"responseBytes":4096,"requestNodes":8192,"responseNodes":8192,"requestDepth":32,"responseDepth":32});
    fs::write(
        directory.join("payload-checked.ts"),
        typescript::generate_payload_registry(
            &Default::default(),
            &[method],
            "./scalar-runtime.js",
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap(),
    )
    .unwrap();
    checks.push_str("const scalar = (await import('./compiled/scalar-runtime.js')).unsignedMagnitude({bytes:4096,nodes:8192,depth:32});\n");
    for (magnitude, value) in [
        (vec![], "0n"),
        (vec![1], "1n"),
        (vec![255; 16], "(1n<<128n)-1n"),
        ([vec![1], vec![0; 32]].concat(), "1n<<256n"),
    ] {
        let encoded = native::encode(&Bytes::<64>::new(magnitude).unwrap(), LIMITS).unwrap();
        writeln!(checks, "assert.deepEqual([...scalar.encode({value})], {encoded:?}); assert.equal(scalar.decode(new Uint8Array({encoded:?})), {value});").unwrap();
    }
    checks.push_str(include_str!("scalar-checks.mjs"));
    checks.push_str("\n}\n");
    checks
}
