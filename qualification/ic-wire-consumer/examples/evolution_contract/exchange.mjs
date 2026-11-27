import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { WireError } from './runtime.ts';
import { packet as a } from './a.ts';
import { packet as b } from './b.ts';
import { packet as c } from './c.ts';
import { packet as recoverA } from './recover-a.ts';
import { packet as recoverB } from './recover-b.ts';
import { packet as tupleA } from './tuple-a.ts';
import { packet as tupleB } from './tuple-b.ts';
import { getPost as inlineV1 } from './inline-v1.ts';
import { getPost as inlineV2 } from './inline-v2.ts';
import { getPost as inlineV3 } from './inline-v3.ts';
import { scanJobs as nestedV1 } from './nested-v1.ts';
import { scanJobs as nestedV2 } from './nested-v2.ts';

const codecs = [a, b, c];
const some = value => ({ some: value });
const expected = (revision, writer = revision) => {
  const rows = Array.from({ length: 4 }, (_, index) => {
    const uid = 120 + index;
    const message = { message: 'steady Δ' };
    if (revision === 2) message.audit = some(Uint8Array.of(0, 255, 9, 1));
    const state = [
      revision === 2 ? { tag: 'Archived', value: 'stopped Δ' } : null,
      { tag: 'Detail', value: message },
      { tag: 'Idle', value: null },
      null,
    ][index];
    const row = { id: uid, state, words: Int16Array.of(-32768, -1, 32767) };
    if (revision > 0) {
      row.note = writer > 0 && index < 2 ? some(index === 0 ? '' : 'low Δ') : null;
      row.marker = writer > 0 && index === 2 ? some(null) : null;
    }
    if (revision === 2) row.epoch = 16384 + uid;
    return row;
  });
  return {
    groups: [
      { channel: 17, rows: rows.slice(0, 2), sequence: 1n << 60n },
      { channel: 33, rows: rows.slice(2), sequence: 257n },
    ],
    suffix: 'tail ✅',
  };
};

// Decoded records have null prototypes; compare their own fields as data.
function comparable(value) {
  if (ArrayBuffer.isView(value)) return [...value];
  if (Array.isArray(value)) return value.map(comparable);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, comparable(v)]));
  }
  return value;
}

const fixtures = ['a', 'b', 'c'].map((name, index) => {
  const rust = readFileSync(`${name}.bin`);
  const generated = codecs[index].encodeRequest(expected(index));
  assert.deepEqual(Buffer.from(generated), rust, `${name} shared encoding`);
  writeFileSync(`${name}-js.bin`, generated);
  return rust;
});

for (const [writer, bytes] of fixtures.entries()) {
  const response = Buffer.from(bytes);
  response[10] = 1;
  assert.deepEqual(Buffer.from(codecs[writer].encodeResponse(expected(writer))), response);
  for (const [reader, codec] of codecs.entries()) {
    if (reader === 2 && writer !== 2) {
      const incompatible = error => error instanceof WireError && error.code === 'incompatible';
      assert.throws(() => codec.decodeRequest(bytes), incompatible);
      assert.throws(() => codec.decodeResponse(response), incompatible);
    } else {
      const view = comparable(expected(reader, writer));
      assert.deepEqual(comparable(codec.decodeRequest(bytes)), view);
      assert.deepEqual(comparable(codec.decodeResponse(response)), view);
    }
  }
}

const decodedOld = a.decodeRequest(fixtures[2]);
assert.equal(decodedOld.groups[0].rows[0].state, null);
assert.equal(decodedOld.groups[0].rows[1].state.value.message, 'steady Δ');
assert.equal(decodedOld.suffix, 'tail ✅');
assert.throws(() => a.decodeRequest(fixtures[2].subarray(0, -1)), WireError);
const wrong = Buffer.from(fixtures[2]);
const index = wrong.indexOf('steady ');
assert.ok(index > 0);
wrong[index] = 255;
assert.throws(() => a.decodeRequest(wrong), error => error instanceof WireError && error.code === 'malformed');

const idle = { tag: 'Idle', value: null };
const active = { tag: 'Active', value: 42 };
const scoped = (first = idle, nested = some(idle)) => ({
  scope: some({ items: [first, idle], nested, witness: 'after Δ' }),
  tail: 'outside',
});
const unscoped = { scope: null, tail: 'outside' };
for (const [name, old, latest] of [
  ['known', scoped(), scoped()],
  ['future-inner', scoped(idle, null), scoped(idle, some(active))],
  ['future-outer', unscoped, scoped(active)],
  ['absent', unscoped, unscoped],
]) {
  const rust = readFileSync(`recover-${name}.bin`);
  assert.deepEqual(comparable(recoverA.decode(rust)), old);
  assert.deepEqual(comparable(recoverB.decode(rust)), latest);
  const browser = recoverB.encode(latest);
  assert.deepEqual(Buffer.from(browser), rust);
  writeFileSync(`recover-${name}-js.bin`, browser);
}
const malformedBranch = readFileSync('recover-future-outer.bin');
const witness = malformedBranch.indexOf('after ');
assert.ok(witness > 0);
malformedBranch[witness] = 255;
assert.throws(() => recoverA.decode(malformedBranch), error =>
  error instanceof WireError && error.code === 'malformed');
writeFileSync('recover-malformed-js.bin', malformedBranch);

const shortTuple = [[[777, 'first'], [9, 'second']], 1n << 60n];
const longTuple = [
  [[777, 'first', some('extra Δ'), null], [9, 'second', null, some(null)]],
  1n << 60n,
  some(false),
];
const absentTuple = [[[777, 'first', null, null], [9, 'second', null, null]], 1n << 60n, null];
for (const [kind, encode, decode] of [
  ['req', 'encodeRequest', 'decodeRequest'],
  ['res', 'encodeResponse', 'decodeResponse'],
]) {
  for (const [revision, codec, input, latest] of [
    ['a', tupleA, shortTuple, absentTuple],
    ['b', tupleB, longTuple, longTuple],
  ]) {
    const name = `tuple-${revision}-${kind}`;
    const rust = readFileSync(`${name}.bin`);
    assert.deepEqual(comparable(tupleA[decode](rust)), shortTuple);
    assert.deepEqual(comparable(tupleB[decode](rust)), latest);
    const bytes = codec[encode](input);
    assert.deepEqual(Buffer.from(bytes), rust);
    writeFileSync(`${name}-js.bin`, bytes);
  }
}
assert.throws(() => tupleA.encodeRequest(longTuple), WireError);
assert.throws(() => tupleB.encodeRequest(shortTuple), WireError);
const invalidSlot = readFileSync('tuple-b-res.bin');
const noteAt = invalidSlot.indexOf('extra ');
assert.ok(noteAt > 0);
invalidSlot[noteAt] = 255;
assert.deepEqual(comparable(tupleA.decodeResponse(invalidSlot)), shortTuple);
assert.throws(() => tupleB.decodeResponse(invalidSlot), error =>
  error instanceof WireError && error.code === 'malformed');
writeFileSync('tuple-invalid-slot-js.bin', invalidSlot);

const inline = [inlineV1, inlineV2, inlineV3];
const filters = [{ active: true, code: 4 }, { active: false, code: 9 }];
function inlineRequest(reader, writer) {
  return {
    post_id: 42, filters,
    ...(reader >= 2 && { include_count: writer >= 2 ? some(false) : null }),
    ...(reader >= 3 && { audience: writer >= 3 ? some('team Δ') : null }),
  };
}
function inlineReply(reader, writer) {
  const reply = [42, true];
  if (reader >= 2) reply.push(writer >= 2 ? some(false) : null);
  if (reader >= 3) reply.push(writer >= 3 ? some(null) : null);
  return reply;
}
for (const [kind, encode, decode, shape] of [
  ['request', 'encodeRequest', 'decodeRequest', inlineRequest],
  ['reply', 'encodeResponse', 'decodeResponse', inlineReply],
]) {
  for (const [index, codec] of inline.entries()) {
    const writer = index + 1;
    const name = `inline-v${writer}-${kind}`;
    const rust = readFileSync(`${name}.bin`);
    const js = codec[encode](shape(writer, writer));
    assert.deepEqual(Buffer.from(js), rust);
    for (const [reader, other] of inline.entries()) {
      assert.deepEqual(comparable(other[decode](rust)), shape(reader + 1, writer));
    }
    writeFileSync(`${name}-js.bin`, js);
  }
}

const nestedRequestOld = { jobs: [{ id: 7, point: [19, true] }], cursor: 42, sample: null };
const nestedRequestWhenAbsent = {
  jobs: [{ id: 7, point: [19, true, null], running: null }], cursor: 42, sample: null,
};
const nestedRequestLatest = {
  jobs: [{ id: 7, point: [19, true, some(null)], running: some(false) }],
  cursor: 42,
  sample: some({ id: 12, point: [31, false, null], running: some(true) }),
};
const nestedReplyOld = { result: { ok: some({ found: true }) }, cursor: 42 };
const nestedReplyLatest = { result: { ok: some({ found: true, count: 11 }) }, cursor: 42 };
for (const [revision, codec, request, reply] of [
  ['v1', nestedV1, nestedRequestOld, nestedReplyOld],
  ['v2', nestedV2, nestedRequestLatest, nestedReplyLatest],
]) {
  for (const [kind, encode, input] of [
    ['request', 'encodeRequest', request],
    ['reply', 'encodeResponse', reply],
  ]) {
    const name = `nested-${revision}-${kind}`;
    const rust = readFileSync(`${name}.bin`);
    const js = codec[encode](input);
    assert.deepEqual(Buffer.from(js), rust);
    writeFileSync(`${name}-js.bin`, js);
  }
}
const nestedOldReq = readFileSync('nested-v1-request.bin');
const nestedNewReq = readFileSync('nested-v2-request.bin');
assert.deepEqual(comparable(nestedV1.decodeRequest(nestedOldReq)), nestedRequestOld);
assert.deepEqual(comparable(nestedV2.decodeRequest(nestedOldReq)), nestedRequestWhenAbsent);
assert.deepEqual(comparable(nestedV1.decodeRequest(nestedNewReq)), {
  ...nestedRequestOld, sample: some({ id: 12, point: [31, false] }),
});
assert.deepEqual(comparable(nestedV2.decodeRequest(nestedNewReq)), nestedRequestLatest);
const nestedOldReply = readFileSync('nested-v1-reply.bin');
const nestedNewReply = readFileSync('nested-v2-reply.bin');
assert.deepEqual(comparable(nestedV1.decodeResponse(nestedOldReply)), nestedReplyOld);
assert.deepEqual(comparable(nestedV1.decodeResponse(nestedNewReply)), nestedReplyOld);
assert.deepEqual(comparable(nestedV2.decodeResponse(nestedNewReply)), nestedReplyLatest);
assert.throws(() => nestedV2.decodeResponse(nestedOldReply), error =>
  error instanceof WireError && error.code === 'incompatible');

const invalidNested = Buffer.from(nestedNewReq);
const boolEntry = invalidNested.indexOf(Buffer.from([3, 3, 1, 0]), 11);
assert.ok(boolEntry >= 11);
invalidNested[boolEntry + 3] = 2;
assert.equal(nestedV1.decodeRequest(invalidNested).cursor, 42);
assert.throws(() => nestedV2.decodeRequest(invalidNested), error =>
  error instanceof WireError && error.code === 'malformed');
writeFileSync('nested-invalid-known-js.bin', invalidNested);
