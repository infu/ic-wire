import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
const {messageSchema} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
const runtime = await import(pathToFileURL(process.argv[1]));
let checked = 0;
function scalarExtension(schema) {
  return runtime.createPayloadCodec({
    schema: { k: 'extension-record', fields: [], extensions: [{ id: 1, name: 'value', value: schema, required: true }] },
    bytes: 32, nodes: 8, depth: 4,
  });
}
function check(codec, input, expected) {
  input = Uint8Array.of(input.length + 2, 1, input.length, ...input);
  if (expected === null) assert.throws(() => codec.decode(input), error => error instanceof runtime.WireError && error.code === 'malformed');
  else assert.deepEqual(codec.decode(input).value, expected);
  checked++;
}
for (const k of ['u', 'i']) {
  const codec = scalarExtension({ k, bits: 16 });
  const value = n => k === 'u' ? n : (n >>> 1) ^ -(n & 1);
  check(codec, new Uint8Array(), null);
  for (let first = 0; first < 256; first++) {
    check(codec, Uint8Array.of(first), first < 128 ? value(first) : null);
    for (let second = 0; second < 256; second++) {
      check(codec, Uint8Array.of(first, second), first >= 128 && second < 128 ? value((first & 127) | second << 7) : null);
    }
  }
}
const boolean = scalarExtension({k:'bool'});
for (let byte = 0; byte < 256; byte++) check(boolean, Uint8Array.of(byte), byte < 2 ? byte === 1 : null);
const fixed = {k:'bool'};
const evolved = messageSchema(fixed);
const policy = {bytes: 8, nodes: 8, depth: 4};
const aliases = {
  Fixed: fixed,
  Evolved: {k:'newtype',item:evolved},
};
const fixedRoot = {k:'ref',name:'Fixed'};
const evolvingRoot = {k:'ref',name:'Evolved'};
const refusal = error => error instanceof runtime.WireError && error.code === 'value';
for (const schema of [fixed, fixedRoot]) {
  assert.throws(() => runtime.createPayloadCodec({schema, ...policy}, aliases), refusal);
}
const frame = {id:'7', requestBytes:16, responseBytes:16, requestNodes:8, responseNodes:8, requestDepth:4, responseDepth:4};
for (const [request,response] of [[fixedRoot,evolvingRoot],[evolvingRoot,fixedRoot]]) {
  assert.throws(() => runtime.createCodec({...frame,request,response}, aliases), refusal);
}
const message = runtime.createPayloadCodec({schema:evolvingRoot,...policy},aliases);
assert.deepEqual(message.encode([true]), Uint8Array.of(1,0));
assert.throws(() => message.decode(Uint8Array.of(1)), error => error instanceof runtime.WireError && error.code === 'malformed');

const emptyRoot = {k:'extension-tuple',items:[],extensions:[]};
const emptyPayload = {...policy,schema:emptyRoot,bytes:1};
assert.throws(() => runtime.createPayloadCodec({...emptyPayload,bytes:0}), refusal);
const payloadAtMinimum = runtime.createPayloadCodec(emptyPayload);
assert.deepEqual(payloadAtMinimum.encode([]), Uint8Array.of(0));
assert.deepEqual(payloadAtMinimum.decode(Uint8Array.of(0)), []);

const emptyFrame = {...frame,request:emptyRoot,response:emptyRoot,requestBytes:12,responseBytes:12};
for (const limit of ['requestBytes','responseBytes']) {
  assert.throws(() => runtime.createCodec({...emptyFrame,[limit]:11}), refusal);
}
const frameAtMinimum = runtime.createCodec(emptyFrame);
const requestAtMinimum = Uint8Array.of(73,87,7,0,0,0,0,0,0,0,0,0);
const responseAtMinimum = Uint8Array.of(73,87,7,0,0,0,0,0,0,0,1,0);
assert.deepEqual(frameAtMinimum.encodeRequest([]), requestAtMinimum);
assert.deepEqual(frameAtMinimum.encodeResponse([]), responseAtMinimum);
assert.deepEqual(frameAtMinimum.decodeRequest(requestAtMinimum), []);
assert.deepEqual(frameAtMinimum.decodeResponse(responseAtMinimum), []);
process.stdout.write(JSON.stringify({ checked }));
