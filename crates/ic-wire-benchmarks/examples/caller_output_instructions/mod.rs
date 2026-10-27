//! Paired encoder measurements with an explicitly prepared owned PocketIC server.
use ic_platform::{candid::contract::Principal, pocket_ic::PocketIcBuilder};
use ic_testkit::pic::PocketIcStartupConfig;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn rejected(error: impl std::fmt::Debug) -> std::io::Error {
    std::io::Error::other(format!("PocketIC rejected fixture operation: {error:?}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = clap::Command::new("caller-output-instructions")
        .arg(
            clap::Arg::new("baseline")
                .long("baseline")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            clap::Arg::new("candidate")
                .long("candidate")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            clap::Arg::new("output")
                .long("output")
                .required(true)
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            clap::Arg::new("samples")
                .long("samples")
                .default_value("8")
                .value_parser(clap::value_parser!(u32).range(1..=64)),
        )
        .get_matches();
    let baseline = args.remove_one::<PathBuf>("baseline").unwrap();
    let candidate = args.remove_one::<PathBuf>("candidate").unwrap();
    let output = args.remove_one::<PathBuf>("output").unwrap();
    let samples = args.remove_one::<u32>("samples").unwrap();
    fs::create_dir(&output)?;
    assert!(
        std::env::var_os("IC_TESTKIT_POCKET_IC_URL").is_none(),
        "use an owned server"
    );
    let server =
        PathBuf::from(std::env::var_os("POCKET_IC_BIN").expect("prepared PocketIC binary"));
    let owned = PocketIcStartupConfig::from_env(Duration::from_secs(60))?
        .build_instance(PocketIcBuilder::new().with_application_subnet())?;
    let pic = owned.instance();
    let mut installed = Vec::new();
    let mut artifacts = Vec::new();
    for (label, path) in [("baseline", baseline), ("candidate", candidate)] {
        let wasm = fs::read(path)?;
        let id = pic.create_canister();
        pic.add_cycles(id, 100_000_000_000_000);
        pic.install_canister(id, wasm.clone(), vec![], None);
        installed.push((label, id));
        artifacts
            .push(json!({"label":label,"sha256":hash(&wasm),"uncompressed_wasm_bytes":wasm.len()}));
    }
    fs::write(
        output.join("identity.json"),
        serde_json::to_vec_pretty(&json!({
            "artifacts":artifacts,"server_sha256":hash(&fs::read(server)?),"samples":samples,
            "lock_sha256":hash(include_bytes!("../../../../Cargo.lock")),
            "fixture_sha256":hash(include_bytes!("../caller_output/mod.rs")),
            "scope":"Counter0 warm/amortized encoding, including output disposal and loop overhead. No subtraction. Preparation and IC transport excluded; updates avoid query-result caching. Wasm capacity is not live heap/RSS."
        }))?,
    )?;
    let mut file = fs::File::create(output.join("samples.jsonl"))?;
    let mut summary = fs::File::create(output.join("summary.csv"))?;
    writeln!(
        summary,
        "variant,case,count,operation,bytes,instructions_per_op,wasm_capacity_bytes,host_millis"
    )?;
    for (kind, name, lengths) in [
        (0u32, "unit", vec![0u32]),
        (1, "u64", vec![0, 1, 2]),
        (2, "u8", vec![0, 16, 4096, 262144]),
        (3, "bytes", vec![0, 16, 4096, 262144]),
        (4, "u64-list", vec![0, 16, 512, 8192]),
        (5, "records", vec![0, 1, 32, 256]),
    ] {
        for n in lengths {
            let repetitions = if n > 65536 {
                1
            } else if n > 4096 {
                4
            } else {
                16
            };
            let input: Vec<_> = [kind, n, repetitions]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            let mut expected: Option<Vec<u8>> = None;
            for sample in 0..samples {
                let mut order = installed.clone();
                if sample % 2 != 0 {
                    order.reverse();
                }
                for (label, id) in order {
                    let start = Instant::now();
                    let reply = pic
                        .update_call(
                            id,
                            Principal::anonymous(),
                            "benchmark_output",
                            input.clone(),
                        )
                        .map_err(rejected)?;
                    let millis = start.elapsed().as_secs_f64() * 1000.0;
                    assert!(reply.len() >= 48);
                    let words: Vec<_> = reply[..48]
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|b| u64::from_le_bytes(*b))
                        .collect();
                    assert_eq!(reply.len() - 48, words[4] as usize);
                    if let Some(bytes) = &expected {
                        assert_eq!(bytes, &reply[48..]);
                    } else {
                        expected = Some(reply[48..].to_vec());
                    }
                    assert!(words[..4].iter().all(|count| *count > 0));
                    writeln!(
                        file,
                        "{}",
                        json!({"variant":label,"case":name,"count":n,"sample":sample,"repetitions":repetitions,"counters":words[..4],"bytes":words[4],"wasm_capacity_bytes":words[5]*65536,"payload_sha256":hash(&reply[48..]),"host_millis":millis})
                    )?;
                    for (operation, count) in ["fresh", "reused", "slice", "scoped"]
                        .into_iter()
                        .zip(&words[..4])
                    {
                        let operation = match (label, operation) {
                            ("baseline", "slice") => "slice-control-reused",
                            ("baseline", "scoped") => "scoped-control-fresh",
                            _ => operation,
                        };
                        {
                            writeln!(
                                summary,
                                "{label},{name},{n},{operation},{},{},{},{millis}",
                                words[4],
                                *count as f64 / repetitions as f64,
                                words[5] * 65536
                            )?;
                        }
                    }
                }
            }
        }
    }
    for (_, id) in installed {
        pic.stop_canister(id, None).map_err(rejected)?;
        pic.delete_canister(id, None).map_err(rejected)?;
    }
    Ok(())
}
