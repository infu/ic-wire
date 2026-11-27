//! Installed counters over original PocketIC types; the runner owns the server.
#![forbid(unsafe_code)]
mod numeric;

use ic_testkit::{
    pocket_ic::{CanisterId, PocketIcBuilder},
    server::{Deletion, Limits as ServerLimits, PreparedServer, Server},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, time::Duration};

const OPERATIONS: [&str; 7] = [
    "encode_request",
    "decode_request",
    "encode_response",
    "decode_response",
    "bad_header",
    "truncated",
    "loop",
];

fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(result, "{byte:02x}").unwrap();
    }

    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("provide artifact directory")?,
    );
    let server = PathBuf::from(
        std::env::var_os("POCKET_IC_BIN").ok_or("set explicit prepared POCKET_IC_BIN")?,
    );
    if std::env::var_os("IC_TESTKIT_POCKET_IC_URL").is_some() {
        return Err("owned prepared server required".into());
    }
    if std::env::var_os("IC_WIRE_POCKET_IC_URL").is_some() {
        return Err("external endpoint overrides are not supported".into());
    }
    if server.file_name().is_none_or(|name| name != "pocket-ic") {
        return Err("prepared server must be named pocket-ic".into());
    }
    let prepared = PreparedServer::check(server.parent().ok_or("server directory required")?)?;
    let server_hash = ic_testkit::server::host_asset()?.binary_sha256;
    let version = std::process::Command::new(prepared.path())
        .arg("--version")
        .output()?;
    if !version.status.success() {
        return Err("server version failed".into());
    }

    let mut server_owner = Server::start(
        prepared,
        ServerLimits {
            startup: Duration::from_secs(60),
            lifetime: Duration::from_secs(300),
            stdout_bytes: 65536,
            stderr_bytes: 65536,
        },
    )?;
    let mut instance = server_owner
        .instance(
            PocketIcBuilder::new().with_application_subnet(),
            Duration::from_secs(60),
        )
        .await?;
    let pic = instance.client_mut();
    let instance_id = pic.instance_id;
    let numeric_mode =
        std::env::args_os().nth(2).as_deref() == Some(std::ffi::OsStr::new("--numeric"));
    let service_mode =
        std::env::args_os().nth(2).as_deref() == Some(std::ffi::OsStr::new("--service"));
    let result = async {
        if service_mode {
            let wasm = fs::read(directory.join("raw_service.wasm"))?;
            let canister = pic.create_canister().await;
            pic.add_cycles(canister, 100_000_000_000_000).await;
            pic.install_canister(canister, wasm, vec![], None).await;
            let gateway = pic.make_live(None).await;
            let outcome = std::process::Command::new("node")
                .args(["--experimental-transform-types", "qualification/typescript/check-service.mjs", gateway.as_str(), &canister.to_text()])
                .status();
            pic.stop_live().await;
            if !outcome?.success() { return Err("raw service check failed".into()); }
            return Ok(Vec::new());
        }
        if numeric_mode {
            return numeric::run(pic, &directory).await;
        }
        let expected = [73,87,7,0,0,0,0,0,0,0,0,128,1,6,115,101,110,115,111,114,2,0,255,6,0,128,0,0,255,127,2,1,128,1,0,
            73,87,7,0,0,0,0,0,0,0,1,129,1,1,0];
        let mut rows = Vec::new();
        let mut caller = 0_u64;
        for profile in ["wire-size", "wire-speed"] {
            let wasm = fs::read(directory.join(format!("{profile}.wasm")))?;
            let artifact_hash = hash(&wasm);
            let canister = pic.create_canister().await;
            pic.add_cycles(canister, 100_000_000_000_000).await;
            pic.install_canister(canister, wasm, vec![], None).await;
            if pic.query_call(canister, CanisterId::anonymous(), "measure", vec![1]).await.is_ok() {
                return Err("counter admitted nonempty arguments".into());
            }
            let mut samples = Vec::new();
            for _ in 0..3 {
                caller += 1;
                let reply = pic.query_call(canister, CanisterId::from_slice(&caller.to_le_bytes()), "measure", vec![]).await.map_err(|e| format!("query failed: {e:?}"))?;
                if reply.len() != 56 + expected.len() || reply[56..] != expected { return Err("counter frame oracle mismatch".into()); }
                let counts: Vec<u64> = reply[..56].as_chunks::<8>().0.iter().map(|bytes| u64::from_le_bytes(*bytes)).collect();
                samples.push(counts);
            }
            pic.stop_canister(canister, None).await.map_err(|e| format!("stop: {e:?}"))?;
            pic.delete_canister(canister, None).await.map_err(|e| format!("delete: {e:?}"))?;
            for (operation, name) in OPERATIONS.iter().enumerate() {
                let values: Vec<u64> = samples.iter().map(|s| s[operation]).collect();
                let mut sorted = values.clone(); sorted.sort_unstable();
                rows.push(json!({"profile":profile,"operation":name,"samples":values,"median_batch":sorted[1],"repetitions":8,"artifact_sha256":artifact_hash}));
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(rows)
    }.await;
    let deletion = instance.close(Duration::from_secs(30)).await;
    let observation = std::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--noproxy",
            "*",
            "--max-time",
            "10",
            "--max-filesize",
            "4096",
        ])
        .arg(format!("{}instances", server_owner.endpoint()))
        .output();
    let stopped = server_owner.stop()?;
    fs::write(directory.join("server.stdout"), &stopped.output.stdout)?;
    fs::write(directory.join("server.stderr"), &stopped.output.stderr)?;
    fs::write(directory.join("server-process.txt"), format!("{stopped:?}"))?;
    if deletion != Deletion::Attempted
        || stopped.abandoned != 0
        || !stopped.process.cleanup.is_empty()
        || stopped.process.status.is_none()
    {
        return Err("simulator cleanup did not complete; inspect server-process.txt".into());
    }
    let observation = observation?;
    if !observation.status.success() {
        return Err("instance deletion observation failed".into());
    }
    fs::write(directory.join("instances.json"), &observation.stdout)?;
    let instances: Vec<String> = serde_json::from_slice(&observation.stdout)?;
    if instances != ["Deleted"] {
        return Err("owned instance remains on server".into());
    }
    let rows = result?;
    if service_mode {
        return Ok(());
    }
    let mut report = json!({"counter":"ic0.performance_counter(0)","instance_id":instance_id,"server_version":String::from_utf8(version.stdout)?,"server_sha256":server_hash,"rows":rows,
        "qualification_lock_sha256":hash(include_bytes!("../../Cargo.lock")),"driver_sha256":hash(include_bytes!("main.rs")),"guest_sha256":hash(include_bytes!("../../ic-wire-consumer/src/ic.rs")),
        "consumer_sha256":hash(include_bytes!("../../ic-wire-consumer/src/lib.rs")),"meter_sha256":hash(include_bytes!("../../../crates/ic-wire/src/meter.rs")),
        "compiler":String::from_utf8(std::process::Command::new("rustc").arg("-Vv").output()?.stdout)?});
    if numeric_mode {
        let fields = report.as_object_mut().unwrap();
        for field in ["guest_sha256", "consumer_sha256", "meter_sha256"] {
            fields.remove(field);
        }
        fields.insert(
            "probe_sha256".into(),
            json!(hash(include_bytes!(
                "../../ic-wire-compatibility/examples/numeric/guest.rs"
            ))),
        );
        fields.insert(
            "workload_sha256".into(),
            json!(hash(include_bytes!(
                "../../ic-wire-compatibility/examples/numeric/workload.rs"
            ))),
        );
        fields.insert(
            "numeric_driver_sha256".into(),
            json!(hash(include_bytes!("numeric.rs"))),
        );
    }
    fs::write(
        directory.join("instructions.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
