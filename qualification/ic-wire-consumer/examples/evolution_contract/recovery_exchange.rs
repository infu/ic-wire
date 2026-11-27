//! Generated-client exchange for recovery above a required variant.
use std::{collections::BTreeMap, error::Error, path::Path};

use serde::{Deserialize, Serialize};
use wire::{
    Encoding, Error as WireError, List, TextRef,
    evolution::{OpaqueOpen, Projected, Recoverable, WriteContext, encode_projected_into},
    typescript::{GenerationError, PayloadLimits, SchemaContext, SchemaRegistry, Wire},
};

use wire::typescript::__private::{Value, json};

use crate::fixture::LIMITS;

#[derive(Serialize, Deserialize)]
struct Packet<'a, const FUTURE: bool> {
    #[serde(borrow)]
    scope: Recoverable<'a>,
    #[serde(borrow)]
    tail: TextRef<'a, 16>,
}

#[derive(Serialize, Deserialize)]
struct Body<'a> {
    #[serde(borrow)]
    items: List<OpaqueOpen<'a>, 4>,
    #[serde(borrow)]
    nested: Recoverable<'a>,
    #[serde(borrow)]
    witness: TextRef<'a, 16>,
}

fn state_schema<const FUTURE: bool>(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
    let mut cases = vec![json!({"id":1,"name":"Idle","value":ctx.describe::<()>()?})];
    if FUTURE {
        cases.push(json!({"id":3,"name":"Active","value":ctx.describe::<u8>()?}));
    }
    Ok(json!({"k":"open-variant","required":true,"variants":cases}))
}

impl<const FUTURE: bool> Wire for Packet<'_, FUTURE> {
    fn schema(ctx: &mut SchemaContext) -> Result<Value, GenerationError> {
        let state = state_schema::<FUTURE>(ctx)?;
        let text = ctx.describe::<TextRef<'_, 16>>()?;
        Ok(json!({"k":"extension-record","fields":[
            {"name":"scope","value":{"k":"recover","item":{"k":"record","fields":[
                {"name":"items","value":{"k":"list","max":4,"item":state}},
                {"name":"nested","value":{"k":"recover","item":state}},
                {"name":"witness","value":text}
            ]}}},
            {"name":"tail","value":text}
        ],"extensions":[]}))
    }
}

#[derive(Clone, Copy)]
enum Witness {
    Known,
    FutureInner,
    FutureOuter,
    Absent,
}

impl Witness {
    const CASES: [(Self, &'static str); 4] = [
        (Self::Known, "known"),
        (Self::FutureInner, "future-inner"),
        (Self::FutureOuter, "future-outer"),
        (Self::Absent, "absent"),
    ];
}

fn selected<'a>(
    ctx: &WriteContext<'_>,
    future: bool,
    scratch: &'a mut [u8],
) -> Result<OpaqueOpen<'a>, WireError> {
    if future {
        OpaqueOpen::write(ctx, 3, &42_u8, scratch)
    } else {
        OpaqueOpen::unit(1)
    }
}

fn encode(case: Witness) -> Result<Vec<u8>, WireError> {
    let mut output = Vec::new();
    let mut scope_bytes = [0; 40];
    let mut item = [0; 1];
    let mut state_bytes = [0; 1];
    let mut nested_bytes = [0; 3];
    encode_projected_into(&mut output, Encoding::Payload, LIMITS, |ctx| {
        let scope = ctx.child().nested(|scope| {
            if matches!(case, Witness::Absent) {
                return Ok(Recoverable::absent());
            }
            let body = scope.nested(|body| {
                let items = body.nested(|items| {
                    let state = items.nested(|element| {
                        selected(element, matches!(case, Witness::FutureOuter), &mut item)
                    })?;
                    List::new(vec![state, OpaqueOpen::unit(1)?])
                })?;
                let nested = body.nested(|nested| {
                    let state = nested.nested(|state| {
                        selected(
                            state,
                            matches!(case, Witness::FutureInner),
                            &mut state_bytes,
                        )
                    })?;
                    Recoverable::write(nested, &state, &mut nested_bytes)
                })?;
                Ok::<_, WireError>(Body {
                    items,
                    nested,
                    witness: TextRef::new("after Δ")?,
                })
            })?;
            Recoverable::write(scope, &body, &mut scope_bytes)
        })?;
        Ok(Projected::empty(Packet::<true> {
            scope,
            tail: TextRef::new("outside")?,
        }))
    })?;
    Ok(output)
}

fn emit_codec<const FUTURE: bool>(dir: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let mut registry = SchemaRegistry::new(Default::default()).map_err(|e| format!("{e:?}"))?;
    registry
        .register::<Packet<'_, FUTURE>>("RecoveryPacket")
        .map_err(|e| format!("{e:?}"))?;
    let graph = registry.finish().map_err(|e| format!("{e:?}"))?;
    let source = graph
        .payload_module(
            "packet",
            "RecoveryPacket",
            "./runtime.ts",
            PayloadLimits {
                bytes: 128,
                nodes: 256,
                depth: 20,
            },
            &BTreeMap::new(),
            |report| assert!(report.is_clean(), "{report:?}"),
        )
        .map_err(|e| format!("{e:?}"))?;
    std::fs::write(dir.join(name), source)?;
    Ok(())
}

pub(super) fn emit(dir: &Path) -> Result<(), Box<dyn Error>> {
    emit_codec::<false>(dir, "recover-a.ts")?;
    emit_codec::<true>(dir, "recover-b.ts")?;
    for (case, name) in Witness::CASES {
        std::fs::write(dir.join(format!("recover-{name}.bin")), encode(case)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wire::evolution::{RecoveryContext, decode_projected};

    fn read_state<const FUTURE: bool>(
        state: OpaqueOpen<'_>,
        ctx: &RecoveryContext<'_>,
    ) -> Result<Option<u8>, WireError> {
        ctx.required_open(state, |id, field| match id {
            1 => field.decode::<()>().map(|()| Some(0)),
            3 if FUTURE => field.decode::<u8>().map(Some),
            _ => Ok(None),
        })
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Projection<'a> {
        Absent,
        Retained {
            first: Option<u8>,
            nested: Option<u8>,
            witness: &'a str,
        },
    }

    fn decode<const FUTURE: bool>(bytes: &[u8]) -> Result<(Projection<'_>, &str), WireError> {
        decode_projected(
            bytes,
            Encoding::Payload,
            LIMITS,
            |root: Projected<'_, Packet<'_, FUTURE>>, ctx| {
                root.extensions.visit(ctx, |_, _| Ok(()))?;
                let packet = root.core;
                let projection = ctx.child().nested(|scope| {
                    packet.scope.project(scope, |body: Body<'_>, scope| {
                        let first = scope.nested(|scope| {
                            let mut first = None;
                            for (index, item) in body.items.into_vec().into_iter().enumerate() {
                                let actual =
                                    scope.nested(|scope| read_state::<FUTURE>(item, scope))?;
                                if index == 0 {
                                    first = actual;
                                }
                            }
                            Ok(first)
                        })?;
                        let nested = scope
                            .nested(|scope| {
                                body.nested
                                    .project(scope.read_context(), read_state::<FUTURE>)
                            })?
                            .flatten();
                        Ok(Projection::Retained {
                            first,
                            nested,
                            witness: body.witness.as_str(),
                        })
                    })
                })?;
                Ok((
                    projection.unwrap_or(Projection::Absent),
                    packet.tail.as_str(),
                ))
            },
        )
    }

    pub(in crate::recovery_exchange) fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
        for (case, name) in Witness::CASES {
            let bytes = std::fs::read(dir.join(format!("recover-{name}-js.bin")))?;
            let expected = match case {
                Witness::Known => Projection::Retained {
                    first: Some(0),
                    nested: Some(0),
                    witness: "after Δ",
                },
                Witness::FutureInner => Projection::Retained {
                    first: Some(0),
                    nested: None,
                    witness: "after Δ",
                },
                Witness::FutureOuter | Witness::Absent => Projection::Absent,
            };
            assert_eq!(decode::<false>(&bytes)?, (expected, "outside"));
            let expected = match case {
                Witness::Known => Projection::Retained {
                    first: Some(0),
                    nested: Some(0),
                    witness: "after Δ",
                },
                Witness::FutureInner => Projection::Retained {
                    first: Some(0),
                    nested: Some(42),
                    witness: "after Δ",
                },
                Witness::FutureOuter => Projection::Retained {
                    first: Some(42),
                    nested: Some(0),
                    witness: "after Δ",
                },
                Witness::Absent => Projection::Absent,
            };
            assert_eq!(decode::<true>(&bytes)?, (expected, "outside"));
        }
        let bad = std::fs::read(dir.join("recover-malformed-js.bin"))?;
        assert_eq!(decode::<false>(&bad), Err(WireError::Malformed));
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn check(dir: &Path) -> Result<(), Box<dyn Error>> {
    tests::check(dir)
}
