import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const base = pathToFileURL(process.argv[1]);
const {createService} = await import(new URL('./service.ts', base));
const {WireError} = await import(base);
const canister = {id: 'test'};
const calls = [];
const header = (id, direction) => [73, 87, id, 0, 0, 0, 0, 0, 0, 0, direction];
function receive(mode, target, name, bytes) {
  assert.equal(target, canister);
  const id = ['read-record', 'write"record', 'nested.read'].indexOf(name) + 1;
  assert.deepEqual([...bytes], [...header(id, 0), ...(id === 2 ? [17] : [1, 97]), 0]);
  calls.push([mode, name]);
  return Uint8Array.from([...header(id, 1), ...(id === 2 ? [1, 1] : [0, 1]), 0]);
}
const client = createService(canister, {
  query: async (...args) => receive('query', ...args),
  update: async (...args) => receive('update', ...args),
});
assert(Object.isFrozen(client));
assert.deepEqual(Object.keys(client), ['fetch', 'save', 'lookup']);
assert.deepEqual(await client.fetch(['a']), [{ok: true}]);
assert.deepEqual(await client.save([17]), [{err: true}]);
assert.deepEqual(await client.lookup(['a']), [{ok: true}]);
const limit = error => error instanceof WireError && error.code === 'limit';
await assert.rejects(client.fetch(['abcd']), limit);
assert.deepEqual(calls, [['query','read-record'],['update','write"record'],['query','nested.read']]);
const oversized = createService(canister, {query: async () => new Uint8Array(16)});
await assert.rejects(oversized.fetch(['a']), limit);
console.log(true);
