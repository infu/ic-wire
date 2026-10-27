const payloadFactory = (await import('./compiled/wire-runtime.js')).createPayloadCodec;
function checkPayload(codec, value, expected, mutations) {
  for (const direction of ['Request','Response']) {
    const encode = 'encode'+direction, decode = 'decode'+direction;
    assert.deepEqual([...codec[encode](value)], expected);
    assert.deepEqual(normalize(codec[decode](new Uint8Array(expected))), normalize(value));
    for (const row of mutations) {
      let accepted = false;
      try { codec[decode](new Uint8Array(row.bytes)); accepted = true; }
      catch (error) { assert.ok(error instanceof WireError); }
      assert.equal(accepted, row.accepted, `payload mutation ${row.bytes}`);
    }
  }
}
const payloadLimits = {bytes:4096,nodes:8192,depth:32};
const emptyPayload = payloadFactory({k:'unit'},{bytes:0,nodes:1,depth:1});
assert.deepEqual([...emptyPayload.encode(null)], []);
assert.equal(emptyPayload.decode(new Uint8Array()), null);
assert.throws(()=>emptyPayload.decode(new Uint8Array([0])), e=>e instanceof WireError && e.code==='limit');
for (const invalid of [{bytes:-1},{nodes:0},{depth:0},{bytes:Infinity},{bytes:1.5}]) {
  assert.throws(()=>payloadFactory({k:'unit'},{...payloadLimits,...invalid}), WireError);
}
const payloadBytes = payloadFactory({k:'bytes'},payloadLimits), sourceBytes = new Uint8Array([3,1,2,3]);
const decodedBytes = payloadBytes.decode(sourceBytes);
sourceBytes.fill(0);
assert.deepEqual(decodedBytes, new Uint8Array([1,2,3]));
for (const [schema,value] of [[{k:'list',item:{k:'u',bits:8}},[1,2]], [{k:'tuple',items:[{k:'unit'}]},[null]]]) {
  assert.throws(()=>payloadFactory(schema,{...payloadLimits,nodes:1}).encode(value),WireError);
  const encoded = payloadFactory(schema,payloadLimits).encode(value);
  assert.throws(()=>payloadFactory(schema,{...payloadLimits,nodes:1}).decode(encoded),WireError);
}
assert.throws(()=>payloadFactory({k:'text'},{...payloadLimits,bytes:3}).encode('abc'),WireError);
assert.throws(()=>payloadFactory({k:'text'},{...payloadLimits,bytes:3}).decode(new Uint8Array([3,97,98,99])),WireError);
const nestedPayload = {k:'option',item:{k:'option',item:{k:'unit'}}};
assert.throws(()=>payloadFactory(nestedPayload,{...payloadLimits,depth:1}).encode({some:{some:null}}),WireError);
const payloadU64 = payloadFactory({k:'u',bits:64},payloadLimits);
for(const invalid of [[128,0],[255,255,255,255,255,255,255,255,255,2],[7,0]]) {
  assert.throws(()=>payloadU64.decode(new Uint8Array(invalid)),WireError);
}
// A payload beginning with IW is ordinary data, never detected as an envelope.
const looksFramed = new Uint8Array([73,87,0,0,0,0,0,0,0,0,0]);
const headerArray = payloadFactory({k:'array',max:11,item:{k:'u',bits:8}},payloadLimits);
assert.deepEqual(headerArray.decode(looksFramed), [...looksFramed]);
assert.deepEqual(headerArray.encode([...looksFramed]), looksFramed);
console.log('Payload codecs: Rust vectors, mutations, limits, ownership and explicit framing passed');
