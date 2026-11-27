use super::{grouped_messages, limits, run_typescript};
use crate::support::{decode, encode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::BTreeMap};
use wire::Encoding;
use wire::typescript::{Method, PayloadLimits, SchemaLimits, ValidatedRegistry, Wire};

#[derive(Debug, PartialEq, Serialize, Deserialize, Wire)]
#[wire(crate = "wire", fixed)]
struct Pair(u8, u16);
#[derive(Debug, PartialEq, Serialize, Deserialize, Wire)]
#[wire(crate = "wire")]
enum Choice {
    Empty(),
    Pair(u8, u16),
}
type Outcome = Result<[u8; 2], (u16, bool)>;
fn definition<T: Wire + ?Sized>(name: &str) -> Value {
    let graph = ValidatedRegistry::from_type::<T>(name, SchemaLimits::default()).unwrap();
    serde_json::from_str::<Value>(&graph.metadata_json().unwrap()).unwrap()[name].clone()
}
#[test]
fn standard_shapes_preserve_prefixes_tags_and_traversal() {
    let types = BTreeMap::from([
        ("Outcome".to_owned(), definition::<Outcome>("Outcome")),
        ("Pair".to_owned(), definition::<Pair>("Pair")),
        ("Choice".to_owned(), definition::<Choice>("Choice")),
        ("Slice".to_owned(), definition::<[u16]>("Slice")),
        ("Text".to_owned(), definition::<Cow<'_, str>>("Text")),
        ("EmptyArray".to_owned(), definition::<[u8; 0]>("EmptyArray")),
    ]);
    assert_eq!(
        types["Outcome"],
        json!({"k":"result","ok":{"k":"array","max":2,"item":{"k":"u","bits":8}},"err":{"k":"tuple","items":[{"k":"u","bits":16},{"k":"bool"}]}})
    );
    let graph =
        ValidatedRegistry::from_metadata(&grouped_messages(types.clone()), SchemaLimits::default())
            .unwrap();
    let policy = PayloadLimits {
        bytes: 128,
        nodes: 32,
        depth: 8,
    };
    let methods: Vec<_> = [
        "MessageOutcome",
        "MessagePair",
        "MessageChoice",
        "MessageSlice",
        "MessageText",
        "MessageEmptyArray",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, name)| Method {
        export: ["outcome", "pair", "choice", "slice", "text", "emptyArray"][index],
        id: index as u64 + 1,
        request: name,
        response: name,
        request_limits: policy,
        response_limits: policy,
    })
    .collect();
    let cases = json!([
        {"method":"outcome","root":"Outcome","value":{"ok":[0,255]},"bytes":[0,0,255],"nodes":4,"depth":3},
        {"method":"outcome","root":"Outcome","value":{"err":[128,true]},"bytes":[1,128,1,1],"nodes":4,"depth":3},
        {"method":"pair","root":"Pair","value":[7,128],"bytes":[7,128,1],"nodes":3,"depth":2},
        {"method":"choice","root":"Choice","value":{"tag":"Empty","value":[]},"bytes":[0],"nodes":2,"depth":2},
        {"method":"choice","root":"Choice","value":{"tag":"Pair","value":[7,128]},"bytes":[1,7,128,1],"nodes":4,"depth":3},
        {"method":"slice","root":"Slice","value":[7,128],"bytes":[2,7,128,1],"nodes":3,"depth":2},
        {"method":"text","root":"Text","value":"é","bytes":[2,195,169],"nodes":1,"depth":1},
        {"method":"emptyArray","root":"EmptyArray","value":[],"bytes":[],"nodes":1,"depth":1}
    ]);
    let fixture = json!({"types":types,"cases":cases,"module":graph.payload_module("outcomePayload","MessageOutcome","./runtime-adapter.ts",policy, &Default::default(), |report| eprint!("{report}")).unwrap(),"frameModule":graph.method_module(&methods,"./runtime-adapter.ts", &Default::default(), |report| eprint!("{report}")).unwrap()});
    let output: Vec<Vec<u8>> =
        serde_json::from_value(run_typescript(&fixture, include_str!("exchange.mjs"))).unwrap();
    macro_rules! check {
        ($index:expr, $ty:ty, $value:expr) => {{
            let value: $ty = $value;
            assert_eq!(
                encode(&value, Encoding::Payload, limits()).unwrap(),
                output[$index]
            );
            assert_eq!(
                decode::<$ty>(&output[$index], Encoding::Payload, limits()).unwrap(),
                value
            );
        }};
    }
    check!(0, Outcome, Ok([0, 255]));
    check!(1, Outcome, Err((128, true)));
    check!(2, Pair, Pair(7, 128));
    check!(3, Choice, Choice::Empty());
    check!(4, Choice, Choice::Pair(7, 128));
    check!(5, Vec<u16>, vec![7, 128]);
    check!(6, Cow<'_, str>, Cow::Borrowed("é"));
    check!(7, [u8; 0], []);
    // Result tags are u32 varints, not the single-byte Option discriminator.
    assert_eq!(
        decode::<Outcome>(&[128, 0, 0, 255, 0], Encoding::Payload, limits()).unwrap(),
        Ok([0, 255])
    );
    assert_eq!(
        decode::<Outcome>(&[129, 0, 128, 1, 1, 0], Encoding::Payload, limits()).unwrap(),
        Err((128, true))
    );
}
mod bytes;
mod floats;
mod packed;
mod platform;
