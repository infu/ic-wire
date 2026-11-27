import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {fieldCodec, slotPayload, slotMethod} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin='';for await(const chunk of process.stdin)stdin+=chunk;
const fixture=JSON.parse(stdin),url=pathToFileURL(process.argv[1]);
const {createPayloadCodec,WireError}=await import(url);
const {checked:rawChecked}=await import(new URL('./client.ts',url));
const {method:rawMethod}=await import(new URL('./frame-client.ts',url));
const {calls}=await import(new URL('./runtime-adapter.ts',url));
const checked = slotPayload(rawChecked), method = slotMethod(rawMethod);
const refusal=(action,code)=>assert.throws(action,e=>e instanceof WireError&&e.code===code);
assert.deepEqual(Array.from(checked.encode(4)),[4,0]);assert.deepEqual(calls.splice(0),['root','alias','item','leaf']);
assert.equal(checked.decode(Uint8Array.of(4,0)),4);assert.deepEqual(calls.splice(0),['item','leaf','root','alias']);
refusal(()=>checked.encode(1),'value');assert.deepEqual(calls.splice(0),['root','alias','item','leaf']);
refusal(()=>checked.decode(Uint8Array.of(1)),'malformed');assert.deepEqual(calls,[]);
refusal(()=>checked.decode(Uint8Array.of(4,0,0)),'malformed');assert.deepEqual(calls.splice(0),['item','leaf','root','alias']);
for(const direction of ['Request','Response']) {
  const bytes=method['encode'+direction](4);assert.deepEqual(calls.splice(0),['root','alias','item','leaf']);
  assert.equal(method['decode'+direction](bytes),4);assert.deepEqual(calls.splice(0),['item','leaf','root','alias']);
}
const meta={schema:{k:'ref',name:'Root'},bytes:16,nodes:2,depth:2};
const absent=fieldCodec(createPayloadCodec, meta,fixture.types);
refusal(()=>absent.encode(4),'value');refusal(()=>absent.decode(Uint8Array.of(4,0)),'value');
const checks=Object.fromEntries(['root','alias','item','leaf'].map(name=>[name,()=>calls.push(name)]));
const short=fieldCodec(createPayloadCodec, {...meta,nodes:1},fixture.types,checks);
refusal(()=>short.encode(4),'limit');assert.deepEqual(calls.splice(0),['root','alias']);
refusal(()=>short.decode(Uint8Array.of(4,0)),'limit');assert.deepEqual(calls,[]);
for(const k of ['u','i'])for(const bits of [8,16,32,64,128]) {
  const schema={k,bits,min:2,max:5},codec=fieldCodec(createPayloadCodec, {schema,bytes:32,nodes:1,depth:1});
  const wide=bits>32;
  for(const value of [2,5]) assert.equal(codec.decode(codec.encode(wide?BigInt(value):value)),wide?BigInt(value):value);
  refusal(()=>codec.encode(wide?1n:1),'value');
  const unrestricted=fieldCodec(createPayloadCodec, {schema:{k,bits},bytes:32,nodes:1,depth:1});
  refusal(()=>codec.decode(unrestricted.encode(wide?1n:1)),'malformed');
}
for(const bits of [32,64]) {
  const codec=fieldCodec(createPayloadCodec, {schema:{k:'float',bits,min:-0.5,max:1.5},bytes:9,nodes:1,depth:1});
  for(const value of [-0.5,-0,1.5]) assert(Object.is(codec.decode(codec.encode(value)),value));
  refusal(()=>codec.encode(2),'value');
  const bytes=new Uint8Array(bits/8),view=new DataView(bytes.buffer);if(bits===32)view.setFloat32(0,2,true);else view.setFloat64(0,2,true);
  refusal(()=>codec.decode(bytes),'malformed');
}
const scalar={schema:{k:'u',bits:8,check:'test'},bytes:4,nodes:1,depth:1};
const inherited=fieldCodec(createPayloadCodec, scalar,{},Object.create({test:()=>{}}));refusal(()=>inherited.encode(4),'value');
const thrown=new Error('consumer rejection'),reject=fieldCodec(createPayloadCodec, scalar,{}, {test:()=>{throw thrown;}});
assert.throws(()=>reject.encode(4),e=>e===thrown);refusal(()=>reject.decode(Uint8Array.of(4,0)),'malformed');
const typed=fieldCodec(createPayloadCodec, scalar,{}, {test:()=>{throw new WireError('value');}});refusal(()=>typed.decode(Uint8Array.of(4,0)),'value');
const returnsFalse=fieldCodec(createPayloadCodec, scalar,{}, {test:()=>false});assert.equal(returnsFalse.decode(Uint8Array.of(4,0)),4);
process.stdout.write(JSON.stringify(true));
