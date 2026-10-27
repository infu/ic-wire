// Canonical Postcard inside the IC Wire envelope. Metadata and scalar validators
// are trusted consumer declarations. Traversal limits do not bound JS engine heap.
export class WireError extends Error {
  constructor(readonly code: 'limit' | 'malformed' | 'identity' | 'value') {
    super(`wire ${code}`);
  }
}
type Field = { name: string; value: Schema };
export type Schema = ( { k: 'unit' | 'bool' }
  | { k: 'bytes'; min?: number; max?: number }
  | { k: 'packed-i16'; max: number }
  | { k: 'packed'; word: keyof typeof packedArrays }
  | { k: 'text'; max?: number }
  | { k: 'u' | 'i' | 'float'; bits: number; min?: number; max?: number }
  | { k: 'list'; max?: number; item: Schema }
  | { k: 'option'; item: Schema }
  | { k: 'newtype'; item: Schema }
  | { k: 'array'; max: number; item: Schema }
  | { k: 'tuple'; items: readonly Schema[] }
  | { k: 'result'; ok: Schema; err: Schema }
  | { k: 'record'; fields: readonly Field[] }
  | { k: 'enum'; variants: readonly (Field & { id: number })[] }
  | { k: 'ref'; name: string }) & { check?: string };
type Method = { id: string; request: Schema; response: Schema; requestBytes: number; responseBytes: number; requestNodes: number; responseNodes: number; requestDepth: number; responseDepth: number };
type Types = Readonly<Record<string, Schema>>;
type Checks = Readonly<Record<string, (value: unknown) => void>>;
// Allocation is deliberately absent: traversal budgets do not bound JS heap.
export type PayloadLimits = { bytes: number; nodes: number; depth: number };
const fail = (code: WireError['code']): never => { throw new WireError(code); };
const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const utf16 = new TextEncoder();
// Generic packed fields use a byte count, unlike ordinary element-count lists.
// Both endpoints must select this schema. Existing packed-i16 keeps its bound.
// Signed-byte packed fields preserve raw Vec<i8> bytes, with bulk budgets and
// Int8Array ownership. Existing schemas are unchanged; regenerate adopted fields.
const packedArrays = { i8: Int8Array, i16: Int16Array, u16: Uint16Array, i32: Int32Array, u32: Uint32Array,
  i64: BigInt64Array, u64: BigUint64Array, hash32: Uint8Array } as const;
const littleEndian = new Uint8Array(new Uint16Array([1]).buffer)[0] === 1;
function swapWords(bytes: Uint8Array, width: number): void {
  for (let start = 0; start < bytes.length; start += width) {
    for (let i = 0; i < width / 2; i++) {
      const other = start + width - 1 - i, value = bytes[start + i]!;
      bytes[start + i] = bytes[other]!; bytes[other] = value;
    }
  }
}
function object(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return fail('value');
  const own = Reflect.ownKeys(value);
  if (own.length !== keys.length || keys.some(k => !Object.hasOwn(value, k))) return fail('value');
  return value as Record<string, unknown>;
}
class Budget {
  private registrySize?: number;
  constructor(public remaining: number, readonly depth: number) {}
  visit(depth: number) { if (--this.remaining < 0 || depth > this.depth) fail('limit'); }
  length(length: number) { if (length > this.remaining) fail('limit'); }
  aliasLimit(types: Types): number { return this.registrySize ??= Object.keys(types).length; }
}
// Resolve aliases without spending a value node, but refuse missing or cyclic
// aliases. Recursive values must pass through a concrete node and its budget.
function resolve(schema: Schema, types: Types, budget: Budget, visit?: (schema: Schema) => void): Schema {
  let remaining = schema.k === 'ref' ? budget.aliasLimit(types) : 0;
  while (schema.k === 'ref') {
    if (remaining-- === 0 || !Object.hasOwn(types, schema.name)) return fail('value');
    visit?.(schema);
    const next = types[schema.name];
    if (next === undefined) return fail('value');
    schema = next;
  }
  visit?.(schema);
  return schema;
}
function validate(schema: Schema, value: unknown, checks: Checks): void {
  if (schema.check !== undefined) {
    if (!Object.hasOwn(checks, schema.check)) fail('value');
    const check = checks[schema.check];
    if (check === undefined) return fail('value');
    check(value);
  }
}
class Writer {
  bytes: Uint8Array;
  private view: DataView;
  offset = 11;
  constructor(readonly limit: number, id: bigint | null, direction: number) {
    // Previously every encode allocated the entire byte allowance. Grow one
    // private buffer instead; the ceiling, bytes and callback visits are unchanged.
    // Consumers can regenerate clients without changing contracts or retry bytes.
    this.bytes = new Uint8Array(Math.min(limit, 64));
    this.view = new DataView(this.bytes.buffer);
    // Payload-only encoding shares all traversal and growth rules. Only these
    // explicit constructors select framing; decoding never sniffs for IW.
    if (id === null) this.offset = 0;
    else {
      this.bytes.set([73, 87]);
      this.view.setBigUint64(2, id, true);
      this.bytes[10] = direction;
    }
  }
  private grow(required: number) {
    if (required > this.limit) fail('limit');
    const bytes = new Uint8Array(Math.min(this.limit, Math.max(required, this.bytes.length * 2)));
    bytes.set(this.bytes);
    this.bytes = bytes;
    this.view = new DataView(bytes.buffer);
  }
  byte(value: number) { if (this.offset === this.bytes.length) this.grow(this.offset + 1); this.bytes[this.offset++] = value; }
  varint(value: bigint) {
    do { const byte = Number(value & 127n); value >>= 7n; this.byte(value ? byte | 128 : byte); } while (value);
  }
  // Only admitted unsigned 16/32-bit values, ZigZag results and u32 tags enter here.
  numberVarint(value: number) {
    do { const byte = value & 127; value >>>= 7; this.byte(value ? byte | 128 : byte); } while (value);
  }
  float(value: number, bits: number) {
    const length = bits / 8;
    if (length > this.bytes.length - this.offset) this.grow(this.offset + length);
    if (bits === 32) this.view.setFloat32(this.offset, value, true); else this.view.setFloat64(this.offset, value, true);
    this.offset += length;
  }
  raw(bytes: Uint8Array) {
    if (bytes.length > this.bytes.length - this.offset) this.grow(this.offset + bytes.length);
    this.bytes.set(bytes, this.offset); this.offset += bytes.length;
  }
}
class Reader {
  readonly view: DataView;
  constructor(readonly bytes: Uint8Array, public offset = 11) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }
  float(bits: number): number {
    const length = bits / 8;
    if (length > this.bytes.length - this.offset) return fail('malformed');
    const value = bits === 32 ? this.view.getFloat32(this.offset, true) : this.view.getFloat64(this.offset, true);
    this.offset += length;
    return value;
  }
  byte(): number { const value = this.bytes[this.offset++]; if (value === undefined) return fail('malformed'); return value; }
  varint(bits: number): bigint {
    let value = 0n;
    for (let i = 0; i < Math.ceil(bits / 7); i++) {
      const byte = this.byte(); value |= BigInt(byte & 127) << BigInt(i * 7);
      if (!(byte & 128)) {
        if (value >= (1n << BigInt(bits)) || (i > 0 && byte === 0)) return fail('malformed');
        return value;
      }
    }
    return fail('malformed');
  }
  numberVarint(bits: number): number {
    let value = 0;
    for (let i = 0; i < Math.ceil(bits / 7); i++) {
      const byte = this.byte(); value += (byte & 127) * 2 ** (i * 7);
      if (!(byte & 128)) {
        if (value >= 2 ** bits || (i > 0 && byte === 0)) return fail('malformed');
        return value;
      }
    }
    return fail('malformed');
  }
  length(max: number): number { const n = this.varint(64); if (n > BigInt(max)) return fail('limit'); return Number(n); }
  raw(length: number): Uint8Array {
    if (length > this.bytes.length - this.offset) return fail('malformed');
    const value = this.bytes.subarray(this.offset, this.offset + length); this.offset += length; return value;
  }
}
function writeNumber(s: Extract<Schema, { k: 'u' | 'i' | 'float' }>, value: unknown, out: Writer): void {
  if (typeof value !== 'number' || !Number.isSafeInteger(value)) fail('value');
  const n = value as number, signed = s.k === 'i';
  const min = signed ? -(2 ** (s.bits - 1)) : 0, max = 2 ** (s.bits - (signed ? 1 : 0)) - 1;
  if (n < min || n > max) fail('limit');
  if (s.min !== undefined && n < s.min || s.max !== undefined && n > s.max) fail('value');
  if (s.bits === 8) out.byte(n & 255);
  else out.numberVarint(signed ? (n < 0 ? -n * 2 - 1 : n * 2) : n);
  return;
}
function write(s: Schema, value: unknown, out: Writer, budget: Budget, depth: number, types: Types, checks: Checks): void {
  budget.visit(depth);
  s = resolve(s, types, budget, schema => validate(schema, value, checks));
  const child = (schema: Schema, item: unknown) => write(schema, item, out, budget, depth + 1, types, checks);
  switch (s.k) {
    case 'unit': if (value !== null) fail('value'); return;
    case 'bool': if (typeof value !== 'boolean') fail('value'); out.byte(value ? 1 : 0); return;
    case 'u': case 'i': {
      if (s.bits <= 32) return writeNumber(s, value, out);
      if (typeof value !== 'bigint') fail('value');
      const n = BigInt(value as number | bigint), bits = BigInt(s.bits), signed = s.k === 'i';
      const min = signed ? -(1n << (bits - 1n)) : 0n, max = (1n << (bits - (signed ? 1n : 0n))) - 1n;
      if (n < min || n > max) fail('limit');
      if (s.min !== undefined && n < BigInt(s.min) || s.max !== undefined && n > BigInt(s.max)) fail('value');
      out.varint(signed ? (n << 1n) ^ (n >> (bits - 1n)) : n); return;
    }
    case 'float': {
      if (typeof value !== 'number' || !Number.isFinite(value) || (s.bits === 32 && !Object.is(Math.fround(value), value))) fail('value');
      const n = value as number;
      if (s.min !== undefined && n < s.min || s.max !== undefined && n > s.max) fail('value');
      out.float(n, s.bits); return;
    }
    case 'bytes': {
      if (!(value instanceof Uint8Array)) fail('value');
      const bytes = value as Uint8Array;
      if (s.min !== undefined && bytes.length < s.min) fail('value');
      if (s.max !== undefined && bytes.length > s.max) fail('limit');
      out.varint(BigInt(bytes.length)); out.raw(bytes); return;
    }
    case 'packed-i16': case 'packed': {
      const kind = s.k === 'packed-i16' ? 'i16' : s.word, ctor = packedArrays[kind];
      if (!(value instanceof ctor)) return fail('value');
      const width = kind === 'hash32' ? 32 : ctor.BYTES_PER_ELEMENT;
      if (value.byteLength % width) return fail('value');
      if (s.k === 'packed-i16' && value.byteLength / width > s.max) fail('limit');
      out.varint(BigInt(value.byteLength));
      if (value.byteLength > out.limit - out.offset) fail('limit');
      const bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
      if (littleEndian || kind === 'hash32') out.raw(bytes);
      else { const copy = new Uint8Array(bytes); swapWords(copy, width); out.raw(copy); }
      return;
    }
    case 'text': {
      if (typeof value !== 'string') fail('value');
      const text = value as string, remaining = Math.min(s.max ?? out.limit, out.limit - out.offset);
      if (text.length > remaining) fail('limit');
      // UTF-8 needs at most three bytes per UTF-16 code unit.
      const buffer = new Uint8Array(Math.min(remaining, text.length * 3)), result = utf16.encodeInto(text, buffer);
      if (result.read !== text.length) fail('limit');
      const bytes = buffer.subarray(0, result.written);
      if (utf8.decode(bytes) !== text) fail('value');
      out.varint(BigInt(bytes.length)); out.raw(bytes); return;
    }
    case 'newtype': child(s.item, value); return;
    case 'list': case 'array': case 'tuple': {
      if (!Array.isArray(value)) fail('value');
      const values = value as unknown[];
      if (s.k === 'array' && values.length !== s.max || s.k === 'list' && values.length > (s.max ?? budget.remaining)) fail('limit');
      if (s.k === 'tuple' && values.length !== s.items.length) fail('value');
      budget.length(values.length);
      if (s.k === 'list') out.varint(BigInt(values.length));
      for (let i = 0; i < values.length; i++) {
        if (!Object.hasOwn(values, i)) fail('value');
        const item = s.k === 'tuple' ? s.items[i] : s.item;
        if (item === undefined) return fail('value');
        child(item, values[i]);
      }
      return;
    }
    case 'option': {
      if (value === null) { out.byte(0); return; }
      const record = object(value, ['some']); out.byte(1); child(s.item, record.some); return;
    }
    case 'result': {
      const key = value && typeof value === 'object' && Object.hasOwn(value, 'ok') ? 'ok' : 'err';
      const record = object(value, [key]); out.byte(key === 'ok' ? 0 : 1); child(s[key], record[key]); return;
    }
    case 'record': {
      const record = object(value, s.fields.map(f => f.name));
      for (const field of s.fields) child(field.value, record[field.name]); return;
    }
    case 'enum': {
      const record = object(value, ['tag', 'value']);
      const variant = s.variants.find(v => v.name === record.tag); if (!variant) return fail('value');
      out.numberVarint(variant.id); child(variant.value, record.value); return;
    }
  }
}
function read(s: Schema, input: Reader, budget: Budget, depth: number, types: Types, checks: Checks): unknown {
  const value = readValue(resolve(s, types, budget), input, budget, depth, types, checks);
  resolve(s, types, budget, schema => validate(schema, value, checks));
  return value;
}
function readNumber(s: Extract<Schema, { k: 'u' | 'i' | 'float' }>, input: Reader): number {
  let n = s.bits === 8 ? input.byte() : input.numberVarint(s.bits);
  if (s.k === 'i') n = s.bits === 8 ? (n >= 128 ? n - 256 : n) : (n % 2 ? -(n + 1) / 2 : n / 2);
  if (s.min !== undefined && n < s.min || s.max !== undefined && n > s.max) fail('malformed');
  return n;
}
function readValue(s: Schema, input: Reader, budget: Budget, depth: number, types: Types, checks: Checks): unknown {
  budget.visit(depth);
  const child = (schema: Schema) => read(schema, input, budget, depth + 1, types, checks);
  switch (s.k) {
    case 'unit': return null;
    case 'bool': { const n = input.byte(); if (n > 1) return fail('malformed'); return n === 1; }
    case 'u': case 'i': {
      if (s.bits <= 32) return readNumber(s, input);
      let n = input.varint(s.bits);
      if (s.k === 'i') n = (n >> 1n) ^ -(n & 1n);
      if (s.min !== undefined && n < BigInt(s.min) || s.max !== undefined && n > BigInt(s.max)) fail('malformed');
      return n;
    }
    case 'float': {
      const value = input.float(s.bits);
      if (!Number.isFinite(value) || s.min !== undefined && value < s.min || s.max !== undefined && value > s.max) return fail('malformed'); return value;
    }
    case 'bytes': {
      const length = input.length(s.max ?? input.bytes.length - input.offset);
      if (s.min !== undefined && length < s.min) fail('malformed');
      return new Uint8Array(input.raw(length));
    }
    case 'packed-i16': case 'packed': {
      const kind = s.k === 'packed-i16' ? 'i16' : s.word, ctor = packedArrays[kind];
      const width = kind === 'hash32' ? 32 : ctor.BYTES_PER_ELEMENT;
      const max = s.k === 'packed-i16' ? s.max * width : input.bytes.length - input.offset;
      const length = input.length(Math.min(max, input.bytes.length - input.offset));
      // New packed schemas reject partial words of their selected width. The
      // existing packed-i16 schema still rejects odd lengths and values over max.
      if (length % width) return fail('malformed');
      if (length === 0) return new ctor(0);
      // Check the entire span before allocating. Copy also handles unaligned input
      // and ensures decoded values never retain or mutate the caller's buffer.
      const bytes = new Uint8Array(input.raw(length));
      if (!littleEndian && kind !== 'hash32') swapWords(bytes, width);
      return new ctor(bytes.buffer);
    }
    case 'text': {
      const text = utf8.decode(input.raw(input.length(Math.min(s.max ?? input.bytes.length, input.bytes.length - input.offset))));
      return text;
    }
    case 'newtype': return child(s.item);
    case 'list': case 'array': case 'tuple': {
      const length = s.k === 'list' ? input.length(Math.min(s.max ?? budget.remaining, budget.remaining)) : s.k === 'array' ? s.max : s.items.length;
      budget.length(length);
      const values: unknown[] = [];
      for (let i = 0; i < length; i++) {
        const item = s.k === 'tuple' ? s.items[i] : s.item;
        if (item === undefined) return fail('value');
        values.push(child(item));
      }
      return values;
    }
    case 'option': {
      const tag = input.byte(); if (tag === 0) return null; if (tag !== 1) return fail('malformed');
      return { some: child(s.item) };
    }
    case 'result': {
      const tag = input.byte(); if (tag > 1) return fail('malformed');
      return tag === 0 ? { ok: child(s.ok) } : { err: child(s.err) };
    }
    case 'record': {
      const value = Object.create(null) as Record<string, unknown>;
      for (const field of s.fields) value[field.name] = child(field.value);
      return value;
    }
    case 'enum': {
      const id = input.numberVarint(32), variant = s.variants.find(v => v.id === id);
      if (!variant) return fail('malformed');
      return { tag: variant.name, value: child(variant.value) };
    }
  }
}
// Exact native::encode/decode payloads, without a method/direction envelope.
// Metadata is consumer-owned and trusted, just as in createCodec. A successful
// result owns its bytes; these helpers introduce no SDK or transport policy.
export function createPayloadCodec(schema: unknown, limits: PayloadLimits, registry: unknown = {}, checks: Checks = {}) {
  const type = schema as Schema, types = registry as Types;
  for (const [value, minimum] of [[limits.bytes, 0], [limits.nodes, 1], [limits.depth, 1]] as const) {
    if (!Number.isSafeInteger(value) || value < minimum) fail('value');
  }
  return {
    encode(value: unknown): Uint8Array {
      const out = new Writer(limits.bytes, null, 0);
      write(type, value, out, new Budget(limits.nodes, limits.depth), 1, types, checks);
      return out.offset === out.bytes.length ? out.bytes : out.bytes.slice(0, out.offset);
    },
    decode(bytes: Uint8Array): unknown {
      if (!(bytes instanceof Uint8Array)) return fail('value');
      if (bytes.length > limits.bytes) return fail('limit');
      try {
        const input = new Reader(bytes, 0);
        const value = read(type, input, new Budget(limits.nodes, limits.depth), 1, types, checks);
        if (input.offset !== bytes.length) return fail('malformed');
        return value;
      } catch (error) { if (error instanceof WireError) throw error; return fail('malformed'); }
    },
  };
}
// Accept only generated trusted metadata. No SDK/auth/transport policy lives here.
export function createCodec(metadata: unknown, registry: unknown = {}, checks: Checks = {}) {
  const meta = metadata as Method, types = registry as Types, id = BigInt(meta.id);
  if (id < 0n || id >= (1n << 64n)) fail('value');
  for (const value of [meta.requestBytes, meta.responseBytes, meta.requestNodes, meta.responseNodes, meta.requestDepth, meta.responseDepth]) {
    if (!Number.isSafeInteger(value) || value < 1) fail('value');
  }
  if (meta.requestBytes < 11 || meta.responseBytes < 11) fail('value');
  function encode(value: unknown, direction: 0 | 1): Uint8Array {
    const out = new Writer(direction ? meta.responseBytes : meta.requestBytes, id, direction);
    write(direction ? meta.response : meta.request, value, out, new Budget(direction ? meta.responseNodes : meta.requestNodes, direction ? meta.responseDepth : meta.requestDepth), 1, types, checks);
    // A full private buffer is already an exact owned result; otherwise trim it.
    return out.offset === out.bytes.length ? out.bytes : out.bytes.slice(0, out.offset);
  }
  function decode(bytes: Uint8Array, direction: 0 | 1): unknown {
    if (!(bytes instanceof Uint8Array)) return fail('value');
    if (bytes.length > (direction ? meta.responseBytes : meta.requestBytes)) return fail('limit');
    if (bytes.length < 11) return fail('malformed');
    if (bytes[0] !== 73 || bytes[1] !== 87 || bytes[10] !== direction) return fail('identity');
    try {
      const input = new Reader(bytes);
      if (input.view.getBigUint64(2, true) !== id) return fail('identity');
      const value = read(direction ? meta.response : meta.request, input, new Budget(direction ? meta.responseNodes : meta.requestNodes, direction ? meta.responseDepth : meta.requestDepth), 1, types, checks);
      if (input.offset !== bytes.length) return fail('malformed');
      return value;
    } catch (error) { if (error instanceof WireError) throw error; return fail('malformed'); }
  }
  return { encodeRequest: (value: unknown) => encode(value, 0), decodeRequest: (bytes: Uint8Array) => decode(bytes, 0), encodeResponse: (value: unknown) => encode(value, 1), decodeResponse: (bytes: Uint8Array) => decode(bytes, 1) };
}
