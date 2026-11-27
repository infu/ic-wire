import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {fieldCodec, slotMethod} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin='';for await(const chunk of process.stdin)stdin+=chunk;
const fixture=JSON.parse(stdin),url=pathToFileURL(process.argv[1]);
const {createPayloadCodec,WireError}=await import(url);
const methods=Object.fromEntries(Object.entries(await import(new URL('./frame-client.ts',url))).map(([name,codec])=>[name,slotMethod(codec)]));
const cases=[['Bools','bools',[false,true]],['F32','floats32',Float32Array.of(-0,2**-149)],['F64','floats64',Float64Array.of(-0,Number.MIN_VALUE)],['Nonzero','nonzero',BigInt64Array.of(-1n,7n)],['Heap','heap',BigUint64Array.of(3n,1n,3n)]];
const refusal=(action,code)=>assert.throws(action,e=>e instanceof WireError&&e.code===code);
const output=[];
for(const [index,[root,method,value]] of cases.entries()) {
  const meta={schema:{k:'ref',name:root},bytes:64,nodes:2,depth:2},codec=fieldCodec(createPayloadCodec, meta,fixture.types);
  const bytes=codec.encode({value});assert.deepEqual(Array.from(bytes),fixture.bytes[index]);
  const selected=new Uint8Array(bytes.length+2);selected.set(bytes,1);
  const decoded=codec.decode(selected.subarray(1,-1));assert.deepEqual(decoded.value,value);
  selected.fill(0);assert.deepEqual(decoded.value,value);
  for(const direction of ['Request','Response']) {
    const frame=methods[method]['encode'+direction]({value});assert.deepEqual(frame.subarray(11),bytes);
    assert.deepEqual(methods[method]['decode'+direction](frame).value,value);
  }
  for(const key of ['nodes','depth']) refusal(()=>fieldCodec(createPayloadCodec, {...meta,[key]:1},fixture.types).decode(bytes),'limit');
  output.push(Array.from(bytes));
  refusal(()=>codec.decode(bytes.subarray(0,-1)),'malformed');
  if(root==='Bools') {
    for(let n=2;n<256;n++)refusal(()=>codec.decode(Uint8Array.of(1,n)),'malformed');
    refusal(()=>codec.encode({value:[false,1]}),'value');
    refusal(()=>codec.encode({value:new Array(2)}),'value');
  } else if(root==='F32'||root==='F64') {
    for(const value of [NaN,Infinity,-Infinity]) {
      const bad=root==='F32'?Float32Array.of(value):Float64Array.of(value);
      refusal(()=>codec.encode({value:bad}),'value');
      const bytes=new Uint8Array(1+bad.BYTES_PER_ELEMENT);bytes[0]=bad.BYTES_PER_ELEMENT;
      const view=new DataView(bytes.buffer);if(root==='F32')view.setFloat32(1,value,true);else view.setFloat64(1,value,true);
      refusal(()=>codec.decode(bytes),'malformed');
    }
    refusal(()=>codec.decode(Uint8Array.of(1,0)),'malformed');
  } else if(root==='Nonzero') {
    refusal(()=>codec.encode({value:BigInt64Array.of(0n)}),'value');
    refusal(()=>codec.decode(Uint8Array.of(8,0,0,0,0,0,0,0,0)),'malformed');
  }
}
process.stdout.write(JSON.stringify(output));
