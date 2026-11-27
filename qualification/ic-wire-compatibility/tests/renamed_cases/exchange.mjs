import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const {slotPayload} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
const { literals } = JSON.parse(readFileSync(0, 'utf8'));
const { codec: rawPayload } = await import(new URL('./client.ts', pathToFileURL(process.argv[1])).href);
const codec = slotPayload(rawPayload);
const { WireError } = await import(pathToFileURL(process.argv[1]).href);
const values = [
  Object.assign(Object.create(null), { ['__proto__']: false, 'choice-value': { tag: 'ready-now', value: null } }),
  Object.assign(Object.create(null), { ['__proto__']: true, 'choice-value': { tag: 'has-value', value: 128 } }),
];
const output = [];
for (const [index, value] of values.entries()) {
  const decoded = codec.decode(new Uint8Array(literals[index]));
  assert.deepEqual(decoded, value);
  assert.equal(Object.getPrototypeOf(decoded), null);
  assert(Object.hasOwn(decoded, '__proto__'));
  const bytes = Array.from(codec.encode(value));
  assert.deepEqual(bytes, literals[index]);
  output.push(bytes);
}
assert.throws(() => codec.encode({ flag: true, choice: { tag: 'Value', value: 128 } }), error => error instanceof WireError && error.code === 'value');
assert.throws(() => codec.encode({ ['__proto__']: true, 'choice-value': { tag: 'Value', value: 128 } }), error => error instanceof WireError && error.code === 'value');
process.stdout.write(JSON.stringify(output));
