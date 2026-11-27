import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const {fieldCodec, slotPayload, slotMethod} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin = ''; for await (const chunk of process.stdin) stdin += chunk;
const fixture = JSON.parse(stdin), url = pathToFileURL(process.argv[1]);
const { WireError, createPayloadCodec } = await import(url);
const { blob: rawBlob } = await import(new URL('./client.ts', url));
const { blobFrame: rawFrame } = await import(new URL('./frame-client.ts', url));
const blob = slotPayload(rawBlob), blobFrame = slotMethod(rawFrame);
const refusal = (action, code) => assert.throws(action, error => error instanceof WireError && error.code === code);
const prefixes = [[0], [63], [64], [127], [128, 1], [255, 127], [128, 128, 1], [128, 128, 64]];
const outputs = [];
for (const [index, size] of fixture.sizes.entries()) {
  // Select a nonzero-offset span with unrelated bytes on each side.
  const backing = new Uint8Array(size + 6).fill(255), value = backing.subarray(3, size + 3);
  for (let i = 0; i < size; i++) value[i] = i % 251;
  const expected = new Uint8Array(prefixes[index].length + size + 1);
  expected.set(prefixes[index]); expected.set(value, prefixes[index].length);
  const encoded = blob.encode(value);
  assert.deepEqual(encoded, expected);
  const exact = fieldCodec(createPayloadCodec, {schema:{k:'bytes',max:1048576},bytes:expected.length,nodes:1,depth:1});
  assert.deepEqual(exact.encode(value), expected);
  assert.deepEqual(exact.decode(expected), value);
  const short = fieldCodec(createPayloadCodec, {schema:{k:'bytes',max:1048576},bytes:expected.length-1,nodes:1,depth:1});
  refusal(() => short.encode(value), 'limit'); refusal(() => short.decode(expected), 'limit');
  for (const [direction, encode, decode] of [[0, blobFrame.encodeRequest, blobFrame.decodeRequest], [1, blobFrame.encodeResponse, blobFrame.decodeResponse]]) {
    const frame = new Uint8Array(11 + expected.length);
    frame.set([73,87,7,0,0,0,0,0,0,0,direction]); frame.set(expected,11);
    assert.deepEqual(encode(value), frame);
    const storage = new Uint8Array(frame.length + 5).fill(255); storage.set(frame, 2);
    const selected = storage.subarray(2, frame.length + 2), decoded = decode(selected);
    assert.deepEqual(decoded, value);
    storage.fill(0); assert.deepEqual(decoded, value);
    const identity = frame.slice(); identity[2] ^= 1;
    refusal(() => decode(identity), 'identity');
    const wrongDirection = frame.slice(); wrongDirection[10] ^= 1;
    refusal(() => decode(wrongDirection), 'identity');
  }
  if (size > 0) {
    // The field bound admits the length; the missing body is malformed.
    refusal(() => blob.decode(expected.subarray(0, prefixes[index].length)), 'malformed');
    const decoded = blob.decode(encoded); encoded.fill(0);
    assert.deepEqual(decoded, value);
  }
  const trailing = new Uint8Array(expected.length + 1); trailing.set(expected);
  refusal(() => blob.decode(trailing), size === 1048576 ? 'limit' : 'malformed');
  value.fill(0); assert.deepEqual(blob.decode(expected), Uint8Array.from({length:size}, (_, i) => i % 251));
  outputs.push(Array.from(expected));
}
refusal(() => blob.encode(new Uint8Array(1048577)), 'limit');
assert.deepEqual(blob.decode(Uint8Array.of(128, 0, 0)), new Uint8Array());
process.stdout.write(JSON.stringify(outputs));
