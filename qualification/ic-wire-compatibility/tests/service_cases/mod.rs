use super::{message_schema, run_typescript};
use serde_json::json;
use wire::typescript::{
    GenerationError, Method, PayloadLimits, SchemaLimits, ServiceMethod, ValidatedRegistry,
};

fn graph(output_bytes: usize, write_method: bool) -> ValidatedRegistry {
    ValidatedRegistry::from_metadata(
        &[
            (
                "Request".into(),
                message_schema(if write_method {
                    json!({"k":"u","bits":8})
                } else {
                    json!({"k":"text"})
                }),
            ),
            (
                "Response".into(),
                message_schema(if write_method {
                    json!({"k":"result","ok":{"k":"u","bits":8},"err":{"k":"bool"}})
                } else {
                    json!({"k":"result","ok":{"k":"bool"},"err":{"k":"u","bits":8}})
                }),
            ),
        ]
        .into(),
        SchemaLimits {
            output_bytes,
            ..Default::default()
        },
    )
    .unwrap()
}

fn methods() -> [ServiceMethod<'static>; 3] {
    [
        ("fetch", "read-record", "query", "./client.ts", "codec"),
        (
            "save",
            "write\"record",
            "update",
            "./frame-client.ts",
            "codec",
        ),
        (
            "lookup",
            "nested.read",
            "composite_query",
            "./client.ts",
            "nested",
        ),
    ]
    .map(
        |(binding, endpoint, mode, codec_module, codec_export)| ServiceMethod {
            binding,
            codec_export,
            endpoint,
            mode,
            codec_module,
        },
    )
}

#[test]
fn mounted_methods_use_existing_codecs_and_injected_transport() {
    let read = graph(65536, false);
    let write = graph(65536, true);
    let [fetch, save, lookup] = methods();
    let limits = PayloadLimits {
        bytes: 4,
        nodes: 8,
        depth: 4,
    };
    let codec = |export, id| Method {
        export,
        id,
        request: "Request",
        response: "Response",
        request_limits: limits,
        response_limits: limits,
    };
    let render = |registry: &ValidatedRegistry, methods: &[Method<'_>]| {
        registry
            .method_module(methods, "./codec-runtime.ts", &Default::default(), |_| {})
            .unwrap()
    };
    let fixture = json!({
        "module":render(&read, &[codec("codec",1),codec("nested",3)]),
        "frameModule":render(&write, &[codec("codec",2)]),
        "serviceModule":read.service_module(&[fetch,save,lookup]).unwrap(),
        "typeAssertions":include_str!("types.ts"),
    });
    assert_eq!(
        run_typescript(&fixture, include_str!("transport.mjs")),
        json!(true)
    );
}

#[test]
fn service_generation_admits_names_modes_and_total_output() {
    let graph_large = graph(65536, false);
    let [a, b, c] = methods();
    let source = graph_large.service_module(&[a, b, c]).unwrap();
    assert_eq!(
        graph(source.len(), false)
            .service_module(&[a, b, c])
            .unwrap(),
        source
    );
    assert_eq!(
        graph(source.len() - 1, false).service_module(&[a, b, c]),
        Err(GenerationError::Complexity)
    );
    for bad in [
        ServiceMethod {
            mode: "notify",
            ..b
        },
        ServiceMethod {
            endpoint: a.endpoint,
            ..b
        },
        ServiceMethod {
            binding: a.binding,
            ..b
        },
        ServiceMethod {
            codec_export: "bad-name",
            ..b
        },
        ServiceMethod {
            codec_module: "",
            ..b
        },
    ] {
        assert_eq!(
            graph_large.service_module(&[a, bad]),
            Err(GenerationError::Schema)
        );
    }
}
