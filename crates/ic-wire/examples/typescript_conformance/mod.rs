//! Generate independent Rust vectors and executable JavaScript conformance checks.
mod admission;
mod large_bytes;
mod native_schema;
mod payload;
use ic_wire::{Bytes, Limits, List, Text, Wire};
use std::{
    fmt::Write as _,
    fs,
    io::{BufWriter, Write as _},
    path::PathBuf,
};

const ID: u64 = 0xfedc_ba98_7654_3210;
const LIMITS: Limits = Limits {
    bytes: 4096,
    allocation: 65536,
    nodes: 4096,
    depth: 64,
};
fn generate<T: ic_wire::typescript::Wire>() -> String {
    let schema = ic_wire::typescript::native_schema::<T>().unwrap();
    let method = serde_json::json!({"name":"codec","mode":"query","id":ID.to_string(),"request":schema,"response":schema,"requestBytes":LIMITS.bytes,"responseBytes":LIMITS.bytes,"requestNodes":LIMITS.nodes,"responseNodes":LIMITS.nodes,"requestDepth":LIMITS.depth,"responseDepth":LIMITS.depth});
    ic_wire::typescript::generate_registry(
        &Default::default(),
        &[method],
        "./wire-runtime.js",
        &Default::default(),
        |report| eprint!("{report}"),
    )
    .unwrap()
}
#[derive(serde::Serialize, serde::Deserialize, Wire)]
struct Record {
    r#type: Text<32>,
    __proto__: Bytes<8>,
    optional: Option<Option<()>>,
}
#[derive(serde::Serialize, serde::Deserialize, Wire)]
enum Choice {
    Last,
    Payload(Record),
    Empty,
}
#[derive(serde::Serialize, serde::Deserialize, Wire)]
enum BoxedChoice {
    Payload(Box<Record>),
    Empty,
}
// Consumer-shaped SQL page, not IcyDB's public DTO or storage representation.
#[derive(serde::Serialize, serde::Deserialize, Wire)]
enum SqlCell {
    Null,
    Signed(i64),
    Unsigned(u64),
    Real(ic_wire::FiniteF64),
    Single(ic_wire::FiniteF32),
    Text(Text<64>),
    Blob(Bytes<32>),
    Boolean(bool),
}
#[derive(serde::Serialize, serde::Deserialize, Wire)]
struct SqlPage {
    columns: List<Text<32>, 8>,
    rows: List<List<SqlCell, 8>, 4>,
    next: Option<Bytes<16>>,
}
// Consumer-owned numeric evidence; Wire only owns the bounded byte layout.
#[derive(serde::Serialize, serde::Deserialize, Wire)]
struct MagnitudeEvidence {
    cycles_sent: Option<Bytes<64>>,
}

// Native Serde shape oracle, independent of the JavaScript descriptor.
#[derive(serde::Serialize)]
struct NativeCount(u64);
#[derive(serde::Serialize)]
enum NativeNode {
    Leaf(String, NativeCount),
    Branch(Vec<Self>),
}
struct Fixture {
    directory: PathBuf,
    checks: String,
    count: usize,
    mutations: usize,
    mutation_data: BufWriter<fs::File>,
}
impl Fixture {
    fn add_boxed<T: ic_wire::typescript::Wire + serde::de::DeserializeOwned>(
        &mut self,
        value: T,
        javascript: &str,
    ) {
        let boxed = Box::new(value);
        assert_eq!(
            ic_wire::native::encode_request::<T>(ID, &boxed, LIMITS)
                .unwrap()
                .as_slice(),
            ic_wire::native::encode_request::<Box<T>>(ID, &boxed, LIMITS)
                .unwrap()
                .as_slice(),
        );
        assert_eq!(generate::<T>(), generate::<Box<T>>(),);
        self.add(boxed, javascript);
    }
    fn add<T: ic_wire::typescript::Wire + serde::de::DeserializeOwned>(
        &mut self,
        value: T,
        javascript: &str,
    ) {
        let name = format!("m{}", self.count);
        fs::write(self.directory.join(format!("{name}.ts")), generate::<T>()).unwrap();
        let request = ic_wire::native::encode_request::<T>(ID, &value, LIMITS).unwrap();
        let response = ic_wire::native::encode_response::<T>(ID, &value, LIMITS).unwrap();
        writeln!(self.checks, "import {{ codec as {name} }} from './compiled/{name}.js';\ncheck({name}, {javascript}, {:?}, {:?});", request.as_slice(), response.as_slice()).unwrap();
        assert_eq!(
            ic_wire::native::encoded_frame_len::<T>(&value, LIMITS).unwrap(),
            request.as_slice().len()
        );
        assert_eq!(
            ic_wire::native::encoded_frame_len::<T>(&value, LIMITS).unwrap(),
            response.as_slice().len()
        );
        for (direction, original) in [(0, request.as_slice()), (1, response.as_slice())] {
            let mut candidates = Vec::new();
            for offset in 0..original.len() {
                for bit in [1, 128] {
                    let mut bytes = original.to_vec();
                    bytes[offset] ^= bit;
                    candidates.push(bytes);
                }
            }
            // Nonminimal, overflowing and unterminated prefixes, independently
            // judged by Rust even when the first field is not a scalar varint.
            for prefix in [
                vec![0x80, 0],
                vec![0xff, 0xff, 4],
                vec![0xff, 0xff, 0xff, 0xff, 16],
                vec![0x80; 5],
                vec![0x80; 10],
                vec![0xff; 19],
            ] {
                let mut bytes = original[..11].to_vec();
                bytes.extend(prefix);
                bytes.extend_from_slice(&original[11..]);
                candidates.push(bytes);
            }
            // Fixed xorshift stream: reproducible mutation testing, not coverage-guided fuzzing.
            let mut seed = 0x92d6_8ca2_u32;
            for length in 0..64 {
                let mut bytes = original[..11].to_vec();
                for _ in 0..length {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    bytes.push(seed as u8);
                }
                candidates.push(bytes);
            }
            for bytes in candidates {
                let accepted = if direction == 0 {
                    ic_wire::native::decode_request::<T>(ID, &bytes, LIMITS)
                        .map(|v| ic_wire::native::encode_request::<T>(ID, &v, LIMITS).unwrap())
                } else {
                    ic_wire::native::decode_response::<T>(ID, &bytes, LIMITS)
                        .map(|v| ic_wire::native::encode_response::<T>(ID, &v, LIMITS).unwrap())
                };
                if let Ok(encoded) = &accepted {
                    assert_eq!(encoded, &bytes);
                }
                serde_json::to_writer(
                    &mut self.mutation_data,
                    &serde_json::json!({
                        "codec": name,
                        "direction": direction,
                        "bytes": bytes,
                        "accepted": accepted.is_ok(),
                    }),
                )
                .unwrap();
                writeln!(self.mutation_data).unwrap();
                self.mutations += 1;
            }
        }
        self.count += 1;
    }
}
fn main() {
    let mut arguments = clap::Command::new("typescript-conformance")
        .arg(
            clap::Arg::new("directory")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .get_matches();
    let directory = arguments.remove_one::<PathBuf>("directory").unwrap();
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("wire-runtime.ts"),
        ic_wire::typescript::RUNTIME,
    )
    .unwrap();
    fs::write(directory.join("package.json"), "{\"type\":\"module\"}").unwrap();
    let mut types = std::collections::BTreeMap::from([(
        "Node".into(),
        serde_json::json!({
            "k":"enum","variants":[
                {"name":"Leaf","id":0,"value":{"k":"tuple","items":[{"k":"text"},{"k":"newtype","item":{"k":"u","bits":64}}]}},
                {"name":"Branch","id":1,"value":{"k":"list","item":{"k":"ref","name":"Node"}}}
            ]
        }),
    )]);
    // Consumer aliases may share built-in names without capturing their uses.
    for (name, schema) in [
        ("Int16Array", serde_json::json!({"k":"packed-i16","max":8})),
        ("Uint8Array", serde_json::json!({"k":"bytes","max":8})),
        (
            "ReadonlyArray",
            serde_json::json!({"k":"list","item":{"k":"u","bits":16}}),
        ),
        ("globalThis", serde_json::json!({"k":"unit"})),
        ("JSON", serde_json::json!({"k":"unit"})),
        ("types", serde_json::json!({"k":"unit"})),
        ("createCodec", serde_json::json!({"k":"unit"})),
    ] {
        types.insert(name.into(), schema);
    }
    types.insert("__proto__".into(), serde_json::json!({"k":"unit"}));
    types.insert("Alias".into(), serde_json::json!({"k":"ref","name":"Node"}));
    types.insert(
        "WrappedAlias".into(),
        serde_json::json!({"k":"newtype","item":{"k":"ref","name":"Alias"}}),
    );
    let method = serde_json::json!({"name":"nativeTree","mode":"query","id":"1",
        "request":{"k":"ref","name":"WrappedAlias"},"response":{"k":"ref","name":"Alias"},
        "requestBytes":128,"responseBytes":128,"requestNodes":32,"responseNodes":32,
        "requestDepth":8,"responseDepth":8});
    // Method exports live in the value namespace; compile their signatures too.
    for name in [
        "Int16Array",
        "Uint8Array",
        "ReadonlyArray",
        "globalThis",
        "JSON",
        "types",
        "createCodec",
    ] {
        let mut shadow = method.clone();
        shadow["name"] = serde_json::json!(name);
        shadow["request"] = serde_json::json!({"k":"packed-i16","max":8});
        shadow["response"] = serde_json::json!({"k":"bytes","max":8});
        fs::write(
            directory.join(format!("shadow-{name}.ts")),
            ic_wire::typescript::generate_registry(
                &Default::default(),
                &[shadow],
                "./wire-runtime.js",
                &Default::default(),
                |report| eprint!("{report}"),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let mut proto = method.clone();
    proto["name"] = serde_json::json!("prototypeName");
    // This independent method previously reused nativeTree's ID 1. Give it
    // ID 2 so the fixture follows the registry uniqueness contract.
    proto["id"] = serde_json::json!("2");
    proto["request"] = serde_json::json!({"k":"ref","name":"__proto__"});
    proto["response"] = proto["request"].clone();
    let mut unsigned = method.clone();
    unsigned["name"] = serde_json::json!("unsignedByte");
    unsigned["id"] = serde_json::json!("7");
    unsigned["request"] = serde_json::json!({"k":"u","bits":8});
    unsigned["response"] = unsigned["request"].clone();
    let mut signed = unsigned.clone();
    signed["name"] = serde_json::json!("signedByte");
    signed["id"] = serde_json::json!("8");
    signed["request"] = serde_json::json!({"k":"i","bits":8});
    signed["response"] = signed["request"].clone();
    fs::write(
        directory.join("native.ts"),
        ic_wire::typescript::generate_registry(
            &types,
            &[method, proto, unsigned, signed],
            "./wire-runtime.js",
            &Default::default(),
            |report| eprint!("{report}"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut f = Fixture {
        mutation_data: BufWriter::new(fs::File::create(directory.join("mutations.jsonl")).unwrap()),
        directory,
        checks: format!(
            "const nativePayload = {:?};\n{}",
            postcard::to_allocvec(&NativeNode::Branch(vec![NativeNode::Leaf(
                "é".into(),
                NativeCount(128)
            )]))
            .unwrap(),
            include_str!("checks.mjs")
        ),
        count: 0,
        mutations: 0,
    };
    f.add((), "null");
    f.add(false, "false");
    f.add(true, "true");
    macro_rules! integers {
        ($($ty:ty => $suffix:literal),*) => {$({
            for value in [<$ty>::MIN, 0, 1, <$ty>::MAX] {
                f.add(value, &format!("{value}{}", $suffix));
            }
        })*};
    }
    integers!(u8 => "", i8 => "", u16 => "", i16 => "", u32 => "", i32 => "", u64 => "n", i64 => "n", u128 => "n", i128 => "n");
    // Every small-integer varint width transition, including signed ZigZag.
    for shift in [7, 14, 21, 28] {
        let boundary = 1_u32 << shift;
        for value in [boundary - 1, boundary, boundary + 1] {
            f.add(value, &value.to_string());
            if let Ok(value) = u16::try_from(value) {
                f.add(value, &value.to_string());
            }
        }
        let signed = (boundary / 2) as i32;
        for value in [
            -signed - 1,
            -signed,
            -signed + 1,
            signed - 1,
            signed,
            signed + 1,
        ] {
            f.add(value, &value.to_string());
            if let Ok(value) = i16::try_from(value) {
                f.add(value, &value.to_string());
            }
        }
    }
    f.add(
        List::<List<u32, 3>, 2>::new(vec![
            List::new(vec![127, 128, u32::MAX]).unwrap(),
            List::new(vec![]).unwrap(),
        ])
        .unwrap(),
        "[[127,128,4294967295],[]]",
    );
    for value in [127_u64, 128, 16383, 16384, 1 << 53] {
        f.add(value, &format!("{value}n"));
    }
    for value in [
        0.0_f32,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MIN,
        f32::MAX,
        0.1,
        -2.5,
    ] {
        f.add(
            ic_wire::FiniteF32::new(value).unwrap(),
            &format!("{:?}", f64::from(value)),
        );
    }
    for value in [
        0.0_f64,
        -0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MIN,
        f64::MAX,
        0.1,
        -2.5,
    ] {
        f.add(
            ic_wire::FiniteF64::new(value).unwrap(),
            &format!("{value:?}"),
        );
    }
    f.add(None::<Option<()>>, "null");
    f.add(Some(None::<()>), "{some:null}");
    f.add(Some(Some(())), "{some:{some:null}}");
    f.add(Ok::<_, Text<8>>(42_u64), "{ok:42n}");
    f.add(
        Err::<u64, _>(Text::<8>::new("oops".into()).unwrap()),
        "{err:'oops'}",
    );
    f.add([i16::MIN, 0, i16::MAX], "[-32768,0,32767]");
    f.add([0_u8; 0], "[]");
    f.add(
        List::<(), 4>::new(vec![(); 4]).unwrap(),
        "[null,null,null,null]",
    );
    f.add(List::<u64, 4>::new(vec![]).unwrap(), "[]");
    f.add(
        List::<u64, 4>::new(vec![0, 128, u64::MAX]).unwrap(),
        "[0n,128n,18446744073709551615n]",
    );
    f.add(
        Bytes::<8>::new(vec![0, 128, 255]).unwrap(),
        "new Uint8Array([0,128,255])",
    );
    // Retain the retired screen's principal-byte nesting and maximum length.
    // These are bytes, not a replacement for consumer principal validation.
    f.add(
        List::<Bytes<29>, 8>::new(vec![
            Bytes::new(vec![]).unwrap(),
            Bytes::new(vec![2; 10]).unwrap(),
            Bytes::new(vec![3; 29]).unwrap(),
        ])
        .unwrap(),
        "[new Uint8Array([]),new Uint8Array(10).fill(2),new Uint8Array(29).fill(3)]",
    );
    for text in ["", "é🦀", "\u{feff}text", "12345678"] {
        f.add(
            Text::<16>::new(text.into()).unwrap(),
            &serde_json::to_string(text).unwrap(),
        );
    }
    f.add(SqlPage {
        columns: List::new(["null", "signed", "unsigned", "real", "single", "text", "blob", "bool"].into_iter().map(|s| Text::new(s.into()).unwrap()).collect()).unwrap(),
        rows: List::new(vec![List::new(vec![SqlCell::Null, SqlCell::Signed(i64::MIN), SqlCell::Unsigned(u64::MAX), SqlCell::Real(ic_wire::FiniteF64::new(-0.0).unwrap()), SqlCell::Single(ic_wire::FiniteF32::new(0.1).unwrap()), SqlCell::Text(Text::new("é🦀".into()).unwrap()), SqlCell::Blob(Bytes::new(vec![0,255]).unwrap()), SqlCell::Boolean(true)]).unwrap()]).unwrap(),
        next: Some(Bytes::new(vec![1,2]).unwrap()),
    }, "{columns:['null','signed','unsigned','real','single','text','blob','bool'],rows:[[{tag:'Null',value:null},{tag:'Signed',value:-9223372036854775808n},{tag:'Unsigned',value:18446744073709551615n},{tag:'Real',value:-0},{tag:'Single',value:Math.fround(0.1)},{tag:'Text',value:'é🦀'},{tag:'Blob',value:new Uint8Array([0,255])},{tag:'Boolean',value:true}]],next:{some:new Uint8Array([1,2])}}");
    f.add(
        SqlPage {
            columns: List::new(vec![]).unwrap(),
            rows: List::new(vec![]).unwrap(),
            next: None,
        },
        "{columns:[],rows:[],next:null}",
    );
    f.add(Choice::Empty, "{tag:'Empty',value:null}");
    f.add(Choice::Last, "{tag:'Last',value:null}");
    f.add(Choice::Payload(Record { r#type: Text::new("hello".into()).unwrap(), __proto__: Bytes::new(vec![1,2]).unwrap(), optional: Some(None) }), "{tag:'Payload',value:{['__proto__']:new Uint8Array([1,2]),optional:{some:null},type:'hello'}}");
    f.add_boxed((), "null");
    f.add_boxed(42_u64, "42n");
    f.add_boxed(Box::new(42_u64), "42n");
    f.add_boxed(
        Record {
            r#type: Text::new("hello".into()).unwrap(),
            __proto__: Bytes::new(vec![1, 2]).unwrap(),
            optional: Some(None),
        },
        "{['__proto__']:new Uint8Array([1,2]),optional:{some:null},type:'hello'}",
    );
    f.add_boxed(Choice::Empty, "{tag:'Empty',value:null}");
    f.add(BoxedChoice::Payload(Box::new(Record { r#type: Text::new("hello".into()).unwrap(), __proto__: Bytes::new(vec![1,2]).unwrap(), optional: Some(None) })), "{tag:'Payload',value:{['__proto__']:new Uint8Array([1,2]),optional:{some:null},type:'hello'}}");
    f.add(
        ic_wire::PackedI16::<0>::new(vec![]).unwrap(),
        "new Int16Array()",
    );
    f.add(
        ic_wire::PackedI16::<1>::new(vec![32767]).unwrap(),
        "new Int16Array([32767])",
    );
    f.add(
        ic_wire::PackedI16::<8>::new(vec![-32768, -65, -1, 0, 63, 64, 16384, 32767]).unwrap(),
        "new Int16Array([-32768,-65,-1,0,63,64,16384,32767])",
    );
    f.add(ic_wire::Packed::<i8>::new(vec![]), "new Int8Array()");
    f.add(
        ic_wire::Packed::<i8>::new((i8::MIN..=i8::MAX).collect()),
        "Int8Array.from({length:256}, (_, i) => i - 128)",
    );
    f.add(ic_wire::Packed::<i16>::new(vec![]), "new Int16Array()");
    f.add(
        ic_wire::Packed::new(vec![i16::MIN, i16::MAX]),
        "new Int16Array([-32768,32767])",
    );
    f.add(
        ic_wire::Packed::new(vec![0_u16, u16::MAX]),
        "new Uint16Array([0,65535])",
    );
    f.add(
        ic_wire::Packed::new(vec![i32::MIN, i32::MAX]),
        "new Int32Array([-2147483648,2147483647])",
    );
    f.add(
        ic_wire::Packed::new(vec![0_u32, u32::MAX]),
        "new Uint32Array([0,4294967295])",
    );
    f.add(
        ic_wire::Packed::new(vec![i64::MIN, i64::MAX]),
        "new BigInt64Array([-9223372036854775808n,9223372036854775807n])",
    );
    f.add(
        ic_wire::Packed::new(vec![0_u64, u64::MAX]),
        "new BigUint64Array([0n,18446744073709551615n])",
    );
    f.add(
        ic_wire::Packed::new(vec![[0xab_u8; 32], [0xcd; 32]]),
        "Uint8Array.from([...new Uint8Array(32).fill(0xab),...new Uint8Array(32).fill(0xcd)])",
    );
    // Issue #26: owned arbitrary unsigned evidence uses bounded byte magnitudes.
    // Canonical numeric conversion remains a consumer check; bytes are unchanged.
    f.add(
        MagnitudeEvidence { cycles_sent: None },
        "{cycles_sent:null}",
    );
    for magnitude in [
        vec![],
        vec![255; 16],
        [vec![1], vec![0; 16]].concat(),
        [vec![1], vec![0; 32]].concat(),
        vec![255; 64],
    ] {
        let javascript = format!("{{cycles_sent:{{some:new Uint8Array({magnitude:?})}}}}");
        f.add(
            MagnitudeEvidence {
                cycles_sent: Some(Bytes::<64>::new(magnitude).unwrap()),
            },
            &javascript,
        );
    }
    f.checks.push_str(r#"
function magnitudeToBigInt(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length > 64 || bytes[0] === 0) throw new Error('unsigned magnitude');
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  return value;
}
function bigIntToMagnitude(value) {
  if (typeof value !== 'bigint' || value < 0n || (value >> 512n) !== 0n) throw new Error('unsigned magnitude');
  const bytes = [];
  while (value !== 0n) { bytes.push(Number(value & 255n)); value >>= 8n; }
  return Uint8Array.from(bytes.reverse());
}
for (const value of [0n, (1n << 128n)-1n, 1n << 128n, 1n << 256n, (1n << 512n)-1n]) {
  assert.equal(magnitudeToBigInt(bigIntToMagnitude(value)), value);
}
assert.throws(() => bigIntToMagnitude(-1n));
assert.throws(() => bigIntToMagnitude(1n << 512n));
assert.equal(magnitudeToBigInt(new Uint8Array()), 0n);
assert.equal(magnitudeToBigInt(new Uint8Array(Array(16).fill(255))), (1n << 128n) - 1n);
assert.equal(magnitudeToBigInt(new Uint8Array([1,...Array(16).fill(0)])), 1n << 128n);
assert.equal(magnitudeToBigInt(new Uint8Array([1,...Array(32).fill(0)])), 1n << 256n);
assert.throws(() => magnitudeToBigInt(new Uint8Array([0])));
assert.throws(() => magnitudeToBigInt(new Uint8Array([0,1])));
assert.throws(() => magnitudeToBigInt(new Uint8Array(65).fill(1)));
"#);
    f.mutation_data.flush().unwrap();
    let codecs = (0..f.count)
        .map(|index| format!("m{index}"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(
        f.checks,
        "await checkMutations({{{codecs}}}, {});",
        f.mutations
    )
    .unwrap();
    writeln!(f.checks, "console.log('Rust/TypeScript conformance: {} vectors, both directions, truncation and identity rejection passed');", f.count).unwrap();
    writeln!(f.checks, "console.log('Differential mutations: {} Rust/TypeScript acceptance and canonical re-encode checks passed');", f.mutations).unwrap();
    for name in ["JSON", "types", "createCodec", "globalThis"] {
        writeln!(f.checks, "{{ const module = await import('./compiled/shadow-{name}.js'); const codec = module['{name}']; const value = new Int16Array([-32768, 32767]); assert.deepEqual(codec.decodeRequest(codec.encodeRequest(value)), value); }}").unwrap();
    }
    f.checks.push_str(&large_bytes::generate(&f.directory));
    f.checks.push_str(&native_schema::generate(&f.directory));
    f.checks.push_str(&admission::generate(&f.directory));
    f.checks.push_str(include_str!("writer.mjs"));
    f.checks.push_str(&payload::generate(&f.directory));
    fs::write(f.directory.join("checks.mjs"), f.checks).unwrap();
}
