// Consumer-owned presentation; the shared codec still handles all wire bytes.
import { createPayloadCodec as wirePayload, WireError, type PayloadLimits } from './wire-runtime.js';

function magnitude(value: unknown): asserts value is Uint8Array {
  if (!(value instanceof Uint8Array) || value.length > 64 || value[0] === 0) throw new WireError('value');
}

// This export is selected by generate_payload_registry's existing runtime_module.
export function createPayloadCodec(schema: unknown, limits: PayloadLimits, types: unknown) {
  return wirePayload(schema, limits, types, {magnitude});
}

export function unsignedMagnitude(limits: PayloadLimits) {
  const codec = createPayloadCodec({k:'bytes',max:64,check:'magnitude'},limits,{});
  return {
    encode(value: bigint): Uint8Array {
      if (typeof value !== 'bigint' || value < 0n || value >> 512n !== 0n) throw new WireError('value');
      const bytes: number[] = [];
      while (value) { bytes.push(Number(value & 255n)); value >>= 8n; }
      return codec.encode(Uint8Array.from(bytes.reverse()));
    },
    decode(bytes: Uint8Array): bigint {
      const raw = codec.decode(bytes);
      magnitude(raw);
      let value = 0n;
      for (const byte of raw) value = value << 8n | BigInt(byte);
      return value;
    },
  };
}
