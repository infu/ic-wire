import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {fieldCodec} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
const {createPayloadCodec,WireError}=await import(pathToFileURL(process.argv[1]));
const cases=[['i8',Int8Array,1],['i16',Int16Array,2],['u16',Uint16Array,2],['i32',Int32Array,4],['u32',Uint32Array,4],['i64',BigInt64Array,8],['u64',BigUint64Array,8],['hash16',Uint8Array,16],['hash32',Uint8Array,32],['bool',Array,1],['finite-f32',Float32Array,4],['finite-f64',Float64Array,8],['i64',BigInt64Array,8,true]];
for(const [word,ctor,width,nonzero] of cases) {
  const schema={k:'packed',word,max:1,...(nonzero?{nonzero:true}:{})};
  const codec=fieldCodec(createPayloadCodec, {schema,bytes:128,nodes:1,depth:1});
  const value=new ctor(word.startsWith('hash')?width:1).fill(nonzero?1n:word==='i64'||word==='u64'?0n:word==='bool'?false:0);
  const literal=new Uint8Array(width+2);literal[0]=width;if(nonzero)literal[1]=1;
  assert.deepEqual(codec.encode(value),literal);
  assert.deepEqual(codec.decode(literal),value);
  const tooMany=new ctor(value.length*2).fill(value[0]);
  assert.throws(()=>codec.encode(tooMany),e=>e instanceof WireError&&e.code==='limit');
  const over=new Uint8Array(width*2+1);over[0]=width*2;if(nonzero){over[1]=1;over[width+1]=1;}
  assert.throws(()=>codec.decode(over),e=>e instanceof WireError&&e.code==='limit');
  const empty=fieldCodec(createPayloadCodec, {schema:{...schema,max:0},bytes:128,nodes:1,depth:1});
  assert.deepEqual(empty.encode(new ctor(0)),Uint8Array.of(0,0));
  assert.throws(()=>empty.decode(literal),e=>e instanceof WireError&&e.code==='limit');
}
process.stdout.write(JSON.stringify(cases.length));
