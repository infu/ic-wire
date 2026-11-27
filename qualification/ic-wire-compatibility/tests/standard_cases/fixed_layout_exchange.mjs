import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const { codec } = await import(new URL('./client.ts', pathToFileURL(process.argv[1])));
const { rustBytes } = JSON.parse(readFileSync(0, 'utf8'));
const record = (fields) => Object.assign(Object.create(null), fields);
const fixed = record({ pair: [7, 128] });
const values = [
  record({ fixed, choice: { tag: 'Pair', value: [9, 129] } }),
  record({ fixed, choice: { tag: 'Named', value: record({ value: 255 }) } }),
];
const output = values.map((value, index) => {
  assert.deepEqual(codec.decode(Uint8Array.from(rustBytes[index])), value);
  const bytes = Array.from(codec.encode(value));
  assert.deepEqual(bytes, rustBytes[index]);
  return bytes;
});
process.stdout.write(JSON.stringify(output));
