import fs from 'node:fs';
import crypto from 'node:crypto';
import path from 'node:path';
if (!process.argv[2]) throw Error('Usage: node summarize.mjs EXTRACTED_STUDY_DIRECTORY');
const root = path.resolve(process.argv[2]);
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
const read = p => fs.readFileSync(path.join(root, p));
const rows = p => read(`results/${p}/samples.jsonl`).toString().trim().split('\n').map(JSON.parse);
const csv = ['profile,case,count,operation,old_implementation,upstream_head,fixed,head_vs_old_percent,fixed_vs_head_percent,fixed_vs_old_percent,bytes,old_capacity,head_capacity,fixed_capacity'];
const artifacts = [];
const changes = [];
for (const profile of ['benchmark-speed', 'evaluation']) {
  const before = rows(profile);
  const after = rows(`fix-${profile}`);
  if (before.length !== 320 || after.length !== 320) throw Error('incomplete run');
  const groups = new Map();
  for (const [data, labels] of [[before, {baseline:'old', candidate:'head'}], [after, {baseline:'repeat-head', candidate:'fixed'}]]) {
    for (const row of data) {
      const key = `${row.case}/${row.count}`;
      if (!groups.has(key)) groups.set(key, {});
      const g = groups.get(key);
      (g[labels[row.variant]] ??= []).push(row);
    }
  }
  for (const [key, group] of groups) {
    const all = Object.values(group).flat();
    if (new Set(all.map(r => `${r.payload_sha256}/${r.bytes}/${r.repetitions}`)).size !== 1) throw Error(`unequal work: ${profile}/${key}`);
    for (const list of Object.values(group)) {
      if (list.length !== 8 || new Set(list.map(r=>JSON.stringify(r.counters))).size !== 1) throw Error(`counter drift: ${profile}/${key}`);
    }
    if (JSON.stringify(group.head[0].counters) !== JSON.stringify(group['repeat-head'][0].counters)) throw Error(`head repeat mismatch: ${key}`);
    for (const [i, operation] of ['fresh', 'reused', 'slice', 'scoped'].entries()) {
      const op = name => group[name][0].counters[i] / group[name][0].repetitions;
      const old = op('old'), head = op('head'), fixed = op('fixed');
      const pct = (a,b) => 100*(b-a)/a;
      const row = {profile, case:all[0].case, count:all[0].count, operation, old, head, fixed, head_vs_old:pct(old,head), fixed_vs_head:pct(head,fixed), fixed_vs_old:pct(old,fixed)};
      changes.push(row);
      csv.push([profile,row.case,row.count,operation,old,head,fixed,row.head_vs_old,row.fixed_vs_head,row.fixed_vs_old,all[0].bytes,...['old','head','fixed'].map(n=>group[n][0].wasm_capacity_bytes)].join(','));
    }
  }
  for (const variant of ['old','head','fixed']) {
    const b = read(`artifacts/${variant}-${profile}.wasm`);
    let offset = 8, code;
    const uleb = () => { let value=0, shift=0, c; do { c=b[offset++]; value+=(c&127)*2**shift; shift+=7; } while(c&128); return value; };
    while (offset < b.length) { const id=b[offset++], length=uleb(); if (id===10) code=b.subarray(offset,offset+length); offset+=length; }
    if (!code || offset !== b.length) throw Error('invalid wasm');
    artifacts.push({profile,variant,total_bytes:b.length,code_section_bytes:code.length,sha256:hash(b),code_sha256:hash(code)});
  }
}
fs.mkdirSync(path.join(root,'analysis'),{recursive:true});
fs.writeFileSync(path.join(root,'analysis/instructions.csv'),csv.join('\n')+'\n');
fs.writeFileSync(path.join(root,'analysis/artifacts.json'),JSON.stringify(artifacts,null,2)+'\n');
fs.writeFileSync(path.join(root,'analysis/summary.json'),JSON.stringify({counter_pairs:changes.length,improved:changes.filter(r=>r.fixed<r.head).length,equal:changes.filter(r=>r.fixed===r.head).length,worse:changes.filter(r=>r.fixed>r.head).length,regressions:changes.filter(r=>r.fixed>r.head),largest_residuals:changes.filter(r=>r.fixed>r.old).sort((a,b)=>b.fixed_vs_old-a.fixed_vs_old).slice(0,10)},null,2)+'\n');
console.log(JSON.stringify({selected:changes.filter(r=>(r.case==='u8'&&r.count===262144||r.case==='records'&&r.count===256||r.case==='bytes'&&r.count===262144||r.case==='unit')&&(r.operation==='scoped'||r.operation==='fresh')),artifacts,regressions:changes.filter(r=>r.fixed>r.head)},null,2));
