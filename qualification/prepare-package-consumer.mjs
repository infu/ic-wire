// Scratch copy of the real consumer, selecting only extracted crate archives.
import { cpSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const [output, version] = process.argv.slice(2);
const quote = JSON.stringify;
cpSync('qualification/ic-wire-consumer', join(output, 'ic-wire-consumer'), { recursive: true });
let workspace = readFileSync('qualification/Cargo.toml', 'utf8')
  .replace(/^members = .*$/m, 'members = ["ic-wire-consumer"]')
  .replace('path = "../crates/ic-wire"', `path = ${quote(join(output, 'ic-wire-' + version))}`);
workspace = workspace.replace(/^(candid|proptest|libfuzzer-sys|ic-wire-consumer|serde_bytes|serde_json|stats_alloc|ic-testkit|sha2|tokio) = .*\n/gm, '');
// Extracted archives are standalone packages, not new workspace members.
workspace = workspace.replace('[workspace]\n',
  `[workspace]\nexclude = ["ic-wire-${version}", "ic-wire-derive-${version}"]\n`);
writeFileSync(join(output, 'Cargo.toml'), workspace);
cpSync('qualification/Cargo.lock', join(output, 'Cargo.lock'));
writeFileSync(join(output, 'patch.toml'),
  `[patch.crates-io]\nic-wire-derive = { path = ${quote(join(output, 'ic-wire-derive-' + version))} }\n`);
for (const name of ['ic-wire', 'ic-wire-derive']) {
  const manifest = join(output, `${name}-${version}`, 'Cargo.toml');
  // Isolate unpacked manifests from the enclosing source checkout. Dependency
  // declarations and shipped source files remain unchanged.
  writeFileSync(manifest, readFileSync(manifest, 'utf8') + '\n[workspace]\n');
}
