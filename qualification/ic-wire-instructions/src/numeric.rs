//! Installed numeric-list comparison using the driver's managed instance.
use ic_testkit::pocket_ic::{CanisterId, nonblocking::PocketIc};
use serde_json::{Value, json};
use std::{error::Error, fs, path::Path};

pub(super) async fn run(pic: &PocketIc, directory: &Path) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut rows = Vec::new();
    for profile in ["wire-size", "wire-speed"] {
        for codec in ["ordinary", "bounded", "packed", "candid"] {
            let wasm = fs::read(directory.join(format!("{profile}-{codec}.wasm")))?;
            let size = wasm.len();
            let hash = super::hash(&wasm);
            let id = pic.create_canister().await;
            pic.add_cycles(id, 100_000_000_000_000).await;
            pic.install_canister(id, wasm, vec![], None).await;
            for sample in 1..=3 {
                let reply = pic
                    .query_call(id, CanisterId::from_slice(&[sample]), "measure", vec![])
                    .await
                    .map_err(|e| format!("numeric query: {e:?}"))?;
                if reply.len() != 5 * 3 * 24 {
                    return Err("invalid numeric reply length".into());
                }
                for ((len, distribution), chunk) in [1_u64, 16, 127, 128, 4096]
                    .into_iter()
                    .flat_map(|len| ["small", "sparse-wide", "wide"].map(|d| (len, d)))
                    .zip(reply.as_chunks::<24>().0)
                {
                    let [payload, encode, decode] = chunk
                        .as_chunks::<8>()
                        .0
                        .try_into()
                        .map(|v: &[[u8; 8]; 3]| v.map(u64::from_le_bytes))?;
                    let wide = match distribution {
                        "small" => 0,
                        "sparse-wide" => len.div_ceil(16),
                        _ => len,
                    };
                    let prefix = |n: u64| u64::from(n >= 128) + u64::from(n >= 16384) + 1;
                    let expected = match codec {
                        "ordinary" | "bounded" => prefix(len) + len + 4 * wide + 1,
                        "packed" => prefix(4 * len) + 4 * len + 1,
                        _ => 9 + prefix(len) + 4 * len,
                    };
                    if payload != expected || encode == 0 || decode == 0 {
                        return Err("invalid numeric observation".into());
                    }
                    rows.push(json!({"profile":profile,"codec":codec,"distribution":distribution,"elements":len,"sample":sample,"wasm_bytes":size,"artifact_sha256":hash,"payload_bytes":payload,"encode_instructions":encode,"decode_instructions":decode}));
                }
            }
            pic.stop_canister(id, None)
                .await
                .map_err(|e| format!("stop: {e:?}"))?;
            pic.delete_canister(id, None)
                .await
                .map_err(|e| format!("delete: {e:?}"))?;
        }
    }
    Ok(rows)
}
