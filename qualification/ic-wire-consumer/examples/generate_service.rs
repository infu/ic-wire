//! Generate the paired method codecs and service bindings for the raw counter.
use std::{fs, path::PathBuf};
use wire::typescript::{Method, PayloadLimits, SchemaLimits, ServiceMethod, ValidatedRegistry};

#[path = "records/word.rs"]
mod word;

fn main() {
    let directory = PathBuf::from(std::env::args_os().nth(1).expect("output directory"));
    let graph =
        ValidatedRegistry::from_type::<word::Word>("Word", SchemaLimits::default()).unwrap();
    let limits = PayloadLimits {
        bytes: word::PAYLOAD_BYTES as u64,
        nodes: 4,
        depth: 3,
    };
    let endpoints = [("read", "query"), ("add", "update")];
    let codecs = endpoints.map(|(name, _)| Method {
        export: name,
        id: wire::method_id(name),
        request: "Word",
        response: "Word",
        request_limits: limits,
        response_limits: limits,
    });
    let bindings = endpoints.map(|(name, mode)| ServiceMethod {
        binding: name,
        endpoint: name,
        mode,
        codec_export: name,
        codec_module: "./methods.ts",
    });
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("methods.ts"),
        graph
            .method_module(&codecs, "./runtime.ts", &Default::default(), |report| {
                assert!(report.is_clean())
            })
            .unwrap(),
    )
    .unwrap();
    fs::write(
        directory.join("service.ts"),
        graph.service_module(&bindings).unwrap(),
    )
    .unwrap();
    fs::write(directory.join("runtime.ts"), wire::typescript::RUNTIME).unwrap();
}
