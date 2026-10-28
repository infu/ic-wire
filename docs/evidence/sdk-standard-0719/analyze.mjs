// Compare the unmodified maintained benchmark matrix with one matched baseline.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import {gzipSync} from 'node:zlib';

const [root, output] = process.argv.slice(2);
assert(root && output, 'usage: node analyze.mjs RUN_ROOT NEW_OUTPUT');
fs.mkdirSync(output);
fs.mkdirSync(path.join(output, 'raw'));
const read = p => fs.readFileSync(p, 'utf8');
const json = p => JSON.parse(read(p));
const jsonl = p => read(p).trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const requireHash = (value, label) => assert(typeof value === 'string' && /^[a-f0-9]{64}$/.test(value), label);
const key = r => [r.case, r.codec, r.operation].join('|');
const index = rows => {
  const result = new Map(rows.map(r => [key(r), r]));
  assert.equal(result.size, rows.length, 'duplicate operation key');
  return result;
};
const percent = (before, after) => before === 0 ? (after === 0 ? 0 : null) : 100 * (after / before - 1);
const csv = (name, rows) => {
  assert(rows.length, `empty output ${name}`);
  const columns = [...new Set(rows.flatMap(r => Object.keys(r)))];
  const cell = value => {
    const text = value === null || value === undefined ? '' : String(value);
    return /[",\n\r]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
  };
  fs.writeFileSync(path.join(output, name), [columns.join(','), ...rows.map(r => columns.map(c => cell(r[c])).join(','))].join('\n') + '\n');
};
const profiles = ['evaluation', 'benchmark-speed'];
const modes = ['native-1', 'native-2', 'allocations', 'heap'];
const sourceIdentity = json(path.join(root, 'receipts/identity.json'));
fs.mkdirSync(path.join(output, 'source-receipts'));
for (const variant of ['baseline', 'candidate']) {
  const manifestFile = path.join(root, 'receipts', `${variant}-source.json`);
  assert.equal(hash(fs.readFileSync(manifestFile)), sourceIdentity.variants[variant].source_manifest_sha256);
  const manifest = json(manifestFile);
  const snapshot = path.join(root, `results-${variant}`, 'source');
  assert.equal(read(path.join(root, `results-${variant}`, 'source-head.txt')).trim(), sourceIdentity.variants[variant].head);
  for (const [name, expected] of Object.entries(manifest)) {
    if (name.startsWith('crates/') || ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
      'scripts/ci/evaluate-codecs.sh', 'scripts/ci/evaluate-codec-matrix.sh', 'scripts/ci/codec-cohorts.psv'].includes(name)) {
      assert.equal(hash(fs.readFileSync(path.join(snapshot, name))), expected, `frozen source ${variant}/${name}`);
    }
  }
  for (const suffix of ['source.json', 'status.txt']) fs.copyFileSync(path.join(root, 'receipts', `${variant}-${suffix}`), path.join(output, 'source-receipts', `${variant}-${suffix}`));
  fs.copyFileSync(path.join(root, 'receipts', `${variant}.patch`), path.join(output, 'source-receipts', `${variant}.patch`));
}
const fields = ['fixture_sha256', 'hotspots_fixture_sha256', 'bulk_fixture_sha256',
  'packed_i16_fixture_sha256', 'toko_fixture_sha256', 'derive_fixture_sha256', 'lock_sha256'];
const cohorts = read(path.join(root, 'results-candidate/cohorts.psv')).trim().split('\n').map(line => line.split('|'));
assert.equal(cohorts.length, 22, '21 maintained cohorts plus hotspots required');
assert.equal(new Set(cohorts.map(([name]) => name)).size, cohorts.length);
assert.equal(read(path.join(root, 'results-baseline/cohorts.psv')), read(path.join(root, 'results-candidate/cohorts.psv')));

const instructions = [], native = [], allocations = [], heap = [], artifacts = [], wins = [], rawIndex = [];
let server, lock;
let caseCount = 0;
for (const [cohort, features, primary] of cohorts) {
  const pair = {};
  for (const variant of ['baseline', 'candidate']) {
    const directory = path.join(root, `results-${variant}`, cohort);
    const identity = json(path.join(directory, 'instructions/identity.json'));
    for (const field of [...fields, 'server_sha256', 'driver_sha256', 'wasm_driver_sha256']) requireHash(identity[field], `${cohort}/${variant}/${field}`);
    assert.equal(identity.samples, 3);
    assert.equal(identity.small_repetitions, 4);
    assert.equal(identity.large_repetitions, 1);
    server ??= identity.server_sha256;
    lock ??= identity.lock_sha256;
    assert.equal(identity.server_sha256, server);
    assert.equal(identity.lock_sha256, lock);
    const rows = jsonl(path.join(directory, 'instructions/samples.jsonl'));
    const ic = new Map();
    for (const profile of profiles) ic.set(profile, index(rows.filter(r => r.profile === profile)));
    assert.equal(rows.length, [...ic.values()].reduce((n, map) => n + map.size, 0));
    for (const row of rows) {
      requireHash(row.payload_sha256, 'IC payload hash');
      requireHash(row.input_sha256, 'IC input hash');
      assert.equal(row.samples_instructions.length, 3);
      assert(row.samples_instructions.every(n => Number.isSafeInteger(n) && n > 0));
      assert.equal(row.repetitions, row.encoded_bytes > 65536 ? 1 : 4);
      const sorted = row.samples_instructions.toSorted((a, b) => a - b);
      assert.equal(row.median_instructions_per_op, sorted[1] / row.repetitions);
    }
    const nativeRows = {}, nativeIdentities = {}, nativeIndexes = {};
    for (const profile of profiles) for (const mode of modes) {
      const name = `${profile}-${mode}`;
      const id = nativeIdentities[name] = json(path.join(directory, name, 'identity.json'));
      assert.equal(id.profile, profile);
      assert.equal(id.git_head.trim(), sourceIdentity.variants[variant].head);
      assert.equal(id.mode, mode.startsWith('native-') ? 'native-time' : mode === 'heap' ? 'peak-heap' : 'allocation-counts');
      for (const field of [...fields, 'runner_sha256', 'executable_sha256']) requireHash(id[field], `native ${cohort}/${variant}/${field}`);
      assert(Object.hasOwn(id, 'rustflags'));
      assert(typeof id.compiler === 'string' && id.compiler.startsWith('rustc 1.99.1 '));
      assert(typeof id.host === 'string' && id.host.length > 0);
      for (const policy of ['wire_limits', 'candid_limits']) assert(id[policy] && Object.values(id[policy]).every(n => Number.isSafeInteger(n) && n > 0));
      assert.equal(id.samples, 11);
      assert.equal(id.minimum_batch_ms, 2);
      for (const field of fields) assert.equal(id[field], identity[field], `native/IC ${cohort}/${variant}/${field}`);
      const values = nativeRows[name] = jsonl(path.join(directory, name, 'samples.jsonl'));
      nativeIndexes[name] = index(values);
      const expected = [...ic.get(profile).values()].filter(r => r.operation !== 'loop-control');
      assert.deepEqual([...nativeIndexes[name].keys()].sort(), expected.map(key).sort(), `complete native operation coverage ${cohort}/${name}`);
      assert.equal(id.case_count, new Set(values.map(r => r.case)).size);
      assert.equal(id.case_count, new Set(expected.map(r => r.case)).size);
      for (const row of values) {
        const counterpart = ic.get(profile).get(key(row));
        assert(counterpart, `native/IC pair ${cohort}/${name}/${key(row)}`);
        assert.equal(row.input_sha256, counterpart.input_sha256);
        assert.equal(row.encoded_sha256, counterpart.payload_sha256);
        assert.equal(row.encoded_bytes, counterpart.encoded_bytes);
        if (mode.startsWith('native-')) {
          assert.equal(row.measurement.samples_ns_per_op.length, 11);
          assert(row.measurement.samples_ns_per_op.every(n => Number.isFinite(n) && n > 0));
          assert(Number.isSafeInteger(row.measurement.repetitions) && row.measurement.repetitions >= 1 && row.measurement.repetitions <= 1048576);
          const sorted = row.measurement.samples_ns_per_op.toSorted((a, b) => a - b);
          assert.equal(row.measurement.median_ns, sorted[5]);
          assert.equal(row.measurement.min_ns, sorted[0]);
          assert.equal(row.measurement.max_ns, sorted[10]);
        } else {
          for (const [metric, value] of Object.entries(row.measurement)) {
            assert(Number.isSafeInteger(value), `integer allocation/heap metric ${metric}`);
            assert(value >= 0 || metric === 'net_reallocated_bytes', `nonnegative allocation/heap metric ${metric}`);
          }
        }
      }
    }
    const sizes = read(path.join(directory, 'wasm.csv')).trim().split('\n').slice(1).map(line => {
      const [profile, codec, bytes, sha256, code] = line.split(',');
      const wasm = fs.readFileSync(path.join(directory, `${profile}-${codec}.wasm`));
      assert.equal(wasm.length, +bytes);
      assert(Number.isSafeInteger(+code) && +code > 0 && +code <= wasm.length);
      assert.equal(hash(wasm), sha256);
      assert.equal(identity.artifacts.find(r => r.profile === profile && r.codec === codec)?.sha256, sha256);
      return {profile, codec, total_bytes: +bytes, code_bytes: +code, sha256};
    });
    assert.equal(sizes.length, 4);
    assert.equal(new Set(sizes.map(r => `${r.profile}|${r.codec}`)).size, 4);
    pair[variant] = {identity, rows, ic, nativeRows, nativeIdentities, nativeIndexes, sizes};
    const raw = {cohort, features, primary, identity, instructions: rows, nativeRows, nativeIdentities, sizes};
    const bytes = Buffer.from(JSON.stringify(raw) + '\n');
    const file = `raw/${variant}-${cohort}.json.gz`;
    fs.writeFileSync(path.join(output, file), gzipSync(bytes));
    rawIndex.push({variant, cohort, file, decompressed_sha256: hash(bytes)});
  }
  const before = pair.baseline, after = pair.candidate;
  for (const field of [...fields, 'driver_sha256', 'wasm_driver_sha256']) {
    assert.equal(before.identity[field], after.identity[field], `paired identity ${cohort}/${field}`);
  }
  assert.equal(before.rows.length, after.rows.length);
  caseCount += new Set(after.rows.map(r => r.case)).size;
  for (const profile of profiles) {
    for (const [k, a] of after.ic.get(profile)) {
      const b = before.ic.get(profile).get(k);
      assert(b, `missing baseline ${cohort}/${profile}/${k}`);
      for (const field of ['case_id', 'input_sha256', 'payload_sha256', 'encoded_bytes', 'repetitions']) assert.equal(b[field], a[field]);
      instructions.push({cohort, profile, case: a.case, codec: a.codec, operation: a.operation,
        bytes: a.encoded_bytes, before: b.median_instructions_per_op, after: a.median_instructions_per_op,
        percent: percent(b.median_instructions_per_op, a.median_instructions_per_op),
        baseline_return_variants: new Set(b.samples_instructions).size,
        candidate_return_variants: new Set(a.samples_instructions).size});
    }
    for (const mode of modes) {
      const name = `${profile}-${mode}`;
      assert.equal(before.nativeIndexes[name].size, after.nativeIndexes[name].size);
      assert.equal(before.nativeIdentities[name].compiler, after.nativeIdentities[name].compiler);
      for (const field of ['runner_sha256', 'host', 'rustflags']) assert.equal(before.nativeIdentities[name][field], after.nativeIdentities[name][field]);
      assert.equal(after.nativeIdentities[name].runner_sha256, after.nativeIdentities['evaluation-native-1'].runner_sha256);
      assert.deepEqual(before.nativeIdentities[name].wire_limits, after.nativeIdentities[name].wire_limits);
      assert.deepEqual(before.nativeIdentities[name].candid_limits, after.nativeIdentities[name].candid_limits);
      for (const [k, a] of after.nativeIndexes[name]) {
        const b = before.nativeIndexes[name].get(k);
        assert(b, `missing native baseline ${cohort}/${name}/${k}`);
        for (const field of ['input_sha256', 'encoded_sha256', 'encoded_bytes']) assert.equal(b[field], a[field]);
        const base = {cohort, profile, case: a.case, codec: a.codec, operation: a.operation};
        if (mode.startsWith('native-')) {
          native.push({...base, run: mode, before_ns: b.measurement.median_ns, after_ns: a.measurement.median_ns,
            percent: percent(b.measurement.median_ns, a.measurement.median_ns),
            before_fixture_fresh_output_capacity: b.fresh_output_capacity, after_fixture_fresh_output_capacity: a.fresh_output_capacity,
            before_min_ns: b.measurement.min_ns, before_max_ns: b.measurement.max_ns,
            after_min_ns: a.measurement.min_ns, after_max_ns: a.measurement.max_ns});
        } else {
          assert.deepEqual(Object.keys(b.measurement).sort(), Object.keys(a.measurement).sort());
          for (const field of Object.keys(a.measurement)) {
            (mode === 'heap' ? heap : allocations).push({...base, metric: field,
              before: b.measurement[field], after: a.measurement[field], delta: a.measurement[field] - b.measurement[field]});
          }
        }
      }
    }
    for (const operation of ['encode-fresh', 'decode-owned-or-labelled-borrowed']) {
      const tally = {cohort, profile, operation};
      for (const variant of ['baseline', 'candidate']) {
        const rows = [...pair[variant].ic.get(profile).values()].filter(r => r.codec === primary && r.operation === operation);
        assert(rows.length > 0, `primary coverage ${cohort}/${profile}/${variant}/${operation}`);
        tally.cases = rows.length;
        const paired = rows.map(r => {
          const candid = pair[variant].ic.get(profile).get(key({...r, codec: 'candid'}));
          assert(candid, `missing Candid ${cohort}/${key(r)}`);
          assert.equal(candid.input_sha256, r.input_sha256);
          return [r.median_instructions_per_op, candid.median_instructions_per_op];
        });
        tally[`${variant}_wire_wins`] = paired.filter(([w, c]) => w < c).length;
        tally[`${variant}_ties`] = paired.filter(([w, c]) => w === c).length;
        tally[`${variant}_candid_wins`] = paired.filter(([w, c]) => w > c).length;
      }
      wins.push(tally);
    }
    for (const codec of ['wire', 'candid']) {
      const b = before.sizes.find(r => r.profile === profile && r.codec === codec);
      const a = after.sizes.find(r => r.profile === profile && r.codec === codec);
      artifacts.push({cohort, profile, codec, before_bytes: b.total_bytes, after_bytes: a.total_bytes,
        before_code: b.code_bytes, after_code: a.code_bytes, identical: b.sha256 === a.sha256,
        before_sha256: b.sha256, after_sha256: a.sha256});
    }
  }
  console.log(`Validated ${cohort}: ${after.rows.length} IC rows, all native/allocation/heap pairs`);
}

csv('instruction-changes.csv', instructions);
csv('native-changes.csv', native);
csv('allocation-changes.csv', allocations);
csv('heap-changes.csv', heap);
csv('artifacts.csv', artifacts);
csv('wire-candid-wins.csv', wins);
fs.writeFileSync(path.join(output, 'raw/index.json'), JSON.stringify(rawIndex, null, 2) + '\n');
const counts = rows => ({rows: rows.length, increases: rows.filter(r => r.after > r.before).length,
  ties: rows.filter(r => r.after === r.before).length, decreases: rows.filter(r => r.after < r.before).length});
const wire = instructions.filter(r => r.codec.startsWith('wire-'));
const probe = json(path.join(root, 'receipts/cache-probe/result.json'));
assert.equal(probe.status, 'PASS');
assert.equal(probe.serverSha256, server);
assert.equal(probe.identicalQueryReturnCount, 3);
assert.equal(probe.identicalQueryObservedExecutions, 1);
assert.equal(probe.distinctSenderObservedExecutions, 1);
assert(probe.returns.every(r => r.sha256 === probe.returns[0].sha256));
fs.writeFileSync(path.join(output, 'query-reuse-probe.json'), JSON.stringify(probe, null, 2) + '\n');
fs.cpSync(path.join(root, 'receipts/cache-probe/traces'), path.join(output, 'query-reuse-traces'), {recursive: true});
fs.copyFileSync(path.join(root, 'receipts/cache-probe/session/receipt.json'), path.join(output, 'query-reuse-server-receipt.json'));
const uniqueQueries = new Set(instructions.map(r => [r.cohort, r.profile, r.case, r.codec].join('|'))).size;
const summary = {cohorts: cohorts.length, cases: caseCount, lock_sha256: lock, server_sha256: server,
  source_identity: sourceIdentity,
  query_inputs_per_variant: uniqueQueries, returned_queries_per_variant: uniqueQueries * 3,
  instructions: counts(instructions), wire_instructions: counts(wire),
  candid_instructions: counts(instructions.filter(r => r.codec === 'candid')),
  allocation_metrics: counts(allocations), heap_metrics: counts(heap), native_rows: native.length,
  artifacts: {total: artifacts.length, identical: artifacts.filter(r => r.identical).length,
    code_increases: artifacts.filter(r => r.after_code > r.before_code).length,
    total_increases: artifacts.filter(r => r.after_bytes > r.before_bytes).length},
  wire_regressions: wire.filter(r => r.after > r.before).toSorted((a, b) => b.percent - a.percent).slice(0, 20),
  limitation: 'The maintained driver makes three identical queries. The exact-binary unit probe observed one Wasm execution for those three returns, so three independent executions per case are not claimed. Native timings are serial on a shared host; do not treat row medians as endpoint latency.'};
fs.writeFileSync(path.join(output, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary, null, 2));
