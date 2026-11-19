import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const tsup = path.join(path.dirname(require.resolve('tsup/package.json')), 'dist', 'cli-default.js');
const tsc = require.resolve('typescript/bin/tsc');
function run(script, args, cwd = root) {
  execFileSync(process.execPath, [script, ...args], { cwd, windowsHide: true, stdio: 'inherit' });
}
for (const directory of ['kernel', 'pi-subagents']) {
  run(tsup, [], path.join(root, directory));
  run(tsc, ['--emitDeclarationOnly'], path.join(root, directory));
}
run(tsup, []);
const provenance = JSON.parse(readFileSync(path.join(root, 'provenance.json'), 'utf8'));
writeFileSync(path.join(root, 'dist', 'build-info.json'), JSON.stringify({
  commit: `${provenance.upstreamCommit.slice(0, 12)}+musashi-terminal`,
  sourceFingerprint: process.env.MUSASHI_TERMINAL_BILI_SOURCE_FINGERPRINT || 'development',
}, null, 2) + '\n');
run(path.join(root, 'scripts', 'check-dist-import-annotations.mjs'), []);
