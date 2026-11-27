//! Framed exchange for three revisions of the same Rust declarations.
use std::{collections::BTreeMap, error::Error, path::Path};

use serde::{Deserialize, Serialize};
use wire::{
    List, TextRef, Wire,
    evolution::{__private::EvolvingSchema, Method},
    typescript::{PayloadLimits, SchemaRegistry},
};

use crate::fixture::LIMITS;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Wire)]
#[wire(crate = "wire", fixed)]
struct Filter {
    active: bool,
    code: u8,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct RequestV1 {
    post_id: u32,
    filters: List<Filter, 2>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct RequestV2 {
    post_id: u32,
    filters: List<Filter, 2>,
    #[wire(id = 7)]
    include_count: Option<bool>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct RequestV3<'a> {
    post_id: u32,
    filters: List<Filter, 2>,
    #[wire(id = 7)]
    include_count: Option<bool>,
    #[wire(id = 3)]
    audience: Option<TextRef<'a, 16>>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct ReplyV1(u32, bool);

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct ReplyV2(u32, bool, #[wire(id = 3)] Option<bool>);

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct ReplyV3(
    u32,
    bool,
    #[wire(id = 3)] Option<bool>,
    #[wire(id = 4)] Option<()>,
);

const METHOD_V1: Method<RequestV1, ReplyV1> = Method::new("get-post", LIMITS, LIMITS);
const METHOD_V2: Method<RequestV2, ReplyV2> = Method::new("get-post", LIMITS, LIMITS);
fn method_v3<'a>() -> Method<RequestV3<'a>, ReplyV3> {
    Method::new("get-post", LIMITS, LIMITS)
}

fn filters() -> List<Filter, 2> {
    List::new(vec![
        Filter {
            active: true,
            code: 4,
        },
        Filter {
            active: false,
            code: 9,
        },
    ])
    .unwrap()
}

fn frames(directory: &Path) -> Result<(), Box<dyn Error>> {
    let mut output = Vec::new();

    METHOD_V1.encode_request(
        &RequestV1 {
            post_id: 42,
            filters: filters(),
        },
        &mut output,
    )?;
    std::fs::write(directory.join("inline-v1-request.bin"), &output)?;
    METHOD_V1.encode_reply(&ReplyV1(42, true), &mut output)?;
    std::fs::write(directory.join("inline-v1-reply.bin"), &output)?;

    METHOD_V2.encode_request(
        &RequestV2 {
            post_id: 42,
            filters: filters(),
            include_count: Some(false),
        },
        &mut output,
    )?;
    std::fs::write(directory.join("inline-v2-request.bin"), &output)?;
    METHOD_V2.encode_reply(&ReplyV2(42, true, Some(false)), &mut output)?;
    std::fs::write(directory.join("inline-v2-reply.bin"), &output)?;

    let method = method_v3();
    method.encode_request(
        &RequestV3 {
            post_id: 42,
            filters: filters(),
            include_count: Some(false),
            audience: Some(TextRef::new("team Δ")?),
        },
        &mut output,
    )?;
    std::fs::write(directory.join("inline-v3-request.bin"), &output)?;
    method.encode_reply(&ReplyV3(42, true, Some(false), Some(())), &mut output)?;
    std::fs::write(directory.join("inline-v3-reply.bin"), &output)?;
    Ok(())
}

fn emit_codec<Request: EvolvingSchema, Reply: EvolvingSchema>(
    method: &Method<Request, Reply>,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    const BROWSER_LIMITS: PayloadLimits = PayloadLimits {
        bytes: 128,
        nodes: 256,
        depth: 16,
    };

    let mut builder = SchemaRegistry::new(Default::default()).map_err(|err| format!("{err:?}"))?;
    let codec = builder
        .register_evolving_method(
            method,
            "getPost",
            ("Request", BROWSER_LIMITS),
            ("Reply", BROWSER_LIMITS),
        )
        .map_err(|err| format!("{err:?}"))?;
    let graph = builder.finish().map_err(|err| format!("{err:?}"))?;
    let source = graph
        .method_module(&[codec], "./runtime.ts", &BTreeMap::new(), |report| {
            assert!(report.is_clean(), "{report:?}")
        })
        .map_err(|err| format!("{err:?}"))?;
    std::fs::write(destination, source)?;
    Ok(())
}

pub(super) fn emit(directory: &Path) -> Result<(), Box<dyn Error>> {
    emit_codec(&METHOD_V1, &directory.join("inline-v1.ts"))?;
    emit_codec(&METHOD_V2, &directory.join("inline-v2.ts"))?;
    emit_codec(&method_v3(), &directory.join("inline-v3.ts"))?;
    frames(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wire::Error;

    #[test]
    fn empty_block_is_written_in_version_one_and_new_members_default_to_absence() {
        let mut output = Vec::new();
        METHOD_V1
            .encode_request(
                &RequestV1 {
                    post_id: 42,
                    filters: filters(),
                },
                &mut output,
            )
            .unwrap();
        assert_eq!(&output[11..], &[42, 2, 1, 4, 0, 9, 0]);
        assert_eq!(
            METHOD_V2.decode_request(&output).unwrap().include_count,
            None
        );
        assert_eq!(
            method_v3().decode_request(&output).unwrap(),
            RequestV3 {
                post_id: 42,
                filters: filters(),
                include_count: None,
                audience: None,
            }
        );

        METHOD_V1
            .encode_reply(&ReplyV1(42, true), &mut output)
            .unwrap();
        assert_eq!(&output[11..], &[42, 1, 0]);
        assert_eq!(
            METHOD_V2.decode_reply(&output).unwrap(),
            ReplyV2(42, true, None)
        );
        assert_eq!(
            method_v3().decode_reply(&output).unwrap(),
            ReplyV3(42, true, None, None)
        );
        assert!(METHOD_V1.decode_request(&output).is_err());
    }

    #[test]
    fn old_readers_skip_new_bodies_but_validate_the_extension_block() {
        let method = method_v3();
        let mut output = Vec::new();
        method
            .encode_reply(&ReplyV3(42, true, Some(false), Some(())), &mut output)
            .unwrap();
        assert_eq!(&output[11..], &[42, 1, 5, 3, 1, 0, 4, 0]);
        assert_eq!(METHOD_V1.decode_reply(&output).unwrap(), ReplyV1(42, true));
        assert_eq!(
            METHOD_V2.decode_reply(&output).unwrap(),
            ReplyV2(42, true, Some(false))
        );
        for end in 0..output.len() {
            assert!(METHOD_V1.decode_reply(&output[..end]).is_err());
            assert!(method.decode_reply(&output[..end]).is_err());
        }
        output[16] = 2;
        assert_eq!(METHOD_V1.decode_reply(&output).unwrap(), ReplyV1(42, true));
        assert_eq!(METHOD_V2.decode_reply(&output), Err(Error::Malformed));
        output[17] = 3;
        assert_eq!(METHOD_V1.decode_reply(&output), Err(Error::Malformed));
        method
            .encode_reply(&ReplyV3(42, true, Some(false), None), &mut output)
            .unwrap();
        assert_eq!(&output[11..], &[42, 1, 3, 3, 1, 0]);
    }

    pub(crate) fn check_clients(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
        for rev in ["v1", "v2", "v3"] {
            let request = std::fs::read(directory.join(format!("inline-{rev}-request-js.bin")))?;
            let reply = std::fs::read(directory.join(format!("inline-{rev}-reply-js.bin")))?;
            assert_eq!(
                METHOD_V1.decode_request(&request)?,
                RequestV1 {
                    post_id: 42,
                    filters: filters()
                }
            );
            assert_eq!(METHOD_V1.decode_reply(&reply)?, ReplyV1(42, true));
            let include_count = (rev != "v1").then_some(false);
            assert_eq!(
                METHOD_V2.decode_request(&request)?.include_count,
                include_count
            );
            assert_eq!(
                METHOD_V2.decode_reply(&reply)?,
                ReplyV2(42, true, include_count)
            );
            let latest = method_v3().decode_request(&request)?;
            let audience = latest.audience.map(|text| text.as_str());
            assert_eq!(latest.include_count, include_count);
            assert_eq!(audience, (rev == "v3").then_some("team Δ"));
            if let Some(text) = audience {
                assert!(request.as_ptr_range().contains(&text.as_ptr()));
            }
            assert_eq!(
                method_v3().decode_reply(&reply)?,
                ReplyV3(42, true, include_count, (rev == "v3").then_some(()))
            );
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) use tests::check_clients;
