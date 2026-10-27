//! Focused host-generation timing; metadata preparation and hashing are excluded.
use ic_wire::{Limits, typescript};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, hint::black_box, io::Write, path::PathBuf, time::Instant};

fn generation_error(error: typescript::GenerationError) -> std::io::Error {
    std::io::Error::other(format!("generation failed: {error:?}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = clap::Command::new("caller-generation")
        .arg(
            clap::Arg::new("output")
                .long("output")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            clap::Arg::new("samples")
                .long("samples")
                .default_value("16")
                .value_parser(clap::value_parser!(u32).range(1..=128)),
        )
        .arg(
            clap::Arg::new("iterations")
                .long("iterations")
                .default_value("64")
                .value_parser(clap::value_parser!(u32).range(1..=4096)),
        )
        .get_matches();
    let output = args.remove_one::<PathBuf>("output").unwrap();
    let samples = args.remove_one::<u32>("samples").unwrap();
    let iterations = args.remove_one::<u32>("iterations").unwrap();
    fs::create_dir(&output)?;
    let mut results = fs::File::create(output.join("samples.csv"))?;
    writeln!(
        results,
        "family,types_and_methods,sample,iterations,nanos_per_generation,source_bytes,source_sha256"
    )?;
    for count in [1, 128] {
        let mut types = BTreeMap::new();
        let mut methods = Vec::new();
        let limits = Limits {
            bytes: 4096,
            allocation: 65536,
            nodes: 8192,
            depth: 32,
        };
        for index in 0..count {
            let name = format!("Record{index:03}");
            types.insert(
                name.clone(),
                json!({"k":"record","fields":[
                    {"name":"id","value":{"k":"u","bits":64}},
                    {"name":"data","value":{"k":"bytes","max":256}},
                    {"name":"label","value":{"k":"text","max":64}}
                ]}),
            );
            let mut method = typescript::method_limits(limits, limits).map_err(generation_error)?;
            method["name"] = json!(format!("method{index:03}"));
            method["id"] = json!((index + 1).to_string());
            method["mode"] = json!("update");
            method["request"] = json!({"k":"ref","name":name});
            method["response"] = json!({"k":"unit"});
            methods.push(method);
        }
        for family in ["framed", "payload"] {
            #[cfg(feature = "caller-output-baseline")]
            if family == "payload" {
                continue;
            }
            let generate = || -> Result<String, typescript::GenerationError> {
                let types: &BTreeMap<String, Value> = black_box(&types);
                let methods = black_box(methods.as_slice());
                let acknowledgements = BTreeMap::new();
                let diagnostics = |report| eprint!("{report}");
                #[cfg(not(feature = "caller-output-baseline"))]
                if family == "payload" {
                    return typescript::generate_payload_registry(
                        types,
                        methods,
                        "./wire-runtime.js",
                        &acknowledgements,
                        diagnostics,
                    );
                }
                typescript::generate_registry(
                    types,
                    methods,
                    "./wire-runtime.js",
                    &acknowledgements,
                    diagnostics,
                )
            };
            let expected = generate().map_err(generation_error)?;
            let sha256 = format!("{:x}", Sha256::digest(expected.as_bytes()));
            fs::write(output.join(format!("{family}-{count}.ts")), &expected)?;
            for _ in 0..4 {
                drop(black_box(generate().map_err(generation_error)?));
            }
            for sample in 0..samples {
                let begin = Instant::now();
                for _ in 0..iterations {
                    drop(black_box(generate().map_err(generation_error)?));
                }
                let elapsed = begin.elapsed().as_nanos() as f64 / f64::from(iterations);
                writeln!(
                    results,
                    "{family},{count},{sample},{iterations},{elapsed},{},{sha256}",
                    expected.len()
                )?;
            }
            assert_eq!(generate().map_err(generation_error)?, expected);
        }
    }
    Ok(())
}
