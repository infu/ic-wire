import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const {fieldCodec} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
const { createPayloadCodec, WireError } = await import(pathToFileURL(process.argv[1]).href);
const { types, cases, invalidText } = JSON.parse(readFileSync(0, 'utf8'));
const codec = name => fieldCodec(createPayloadCodec, { schema: { k: 'ref', name }, bytes: 128, nodes: 16, depth: 8 }, types);
const fails = code => error => error instanceof WireError && error.code === code;
const output = [];
for (const item of cases) {
  const selected = codec(item.name);
  const value = item.name === 'Text' ? item.value : BigInt(item.value);
  assert.equal(selected.decode(new Uint8Array(item.bytes)), value);
  const bytes = selected.encode(value);
  assert.deepEqual(Array.from(bytes), item.bytes);
  output.push(Array.from(bytes));
}
for (const name of ['Unsigned', 'Signed']) {
  const selected = codec(name);
  const retained = new Uint8Array([128,0,0]);
  assert.equal(selected.decode(retained), 0n);
  assert.deepEqual(Array.from(selected.encode(0n)), [0,0]);
  assert.deepEqual(Array.from(retained), [128,0,0]);
  assert.throws(() => selected.encode(1), fails('value')); // BigInt is required.
  assert.throws(() => selected.decode(new Uint8Array([128])), fails('malformed'));
}
assert.throws(() => codec('Unsigned').encode(1n << 64n), fails('limit'));
assert.throws(() => codec('Unsigned').encode(-1n), fails('limit'));
assert.throws(() => codec('Signed').encode(1n << 127n), fails('limit'));
assert.throws(() => codec('Signed').encode(-(1n << 127n) - 1n), fails('limit'));
for (const bytes of invalidText) assert.throws(() => codec('Text').decode(new Uint8Array(bytes)), fails('malformed'));
assert.deepEqual(Array.from(codec('Text').encode('😀😀')), [8,240,159,152,128,240,159,152,128,0]);
assert.throws(() => codec('Text').encode('😀😀a'), fails('limit'));
process.stdout.write(JSON.stringify(output));
