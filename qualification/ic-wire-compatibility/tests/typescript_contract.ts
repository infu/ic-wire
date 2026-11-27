// Compile-only consumer contracts. Each expect-error must produce a diagnostic.
import { recordCodec, type Record } from './client.ts';
import { highId, roundtrip, type Chain, type Shared } from './frame-client.ts';
const record: Record = {
  number: 1, choice: { tag: 'Ready', value: null }, optional: { some: 2 },
  bytes: new Uint8Array(), packed: new Int16Array(), ordinary: [1, -2],
};
const payload: Uint8Array = recordCodec.encode([record]);
const decoded: Record = recordCodec.decode(payload)[0];
roundtrip.encodeRequest([decoded]);
const chain: Chain = { value: 1, next: { some: { value: 2, next: null } } };
const shared: Shared = { left: chain, right: chain };
highId.encodeRequest([chain]);
highId.encodeResponse([shared]);
const request: Chain = highId.decodeRequest(payload)[0];
const response: Shared = highId.decodeResponse(payload)[0];
// @ts-expect-error Small integers use Number, not BigInt.
recordCodec.encode([{ ...record, number: 1n }]);
// @ts-expect-error Packed i16 fields require Int16Array.
recordCodec.encode([{ ...record, packed: [1, 2] }]);
// @ts-expect-error Options use explicit some wrappers.
recordCodec.encode([{ ...record, optional: 2 }]);
// @ts-expect-error Enum payload follows its tag.
recordCodec.encode([{ ...record, choice: { tag: 'Ready', value: 3 } }]);
// @ts-expect-error Request and response schemas are distinct.
highId.encodeRequest([response]);
// @ts-expect-error Response encoding requires both branches.
highId.encodeResponse([request]);
// @ts-expect-error Decode returns the declared record, not any.
const wrong: string = recordCodec.decode(payload)[0];
