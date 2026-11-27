import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {slotPayload} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin=''; for await(const chunk of process.stdin) stdin+=chunk;
const fixture=JSON.parse(stdin), url=pathToFileURL(process.argv[1]);
const {codec:rawPayload}=await import(new URL('./client.ts',url));
const codec = slotPayload(rawPayload);
const {WireError}=await import(url);
const output=[];
for(const subaccount of [null,{some:new Uint8Array(32)}]) {
  const value={owner:Uint8Array.of(4),subaccount};
  const bytes=codec.encode(value); assert.deepEqual(Array.from(bytes),fixture.literals[output.length]);
  const decoded=codec.decode(bytes);
  assert.deepEqual(decoded.owner,value.owner);
  assert.deepEqual(decoded.subaccount===null?null:{...decoded.subaccount},subaccount);
  bytes.fill(0); assert.deepEqual(decoded.owner,value.owner);
  output.push(fixture.literals[output.length]);
}
for(const length of [0,1,29]) assert.equal(codec.decode(codec.encode({owner:new Uint8Array(length),subaccount:null})).owner.length,length);
assert.throws(()=>codec.encode({owner:new Uint8Array(30),subaccount:null}),e=>e instanceof WireError && e.code==='limit');
assert.throws(()=>codec.encode({owner:new Uint8Array(),subaccount:{some:new Uint8Array(31)}}),e=>e instanceof WireError && e.code==='value');
process.stdout.write(JSON.stringify(output));
