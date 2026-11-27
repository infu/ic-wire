import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {fieldCodec} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin=''; for await(const chunk of process.stdin) stdin+=chunk;
const fixture=JSON.parse(stdin), url=pathToFileURL(process.argv[1]);
const {createPayloadCodec,WireError}=await import(url);
const values=[Int8Array.of(-128,127),Int16Array.of(-1,128),Uint16Array.of(0,65535),Int32Array.of(-1,128),Uint32Array.of(0x12345678),BigInt64Array.of(-1n),BigUint64Array.of(0x0102030405060708n),new Uint8Array(16).fill(7),new Uint8Array(32).fill(9)];
for(const [index,test] of fixture.cases.entries()) {
  const value=values[index], meta={schema:test.schema,bytes:test.bytes.length,nodes:1,depth:1};
  const codec=fieldCodec(createPayloadCodec, meta);
  assert.deepEqual(Array.from(codec.encode(value)),test.bytes);
  const input=new Uint8Array(test.bytes.length+2); input.set(test.bytes,1);
  const decoded=codec.decode(input.subarray(1,-1)); assert.deepEqual(decoded,value);
  input.fill(0); assert.deepEqual(decoded,value);
  assert.deepEqual(codec.decode(Uint8Array.of(0,0)),new value.constructor(0));
  const short=fieldCodec(createPayloadCodec, {...meta,bytes:meta.bytes-1});
  assert.throws(()=>short.encode(value),e=>e instanceof WireError && e.code==='limit');
  if(index>0) assert.throws(()=>codec.decode(Uint8Array.of(1,0)),e=>e instanceof WireError && e.code==='malformed');
}
process.stdout.write(JSON.stringify(fixture.cases.length));
