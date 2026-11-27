//! Bidirectional generated-client qualification for the sparse extension design.
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "The generator only writes fixtures")
)]
mod fixture;
mod inline_exchange;
mod nested_inline_exchange;
mod recovery_exchange;
mod schema;
mod tuple_exchange;

use std::{collections::BTreeMap, error::Error, path::Path};
use wire::typescript::{Method, PayloadLimits, RUNTIME};

fn generate<const REV: u8>(directory: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let graph = schema::graph::<REV>().map_err(|error| format!("{error:?}"))?;
    let limits = PayloadLimits {
        bytes: (fixture::LIMITS.bytes - 11) as u64,
        nodes: fixture::LIMITS.nodes as u64,
        depth: fixture::LIMITS.depth as u64,
    };
    let method = Method {
        export: "packet",
        id: fixture::METHOD,
        request: "Batch",
        response: "Batch",
        request_limits: limits,
        response_limits: limits,
    };
    let mut reports = Vec::new();
    let source = graph
        .method_module(&[method], "./runtime.ts", &BTreeMap::new(), |report| {
            reports.push(report)
        })
        .map_err(|error| format!("{error:?}"))?;
    assert!(
        reports.iter().all(|report| report.is_clean()),
        "{reports:?}"
    );
    std::fs::write(directory.join(format!("{name}.ts")), source)?;
    let manifests = graph
        .api_manifests(&[(method.export, method)])
        .map_err(|error| format!("{error:?}"))?;
    std::fs::write(
        directory.join(format!("{name}.json")),
        &manifests[method.export],
    )?;
    let mut frame = Vec::new();
    fixture::encode::<REV>(4, fixture::LIMITS, &mut frame)?;
    std::fs::write(directory.join(format!("{name}.bin")), frame)?;
    Ok(())
}

fn emit(directory: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("package.json"), r#"{"type":"module"}"#)?;
    std::fs::write(directory.join("runtime.ts"), RUNTIME)?;
    std::fs::write(directory.join("exchange.mjs"), include_str!("exchange.mjs"))?;
    generate::<0>(directory, "a")?;
    generate::<1>(directory, "b")?;
    generate::<2>(directory, "c")?;
    recovery_exchange::emit(directory)?;
    tuple_exchange::emit(directory)?;
    inline_exchange::emit(directory)?;
    nested_inline_exchange::emit(directory)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let directory = arguments.next().ok_or("expected output directory")?;
    if arguments.next().is_some() {
        return Err("expected exactly one output directory".into());
    }
    emit(Path::new(&directory))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fixture::{State, decode, encode};
    use wire::{Error as WireError, LimitKind};

    fn check_typescript(directory: &Path) -> Result<(), Box<dyn Error>> {
        let compiler = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../typescript/node_modules/typescript/bin/tsc");
        if !compiler.is_file() {
            return Err("Install the locked compiler with npm ci --prefix qualification/typescript --ignore-scripts".into());
        }
        let mut files = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let name = entry?.file_name();
            if Path::new(&name).extension() == Some(std::ffi::OsStr::new("ts")) {
                files.push(name);
            }
        }
        files.sort_unstable();
        let output = std::process::Command::new("node")
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
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "TypeScript static check failed\n{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(())
    }

    #[test]
    fn caller_buffer_produces_the_same_evolved_frames() {
        fn check<const REV: u8>(count: usize, dense: bool) {
            let mut owned = Vec::new();
            fixture::encode_batch::<REV>(count, dense, fixture::LIMITS, &mut owned).unwrap();
            let mut buffer = vec![0; owned.len()];
            let frame =
                fixture::encode_batch_to_slice::<REV>(count, dense, fixture::LIMITS, &mut buffer)
                    .unwrap();
            assert_eq!(frame, owned);
            let short = &mut buffer[..owned.len() - 1];
            assert_eq!(
                fixture::encode_batch_to_slice::<REV>(count, dense, fixture::LIMITS, short),
                Err(WireError::Limit(LimitKind::Output))
            );
            let limits = wire::Limits {
                bytes: owned.len() - 1,
                ..fixture::LIMITS
            };
            assert_eq!(
                fixture::encode_batch_to_slice::<REV>(count, dense, limits, &mut buffer),
                Err(WireError::Limit(LimitKind::Bytes))
            );
        }
        check::<0>(0, false);
        check::<1>(1_024, false);
        check::<1>(1_024, true);
        check::<2>(128, false);
    }

    #[test]
    fn registry_rejects_ambiguous_evolution_ids() {
        use wire::typescript::__private::json;
        use wire::typescript::{GenerationError, SchemaContext, SchemaRegistry, Wire};

        struct Invalid<const CASE: usize>;
        impl<const CASE: usize> Wire for Invalid<CASE> {
            fn schema(
                ctx: &mut SchemaContext,
            ) -> Result<wire::typescript::__private::Value, GenerationError> {
                let unit = ctx.describe::<()>()?;
                let cases = [
                    json!({"k":"open-variant","variants":[{"id":0,"name":"Zero","value":unit}]}),
                    json!({"k":"open-variant","variants":[{"id":2147483648_u32,"name":"Overflow","value":unit}]}),
                    json!({"k":"open-variant","variants":[
                        {"id":1,"name":"First","value":unit},
                        {"id":1,"name":"Other","value":unit}
                    ]}),
                    json!({"k":"extension-record","fields":[{"name":"x","value":unit}],"extensions":[
                        {"id":1,"name":"x","value":unit,"required":false}
                    ]}),
                    json!({"k":"extension-record","fields":[],"extensions":[
                        {"id":2,"name":"x","value":unit,"required":false},
                        {"id":1,"name":"y","value":unit,"required":false}
                    ]}),
                ];
                Ok(cases[CASE].clone())
            }
        }
        fn verify<const CASE: usize>() {
            let mut registry = SchemaRegistry::new(Default::default()).unwrap();
            registry.register::<Invalid<CASE>>("Invalid").unwrap();
            assert!(matches!(registry.finish(), Err(GenerationError::Schema)));
        }
        verify::<0>();
        verify::<1>();
        verify::<2>();
        verify::<3>();
        verify::<4>();
    }

    #[test]
    fn nested_rust_readers_preserve_neighbors_and_borrow_original_input() {
        let mut bytes = Vec::new();
        encode::<2>(4, fixture::LIMITS, &mut bytes).unwrap();
        let a = decode::<0>(&bytes, fixture::LIMITS).unwrap().into_view();
        let b = decode::<1>(&bytes, fixture::LIMITS).unwrap().into_view();
        let c = decode::<2>(&bytes, fixture::LIMITS).unwrap().into_view();
        assert_eq!(a.groups[0].rows[0].state, None);
        assert_eq!(
            a.groups[0].rows[1].state,
            Some(State::Detail {
                message: "steady Δ",
                audit: None
            })
        );
        assert_eq!(
            c.groups[0].rows[1].state,
            Some(State::Detail {
                message: "steady Δ",
                audit: Some(&[0, 255, 9, 1])
            })
        );
        assert_eq!(
            c.groups[0].rows[0].state,
            Some(State::Archived("stopped Δ"))
        );
        assert_eq!(a.groups[1].rows[0].state, Some(State::Idle(())));
        assert_eq!(a.groups[1].rows[1].state, None);
        assert_eq!(b.groups[0].rows[0].note, Some(""));
        assert_eq!(b.groups[0].rows[1].note, Some("low Δ"));
        assert_eq!(b.groups[1].rows[0].note, None);
        assert!(b.groups[1].rows[0].marker);
        assert!(!b.groups[1].rows[1].marker);
        assert_eq!(c.groups[0].rows[0].epoch, Some(16_504));
        let range = bytes.as_ptr_range();
        for value in [a.suffix, b.groups[0].rows[1].note.unwrap(), c.suffix] {
            assert!(range.contains(&value.as_ptr()));
        }
        assert_eq!(a.suffix, "tail ✅");
        assert_eq!(b.groups[1].sequence, 257);
        assert_eq!(c.groups[1].rows[0].words, [i16::MIN, -1, i16::MAX]);
        encode::<0>(4, fixture::LIMITS, &mut bytes).unwrap();
        let b = decode::<1>(&bytes, fixture::LIMITS).unwrap().into_view();
        assert!(
            b.groups
                .iter()
                .flat_map(|g| &g.rows)
                .all(|row| row.note.is_none() && !row.marker)
        );
        assert_eq!(
            decode::<2>(&bytes, fixture::LIMITS),
            Err(WireError::Incompatible)
        );
    }

    #[test]
    fn recovery_does_not_erase_hard_failures() {
        let mut bytes = Vec::new();
        encode::<2>(4, fixture::LIMITS, &mut bytes).unwrap();
        for end in 0..bytes.len() {
            assert!(decode::<0>(&bytes[..end], fixture::LIMITS).is_err());
        }
        let mut budget = fixture::LIMITS;
        budget.bytes = bytes.len();
        assert!(decode::<0>(&bytes, budget).is_ok());
        budget.bytes -= 1;
        assert_eq!(
            decode::<0>(&bytes, budget),
            Err(WireError::Limit(LimitKind::Bytes))
        );
        budget = fixture::LIMITS;
        budget.nodes = 1;
        assert_eq!(
            decode::<0>(&bytes, budget),
            Err(WireError::Limit(LimitKind::Nodes))
        );
        let position = bytes
            .windows(b"steady ".len())
            .position(|bytes| bytes == b"steady ")
            .unwrap();
        bytes[position] = 255;
        assert_eq!(
            decode::<0>(&bytes, fixture::LIMITS),
            Err(WireError::Malformed)
        );
        bytes[position] = b's';
        bytes.push(0);
        assert_eq!(
            decode::<0>(&bytes, fixture::LIMITS),
            Err(WireError::Trailing)
        );
    }

    #[test]
    fn generated_clients_exchange_exact_bytes_in_both_directions() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("wire-evolution-{}-{id}", std::process::id()));
        let result = (|| -> Result<(), Box<dyn Error>> {
            emit(&directory)?;
            if std::env::var_os("IC_WIRE_CHECK_TYPESCRIPT").is_some() {
                check_typescript(&directory)?;
            }
            let output = std::process::Command::new("node")
                .args(["--experimental-transform-types", "exchange.mjs"])
                .current_dir(&directory)
                .output()?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
            }
            for name in ["a", "b", "c"] {
                let rust = std::fs::read(directory.join(format!("{name}.bin")))?;
                let js = std::fs::read(directory.join(format!("{name}-js.bin")))?;
                assert_eq!(js, rust);
            }
            let c = std::fs::read(directory.join("c-js.bin"))?;
            assert_eq!(decode::<0>(&c, fixture::LIMITS)?.suffix.as_str(), "tail ✅");
            assert_eq!(
                decode::<2>(&c, fixture::LIMITS)?.row(0, 0).epoch,
                Some(16_504)
            );
            recovery_exchange::check(&directory)?;
            tuple_exchange::check(&directory)?;
            inline_exchange::check_clients(&directory)?;
            nested_inline_exchange::check_clients(&directory)?;
            Ok(())
        })();
        if result.is_ok() {
            std::fs::remove_dir_all(&directory).unwrap();
        }
        assert!(result.is_ok(), "{result:?} in {}", directory.display());
    }
}
