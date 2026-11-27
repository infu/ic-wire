// Helpers for testing a fixed field within a one-slot sparse message.
export function messageSchema(item) {
  return { k: 'extension-tuple', items: [item], extensions: [] };
}

export function messageBytes(bytes) {
  const message = new Uint8Array(bytes.length + 1);
  message.set(bytes);
  return message;
}

export function slotPayload(codec) {
  return {
    encode: value => codec.encode([value]),
    decode: bytes => codec.decode(bytes)[0],
  };
}

export function slotMethod(codec) {
  return {
    encodeRequest: value => codec.encodeRequest([value]),
    decodeRequest: bytes => codec.decodeRequest(bytes)[0],
    encodeResponse: value => codec.encodeResponse([value]),
    decodeResponse: bytes => codec.decodeResponse(bytes)[0],
  };
}

export function fieldCodec(create, meta, types, checks) {
  return slotPayload(create({
    ...meta,
    schema: messageSchema(meta.schema),
    nodes: meta.nodes + 1,
    depth: meta.depth + 1,
  }, types, checks));
}
