//! Generated client exchange for nested inline records and tuple tails.
use std::{collections::BTreeMap, error::Error, path::Path};

use wire::{
    List, Wire,
    evolution::{__private::EvolvingSchema, Method},
    typescript::{PayloadLimits, SchemaRegistry},
};

use crate::fixture::LIMITS;

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct PointV1(u16, bool);

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct PointV2(u16, bool, #[wire(id = 3)] Option<()>);

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct JobV1 {
    id: u32,
    point: PointV1,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct JobV2 {
    id: u32,
    point: PointV2,
    #[wire(id = 3)]
    running: Option<bool>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct Request<Job> {
    jobs: List<Job, 2>,
    cursor: u32,
    #[wire(id = 1)]
    sample: Option<Job>,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct SummaryV1 {
    found: bool,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct SummaryV2 {
    found: bool,
    #[wire(id = 7, required)]
    count: u16,
}

#[derive(Debug, PartialEq, Eq, Wire)]
#[wire(crate = "wire")]
struct Reply<Summary> {
    result: Result<Option<Summary>, u16>,
    cursor: u32,
}

const V1: Method<Request<JobV1>, Reply<SummaryV1>> = Method::new("scan_jobs", LIMITS, LIMITS);
const V2: Method<Request<JobV2>, Reply<SummaryV2>> = Method::new("scan_jobs", LIMITS, LIMITS);

fn request_v1() -> Request<JobV1> {
    Request {
        jobs: List::new(vec![JobV1 {
            id: 7,
            point: PointV1(19, true),
        }])
        .unwrap(),
        cursor: 42,
        sample: None,
    }
}

fn request_v2() -> Request<JobV2> {
    Request {
        jobs: List::new(vec![JobV2 {
            id: 7,
            point: PointV2(19, true, Some(())),
            running: Some(false),
        }])
        .unwrap(),
        cursor: 42,
        sample: Some(JobV2 {
            id: 12,
            point: PointV2(31, false, None),
            running: Some(true),
        }),
    }
}

fn reply_v1() -> Reply<SummaryV1> {
    Reply {
        result: Ok(Some(SummaryV1 { found: true })),
        cursor: 42,
    }
}

fn reply_v2() -> Reply<SummaryV2> {
    Reply {
        result: Ok(Some(SummaryV2 {
            found: true,
            count: 11,
        })),
        cursor: 42,
    }
}

fn write_client<T: EvolvingSchema, R: EvolvingSchema>(
    method: &Method<T, R>,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    let limits = PayloadLimits {
        bytes: 128,
        nodes: 512,
        depth: 24,
    };
    let mut registry = SchemaRegistry::new(Default::default()).map_err(|err| format!("{err:?}"))?;
    let codec = registry
        .register_evolving_method(method, "scanJobs", ("Request", limits), ("Reply", limits))
        .map_err(|err| format!("{err:?}"))?;
    let graph = registry.finish().map_err(|err| format!("{err:?}"))?;
    let source = graph
        .method_module(&[codec], "./runtime.ts", &BTreeMap::new(), |report| {
            assert!(report.is_clean(), "{report:?}");
        })
        .map_err(|err| format!("{err:?}"))?;
    std::fs::write(destination, source)?;
    Ok(())
}

pub(super) fn emit(directory: &Path) -> Result<(), Box<dyn Error>> {
    write_client(&V1, &directory.join("nested-v1.ts"))?;
    write_client(&V2, &directory.join("nested-v2.ts"))?;

    let mut bytes = Vec::new();
    V1.encode_request(&request_v1(), &mut bytes)?;
    std::fs::write(directory.join("nested-v1-request.bin"), &bytes)?;
    V2.encode_request(&request_v2(), &mut bytes)?;
    std::fs::write(directory.join("nested-v2-request.bin"), &bytes)?;
    V1.encode_reply(&reply_v1(), &mut bytes)?;
    std::fs::write(directory.join("nested-v1-reply.bin"), &bytes)?;
    V2.encode_reply(&reply_v2(), &mut bytes)?;
    std::fs::write(directory.join("nested-v2-reply.bin"), &bytes)?;
    Ok(())
}

#[cfg(test)]
pub(super) fn check_clients(directory: &Path) -> Result<(), Box<dyn Error>> {
    let read = |name| std::fs::read(directory.join(format!("nested-{name}-js.bin")));

    let first = read("v1-request")?;
    assert_eq!(V1.decode_request(&first)?, request_v1());
    let latest = V2.decode_request(&first)?;
    assert_eq!(latest.cursor, 42);
    assert_eq!(latest.sample, None);
    assert_eq!(
        latest.jobs.as_slice(),
        &[JobV2 {
            id: 7,
            point: PointV2(19, true, None),
            running: None
        }]
    );

    let latest = read("v2-request")?;
    assert_eq!(V2.decode_request(&latest)?, request_v2());
    let first = V1.decode_request(&latest)?;
    assert_eq!(first.jobs, request_v1().jobs);
    assert_eq!(first.cursor, 42);
    assert_eq!(
        first.sample,
        Some(JobV1 {
            id: 12,
            point: PointV1(31, false)
        })
    );

    let first = read("v1-reply")?;
    assert_eq!(V1.decode_reply(&first)?, reply_v1());
    assert_eq!(V2.decode_reply(&first), Err(wire::Error::Incompatible));
    let latest = read("v2-reply")?;
    assert_eq!(V1.decode_reply(&latest)?, reply_v1());
    assert_eq!(V2.decode_reply(&latest)?, reply_v2());

    let malformed = read("invalid-known")?;
    assert_eq!(V2.decode_request(&malformed), Err(wire::Error::Malformed));
    assert_eq!(V1.decode_request(&malformed)?.cursor, 42);
    Ok(())
}
