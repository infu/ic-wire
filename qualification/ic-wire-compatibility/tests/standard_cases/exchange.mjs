import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const {fieldCodec, messageBytes, slotPayload, slotMethod} = await import(new URL('./message.mjs', pathToFileURL(process.argv[1])));
let stdin=''; for await(const chunk of process.stdin) stdin+=chunk;
const fixture=JSON.parse(stdin), url=pathToFileURL(process.argv[1]);
const {createPayloadCodec,WireError}=await import(url);
const methods=Object.fromEntries(Object.entries(await import(new URL('./frame-client.ts',url))).map(([name,codec])=>[name,slotMethod(codec)]));
const {outcomePayload:rawOutcome}=await import(new URL('./client.ts',url));
const outcomePayload = slotPayload(rawOutcome);
const refusal=(action,code)=>assert.throws(action,e=>e instanceof WireError && e.code===code);
const output=[];
for(const test of fixture.cases) {
  const expected=messageBytes(test.bytes);
  const meta={schema:{k:'ref',name:test.root},bytes:expected.length,nodes:test.nodes,depth:test.depth};
  const codec=fieldCodec(createPayloadCodec, meta,fixture.types);
  assert.deepEqual(codec.encode(test.value),expected);
  assert.deepEqual(codec.decode(expected),test.value);
  for(const resource of ['bytes','nodes','depth']) {
    if(meta[resource]>1) {
      const short=fieldCodec(createPayloadCodec, {...meta,[resource]:meta[resource]-1},fixture.types);
      refusal(()=>short.encode(test.value),'limit'); refusal(()=>short.decode(expected),'limit');
    } else if(resource==='bytes') {
      refusal(()=>fieldCodec(createPayloadCodec, {...meta,bytes:0},fixture.types),'value');
    }
  }
  for(const direction of ['Request','Response']) {
    const framed=methods[test.method]['encode'+direction](test.value);
    assert.deepEqual(framed.subarray(11),expected);
    assert.deepEqual(methods[test.method]['decode'+direction](framed),test.value);
  }
  output.push(Array.from(expected));
}
assert.deepEqual(outcomePayload.decode(Uint8Array.of(128,0,0,255,0)),{ok:[0,255]});
assert.deepEqual(outcomePayload.decode(Uint8Array.of(129,0,128,1,1,0)),{err:[128,true]});
for(const tag of [[2],[128,128,128,128,16],[128,128,128,128,128]]) refusal(()=>outcomePayload.decode(Uint8Array.from(tag)),'malformed');
refusal(()=>outcomePayload.encode({ok:[1]}),'limit');
refusal(()=>outcomePayload.encode({ok:[1,2,3]}),'limit');
refusal(()=>outcomePayload.encode({ok:[1,2],err:[1,true]}),'value');
refusal(()=>outcomePayload.encode({err:[1]}),'value');
const sparse=[]; sparse.length=2; sparse[1]=2;
refusal(()=>outcomePayload.encode({ok:sparse}),'value');
process.stdout.write(JSON.stringify(output));
