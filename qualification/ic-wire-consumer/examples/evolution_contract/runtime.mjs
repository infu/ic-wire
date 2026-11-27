import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createPayloadCodec, WireError } from '../../../../crates/ic-wire/src/typescript/runtime.ts';

function codec(schema, limits = {}) {
  return createPayloadCodec({ schema, bytes: 1024, nodes: 128, depth: 8, ...limits });
}

const message = (...items) => ({ k: 'extension-tuple', items, extensions: [] });
const messageBytes = (...core) => Uint8Array.of(...core, 0);

function failure(code, operation) {
  assert.throws(operation, error => error instanceof WireError && error.code === code);
}

const unit = { k: 'unit' };
const u32 = { k: 'u', bits: 32 };
const u8 = { k: 'u', bits: 8 };
const active = { id: 1, name: 'Active', value: unit };
const archive = { id: 3, name: 'Archived', value: u8 };
const enabled = { tag: 'Active', value: null };
const archive42 = { tag: 'Archived', value: 42 };
const fields = variants => [
  { name: 'id', value: u32 },
  { name: 'state', value: { k: 'open-variant', variants } },
];
const extensions = [
  { id: 2, name: 'note', value: { k: 'text' }, required: false },
  { id: 5, name: 'stamp', value: unit, required: false },
  { id: 8, name: 'epoch', value: u32, required: true },
];
function batch(fields, extensions) {
  const item = { k: 'extension-record', fields, extensions };
  return {
    k: 'extension-record',
    extensions: [],
    fields: [
      { name: 'entries', value: { k: 'list', item } },
      { name: 'suffix', value: u32 },
    ],
  };
}

// Convert the runtime's prototype-free records for equality with literals.
function asPlain(value) { return JSON.parse(JSON.stringify(value)); }

test('projects sparse entries and open variants in a nested list', () => {
  const old = codec(batch(fields([active]), []));
  const current = codec(batch(fields([active, archive]), extensions));
  const recentValue = {
    entries: [
      { id: 7, state: archive42, note: { some: 'x' }, stamp: { some: null }, epoch: 130 },
      { id: 8, state: enabled, note: null, stamp: null, epoch: 9 },
    ],
    suffix: 42,
  };
  const bytes = messageBytes(
    2, 7, 7, 1, 42, 10, 2, 2, 1, 0x78, 5, 0, 8, 2, 0x82, 1,
    8, 2, 3, 8, 1, 9, 42,
  );
  assert.deepEqual(current.encode(recentValue), bytes);
  assert.deepEqual(asPlain(current.decode(bytes)), recentValue);
  const olderValue = { entries: [{ id: 7, state: null }, { id: 8, state: enabled }], suffix: 42 };
  assert.deepEqual(asPlain(old.decode(bytes)), olderValue);
  failure('incompatible', () => current.decode(old.encode(olderValue)));

  const optionalOnly = codec(batch(fields([active, archive]), extensions.slice(0, 2)));
  assert.deepEqual(asPlain(optionalOnly.decode(old.encode(olderValue))), {
    entries: olderValue.entries.map(entry => ({ ...entry, note: null, stamp: null })),
    suffix: 42,
  });
});

test('rejects invalid extension headers and preserves hard faults in known bodies', () => {
  const unknownOnly = codec({ k: 'extension-record', fields: [], extensions: [] });
  for (const bytes of [
    [4, 1, 0, 1, 0], // duplicate
    [4, 2, 0, 1, 0], // descending
    [2, 0, 0], // zero ID
    [2, 1, 5], // out-of-block field body
    [6, 1, 0], // out-of-input block
  ]) failure('malformed', () => unknownOnly.decode(Uint8Array.from(bytes)));
  const readable = codec({
    k: 'extension-record', fields: [],
    extensions: [{ id: 1, name: 'flag', value: { k: 'bool' }, required: true }],
  });
  failure('malformed', () => readable.decode(Uint8Array.of(3, 1, 1, 2)));
  failure('malformed', () => readable.decode(Uint8Array.of(4, 1, 2, 1, 0)));
  failure('incompatible', () => readable.decode(Uint8Array.of(0)));
  failure('malformed', () => readable.decode(Uint8Array.of(2, 2, 1)));
  failure('value', () => readable.encode({}));
  const limited = codec({ k: 'extension-record', fields: [], extensions: [] }, { nodes: 2 });
  failure('limit', () => limited.decode(Uint8Array.of(4, 1, 0, 2, 0)));

  const listLimited = codec({
    k: 'extension-record', fields: [],
    extensions: [{ id: 1, name: 'items', value: { k: 'list', item: unit }, required: true }],
  }, { nodes: 3 });
  failure('limit', () => listLimited.decode(Uint8Array.of(3, 1, 1, 2)));
});

test('validates open variant extents even when the tag is unfamiliar', () => {
  const open = codec(message({ k: 'open-variant', variants: [active, archive] }));
  assert.deepEqual(open.encode([null]), messageBytes(0));
  assert.deepEqual(open.encode([enabled]), messageBytes(2));
  assert.deepEqual(open.encode([archive42]), messageBytes(7, 1, 42));
  assert.deepEqual(open.decode(messageBytes(0x82, 0)), [enabled]);
  assert.deepEqual(open.decode(messageBytes(8)), [null]);
  assert.deepEqual(open.decode(messageBytes(9, 2, 1, 2)), [null]);
  for (const bytes of [[1], [7, 0], [7, 2, 42, 43], [3, 1, 0], [6], [9, 2, 1]]) {
    failure('malformed', () => open.decode(messageBytes(...bytes)));
  }
  const shallow = codec(message({ k: 'open-variant', variants: [] }), { depth: 2 });
  failure('limit', () => shallow.decode(messageBytes(9, 1, 0)));
});

test('preserves tuple neighbors, nested option layers and the enclosing result', () => {
  const option = item => ({ k: 'option', item });
  const schema = variants => message(
    option(option({ k: 'result', ok: u8, err: { k: 'open-variant', variants } })),
    u8,
  );
  const old = codec(schema([active]));
  const current = codec(schema([active, archive]));
  for (const [body, value] of [
    [[0, 99], [null, 99]],
    [[1, 0, 99], [{ some: null }, 99]],
    [[1, 1, 1, 0, 99], [{ some: { some: { err: null } } }, 99]],
    [[1, 1, 0, 64, 99], [{ some: { some: { ok: 64 } } }, 99]],
  ]) {
    assert.deepEqual(asPlain(old.decode(messageBytes(...body))), value);
    assert.deepEqual(current.encode(value), messageBytes(...body));
  }
  const bytes = messageBytes(1, 1, 1, 7, 1, 42, 99);
  assert.deepEqual(current.encode([{ some: { some: { err: archive42 } } }, 99]), bytes);
  assert.deepEqual(asPlain(old.decode(bytes)), [{ some: { some: { err: null } } }, 99]);
  failure('malformed', () => old.decode(messageBytes(1, 1, 1, 7, 2, 42, 99)));
});

test('accounts for every prefix byte at multibyte lengths', () => {
  const schema = {
    k: 'extension-record', fields: [],
    extensions: [{ id: 128, name: 'raw', value: { k: 'bytes' }, required: true }],
  };
  const value = { raw: new Uint8Array(128) };
  const exact = codec(schema, { bytes: 136, nodes: 3, depth: 2 });
  const bytes = exact.encode(value);
  assert.equal(bytes.length, 136);
  assert.deepEqual(bytes.subarray(0, 8), Uint8Array.of(0x86, 1, 0x80, 1, 0x82, 1, 0x80, 1));
  assert.deepEqual(exact.decode(bytes).raw, value.raw);
  failure('limit', () => codec(schema, { bytes: 135 }).encode(value));
  failure('limit', () => codec(schema, { nodes: 2 }).encode(value));
  failure('limit', () => codec(schema, { bytes: 135 }).decode(bytes));
});

test('discards only the nearest explicit recovery boundary', () => {
  const required = { k: 'open-variant', required: true, variants: [active] };
  const newer = { ...required, variants: [active, archive] };
  const tuple = (...items) => ({ k: 'tuple', items });
  const recover = item => ({ k: 'recover', item });
  const bool = { k: 'bool' };
  const old = codec(message(recover(tuple(required, bool)), bool));
  const writer = codec(message(recover(tuple(newer, bool)), bool));
  const body = [1, 4, 7, 1, 42, 1, 1];
  const bytes = messageBytes(...body);
  assert.deepEqual(writer.encode([{ some: [archive42, true] }, true]), bytes);
  assert.deepEqual(old.decode(bytes), [null, true]);
  assert.deepEqual(old.decode(messageBytes(1, 2, 2, 1, 1)), [{ some: [enabled, true] }, true]);
  const unitMessage = codec(message(recover(unit)));
  assert.deepEqual(unitMessage.encode([{ some: null }]), messageBytes(1, 0));
  assert.deepEqual(unitMessage.decode(messageBytes(1, 0)), [{ some: null }]);
  assert.deepEqual(unitMessage.decode(messageBytes(0)), [null]);

  const closest = codec(message(recover(tuple(recover(tuple(required, bool)), bool))));
  assert.deepEqual(closest.decode(messageBytes(1, body.length, ...body)), [{ some: [null, true] }]);
  failure('incompatible', () => codec(message(required, bool)).decode(messageBytes(...body.slice(2, 6))));
  failure('incompatible', () => codec(message({ k: 'option', item: required })).decode(messageBytes(1, 7, 1, 42)));
  failure('malformed', () => codec(message(recover(required))).decode(messageBytes(1, 1, 0)));
  failure('value', () => codec(message(required)).encode([null]));
});

test('checks known bytes, domains and resource limits after a recovery mark', () => {
  const required = { k: 'open-variant', required: true, variants: [active] };
  const bool = { k: 'bool' };
  const branch = { k: 'tuple', items: [required, bool] };
  const schema = message({ k: 'recover', item: branch }, bool);
  const older = codec(schema);
  const bytes = messageBytes(1, 4, 7, 1, 42, 1, 1);
  failure('malformed', () => older.decode(messageBytes(1, 4, 7, 1, 42, 2, 1)));
  failure('malformed', () => older.decode(messageBytes(1, 4, 7, 1, 42, 1, 2)));
  failure('malformed', () => older.decode(messageBytes(1, 5, 7, 1, 42, 1, 0, 1)));
  failure('malformed', () => older.decode(messageBytes(1, 4, 7, 3, 42, 1, 1)));
  failure('limit', () => codec(schema, { depth: 4 }).decode(bytes));
  failure('limit', () => codec(schema, { nodes: 6 }).decode(bytes));
  assert.deepEqual(codec(schema, { nodes: 7, depth: 5 }).decode(bytes), [null, true]);

  const checked = createPayloadCodec({
    schema: message({ k: 'recover', item: { k: 'tuple', items: [required, { ...bool, check: 'trueFlag' }] } }),
    bytes: 32,
    nodes: 8,
    depth: 5,
  }, {}, {
    trueFlag(value) { if (value !== true) throw new WireError('value'); },
  });
  failure('value', () => checked.decode(messageBytes(1, 4, 7, 1, 42, 0)));
});

test('recovers a missing required extension only inside a bounded option', () => {
  const fields = [{ name: 'id', value: u8 }];
  const entry = {
    k: 'extension-record', fields,
    extensions: [{ id: 8, name: 'epoch', value: u8, required: true }],
  };
  const bool = { k: 'bool' };
  const branch = { k: 'tuple', items: [entry, bool] };
  const schema = message({ k: 'recover', item: branch }, bool);
  const reader = codec(schema, { nodes: 8 });
  const priorBranch = { ...branch, items: [{ ...entry, extensions: [] }, bool] };
  const writer = codec(message({ k: 'recover', item: priorBranch }, bool));
  const priorBytes = messageBytes(1, 3, 7, 0, 1, 1);
  assert.deepEqual(writer.encode([{ some: [{ id: 7 }, true] }, true]), priorBytes);
  assert.deepEqual(reader.decode(priorBytes), [null, true]);
  assert.deepEqual(asPlain(codec(schema).decode(messageBytes(1, 6, 7, 3, 8, 1, 9, 1, 1))),
    [{ some: [{ id: 7, epoch: 9 }, true] }, true]);

  const unbounded = codec(message(entry, bool), { nodes: 5 });
  failure('incompatible', () => unbounded.decode(messageBytes(7, 0, 1)));
  failure('malformed', () => unbounded.decode(messageBytes(7, 0, 2)));
  failure('malformed', () => reader.decode(messageBytes(1, 3, 7, 0, 2, 1)));
  failure('malformed', () => reader.decode(messageBytes(1, 3, 7, 0, 1, 2)));
  failure('limit', () => codec(schema, { nodes: 7 }).decode(priorBytes));
});

test('checks a known sparse body after a missing required declaration', () => {
  const entry = {
    k: 'extension-record', fields: [], extensions: [
      { id: 1, name: 'epoch', value: u8, required: true },
      { id: 3, name: 'flag', value: { k: 'bool' }, required: true },
    ],
  };
  const valid = Uint8Array.of(3, 3, 1, 1);
  const malformed = Uint8Array.of(3, 3, 1, 2);
  const root = codec(entry, { nodes: 4 });
  failure('incompatible', () => root.decode(valid));
  failure('malformed', () => root.decode(malformed));
  failure('limit', () => codec(entry, { nodes: 3 }).decode(valid));

  const recovery = codec(message({ k: 'recover', item: entry }), { nodes: 6 });
  assert.deepEqual(recovery.decode(messageBytes(1, valid.length, ...valid)), [null]);
  failure('malformed', () => recovery.decode(messageBytes(1, malformed.length, ...malformed)));
});

test('matches Rust recovery when a known nested extension contains a bad boolean', () => {
  const detail = {
    k: 'extension-record', fields: [],
    extensions: [{ id: 1, name: 'active', value: { k: 'bool' }, required: false }],
  };
  const branch = {
    k: 'extension-record', fields: [],
    extensions: [
      { id: 1, name: 'generation', value: u8, required: true },
      { id: 2, name: 'detail', value: detail, required: false },
    ],
  };
  const reader = codec(message({ k: 'recover', item: branch }, u8));

  const missing = messageBytes(1, 7, 6, 2, 4, 3, 1, 1, 1, 23);
  assert.deepEqual(reader.decode(missing), [null, 23]);
  const malformed = missing.slice();
  malformed[8] = 2;
  failure('malformed', () => reader.decode(malformed));

  const complete = messageBytes(1, 10, 9, 1, 1, 7, 2, 4, 3, 1, 1, 1, 23);
  assert.deepEqual(asPlain(reader.decode(complete)), [
    { some: { generation: 7, detail: { some: { active: { some: true } } } } },
    23,
  ]);
});

test('charges every declared sparse slot even when a list omits its entries', () => {
  const absent = [null, null, null];
  const mixed = [null, { some: null }, null];
  const extensions = ['first', 'second', 'third'].map((name, index) => ({
    id: index + 1, name, value: unit, required: false,
  }));
  const record = values => Object.fromEntries(extensions.map(({ name }, index) => [name, values[index]]));
  for (const [item, valueOf] of [
    [{ k: 'extension-record', fields: [], extensions }, record],
    [{ k: 'extension-tuple', items: [], extensions }, values => values],
  ]) {
    const schema = message({ k: 'list', item });
    for (const [row, bytes, nodes] of [
      [absent, messageBytes(2, 0, 0), 10],
      [mixed, messageBytes(2, 2, 2, 0, 0), 11],
    ]) {
      const value = [[valueOf(row), valueOf(absent)]];
      const exact = codec(schema, { nodes, depth: 4 });
      assert.deepEqual(exact.encode(value), bytes);
      assert.deepEqual(asPlain(exact.decode(bytes)), value);
      for (const limits of [{ nodes: nodes - 1 }, { nodes, depth: 3 }]) {
        const limited = codec(schema, limits);
        failure('limit', () => limited.encode(value));
        failure('limit', () => limited.decode(bytes));
      }
    }
  }
});

test('appends optional tuple slots inside a list without consuming its sibling', () => {
  const bool = { k: 'bool' };
  const schema = extensions => message(
    { k: 'list', item: { k: 'extension-tuple', items: [u8], extensions } },
    bool,
  );
  const prior = codec(schema([]));
  const currentSchema = schema([
    { id: 2, name: 'description', value: { k: 'text' }, required: false },
    { id: 3, name: 'seen', value: unit, required: false },
  ]);
  const current = codec(currentSchema);
  const priorValue = [[[7], [9]], true];
  const currentValue = [[[7, { some: 'x' }, { some: null }], [9, null, { some: null }]], true];
  const priorBytes = messageBytes(2, 7, 0, 9, 0, 1);
  const currentBytes = messageBytes(2, 7, 6, 2, 2, 1, 0x78, 3, 0, 9, 2, 3, 0, 1);
  assert.deepEqual(prior.encode(priorValue), priorBytes);
  assert.deepEqual(current.encode(currentValue), currentBytes);
  assert.deepEqual(prior.decode(currentBytes), priorValue);
  assert.deepEqual(current.decode(currentBytes), currentValue);
  assert.deepEqual(current.decode(priorBytes), [[[7, null, null], [9, null, null]], true]);
  failure('value', () => current.encode([[[7, null]], true]));
  failure('value', () => current.encode([[[7, , null]], true]));
  failure('limit', () => codec(currentSchema, { bytes: currentBytes.length - 1 }).encode(currentValue));

  const excess = currentBytes.slice();
  excess[8] = 2;
  for (const reader of [prior, current]) {
    failure('malformed', () => reader.decode(excess));
    failure('malformed', () => reader.decode(currentBytes.subarray(0, -1)));
  }
});
