import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {slotPayload} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin=''; for await(const chunk of process.stdin) stdin+=chunk;
const fixture=JSON.parse(stdin), url=pathToFileURL(process.argv[1]);
const {codec:rawPayload}=await import(new URL('./client.ts',url));
const codec = slotPayload(rawPayload);
const {WireError}=await import(url);
const output=[];
for(const value of [[0,0],[-0,-0],[2**-149,Number.MIN_VALUE],[1.5,1.5]]) {
  const bytes=codec.encode(value); assert.deepEqual(Array.from(bytes),fixture.literals[output.length]);
  const selected=new Uint8Array(15); selected.set(bytes,1);
  const decoded=codec.decode(selected.subarray(1,14));
  assert(Object.is(decoded[0],value[0])); assert(Object.is(decoded[1],value[1]));
  output.push(Array.from(bytes));
}
for(const value of [NaN,Infinity,-Infinity]) {
  assert.throws(()=>codec.encode([value,0]),e=>e instanceof WireError && e.code==='value');
  assert.throws(()=>codec.encode([0,value]),e=>e instanceof WireError && e.code==='value');
  for(const width of [32,64]) {
    const bytes=new Uint8Array(13), view=new DataView(bytes.buffer);
    if(width===32) view.setFloat32(0,value,true); else view.setFloat64(4,value,true);
    assert.throws(()=>codec.decode(bytes),e=>e instanceof WireError && e.code==='malformed');
  }
}
assert.throws(()=>codec.encode([0.1,0]),e=>e instanceof WireError && e.code==='value');
assert.throws(()=>codec.decode(new Uint8Array(11)),e=>e instanceof WireError && e.code==='malformed');
process.stdout.write(JSON.stringify(output));
