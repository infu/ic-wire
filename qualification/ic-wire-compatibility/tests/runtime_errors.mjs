import assert from 'node:assert/strict';
import { copyFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
const {fieldCodec} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));

const first = pathToFileURL(process.argv[1]);
const second = new URL('./second-runtime.ts', first);
await copyFile(first, second);
const local = await import(first);
const other = await import(second);
assert.notEqual(local.WireError, other.WireError);

for (const runtime of [local, other]) {
  const codec = fieldCodec(runtime.createPayloadCodec, { schema: { k: 'bool' }, bytes: 2, nodes: 1, depth: 1 });
  assert.throws(() => codec.decode(Uint8Array.of(2,0)), error =>
    error.name === 'WireError' && error.code === 'malformed');
}
console.log(true);
