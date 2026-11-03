// SOURCE-RUNTIME proof, not a replacement entry point or a running-CLI attestation.
// The sole canonical prepared tree and its provenance lock remain unchanged.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const [prepared, privateRoot, dependencies] = process.argv.slice(2);
for (const root of [prepared, privateRoot, dependencies]) assert.ok(path.isAbsolute(root));
const workspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const pin = JSON.parse(fs.readFileSync(path.join(workspace, 'sidecar/source-build/provenance.lock.json')));
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const send = value => process.stdout.write(JSON.stringify(value) + '\n');
const configuration = path.join(privateRoot, 'config/billion-context.json');
assert.equal(path.resolve(process.env.BILI_CONFIG_FILE ?? configuration), configuration);
assert.equal(path.resolve(process.env.MUSASHI_TERMINAL_BILI_ROOT ?? process.env.BILI_ROOT ?? ''), privateRoot);
const configBytes = fs.readFileSync(configuration);
assert.ok(configBytes.length <= 16 * 1024);
const originalCliEntry = path.join(prepared, pin.entry);
assert.equal(sha256(fs.readFileSync(originalCliEntry)), pin.entrySha256);
const parentEnvironment = { ...process.env };
const sourceFiles = [
  'src/config.ts', 'src/paths.ts', 'src/compress-settings.ts', 'src/output-steering.ts',
  'src/musashi-terminal/adaptive-turn.ts', 'src/musashi-terminal/adaptive-pressure.ts',
  'src/musashi-terminal/prompt-rewrite.ts', 'src/musashi-terminal/rewrite-settings.ts',
  'kernel/src/config.ts', 'kernel/src/output-steering.ts',
];
const sourceEvidence = sourceFiles.map(relative => {
  const expected = pin.sourceFiles.find(entry => entry.path === relative);
  assert.ok(expected, 'source absent from canonical preparation pin');
  const bytes = fs.readFileSync(path.join(prepared, relative));
  assert.equal(bytes.length, expected.bytes);
  assert.equal(sha256(bytes), expected.sha256);
  return { path: relative, sha256: expected.sha256 };
});
const watchdog = setTimeout(() => process.exit(90), 27_000);
watchdog.unref();
const requireLocal = createRequire(path.join(dependencies, 'esbuild/package.json'));
const esbuild = requireLocal('esbuild');
const artifact = path.join(privateRoot, 'temp/effective-config-runtime.mjs');
fs.mkdirSync(path.dirname(artifact), { recursive: true });
let stage = 'canonical-source-build';
try {
  await esbuild.build({
    stdin: { contents: [
      'export { loadOptions, passthroughState } from "./src/config.ts";',
      'export { configFile } from "./src/paths.ts";',
      'export { resolveCompress, resolveRequestConfig } from "./src/compress-settings.ts";',
      'export { parseOutputSteering, applyOutputSteeringJson } from "./src/output-steering.ts";',
      'export { processAdaptiveTurn } from "./src/musashi-terminal/adaptive-turn.ts";',
      'export { adaptivePressureThreshold } from "./src/musashi-terminal/adaptive-pressure.ts";',
      'export { createPromptRewriter } from "./src/musashi-terminal/prompt-rewrite.ts";',
      'export { createCore, createInitialState } from "./kernel/src/compress.ts";',
    ].join('\n'), resolveDir: prepared, loader: 'ts' },
    outfile: artifact, bundle: true, platform: 'node', format: 'esm', target: 'node22',
    mainFields: ['module', 'main'], nodePaths: [dependencies], external: ['jimp', 'mysql2'],
    alias: {
      'acp-kernel': path.join(prepared, 'kernel/src/index.ts'),
      'acp-kernel/persist': path.join(prepared, 'kernel/src/persist/index.ts'),
      'acp-kernel/wire': path.join(prepared, 'kernel/src/wire/index.ts'),
    },
    banner: { js: 'import {createRequire as fixtureRequire} from "node:module";const require=fixtureRequire(import.meta.url);' },
    logLevel: 'silent',
  });
  send({ phase: 'built', artifactSha256: sha256(fs.readFileSync(artifact)),
    sourceTreeSha256: pin.sourceTreeSha256, artifactTreeSha256: pin.artifactTreeSha256,
    esbuildVersion: esbuild.version, sourceEvidence });
  // The Rust owner must verify esbuild's real job membership before releasing it.
  const permission = await new Promise(resolve => {
    let input = '';
    const onInput = chunk => {
      input += chunk.toString();
      assert.ok(input.length <= 64);
      if (input.includes('\n')) {
        process.stdin.off('data', onInput);
        process.stdin.pause();
        resolve(input.trim());
      }
    };
    process.stdin.on('data', onInput);
  });
  assert.equal(permission, 'census-complete');
  esbuild.stop();
  stage = 'canonical-source-runtime';
  const source = await import(pathToFileURL(artifact).href);
  assert.equal(path.resolve(source.configFile()), configuration);
  const options = source.loadOptions();
  const resolved = source.resolveRequestConfig(options.kernelConfig, options.routes,
    'http://127.0.0.1:1/synthetic', 'synthetic-local-model', 1_000_000, options.compress, options.namedProviders);
  assert.equal(resolved.tiers.enabled, true);
  assert.equal(resolved.adaptivePressure.enabled, true);
  assert.equal(resolved.adaptivePressure.maxTokens, 160_000);
  assert.equal(options.promptRewrite.mode, 'auto');
  const steering = source.parseOutputSteering(source.resolveCompress(options.routes,
    'http://127.0.0.1:1/synthetic', undefined, options.compress).outputSteering);
  assert.deepEqual(steering, { enabled: true, verbosityLevel: 2, effortRouting: false });

  // Actual adapter wire mutation, on a mechanical continuation that WOULD lower
  // effort when enabled. No client/provider, auth headers or model is contacted.
  const body = {
    model: 'synthetic-local-model', max_tokens: 1024, system: 'synthetic-prefix',
    output_config: { effort: 'high' },
    messages: [{ role: 'user', content: [{ type: 'tool_result', tool_use_id: 'synthetic-tool', content: 'ok' }] }],
  };
  const labels = source.applyOutputSteeringJson(body, 'anthropic', steering);
  assert.equal(body.output_config.effort, 'high');
  assert.ok(body.system.startsWith('synthetic-prefix'));
  assert.ok(body.system.includes('<bili_output_steering>'));
  assert.ok(labels.includes('steering:L2'));
  assert.ok(!labels.includes('effort:low'));
  const enabledEffort = structuredClone(body);
  source.applyOutputSteeringJson(enabledEffort, 'anthropic', { ...steering, effortRouting: true });
  assert.equal(enabledEffort.output_config.effort, 'low');
  const disabledBody = structuredClone(body);
  const disabledBefore = JSON.stringify(disabledBody);
  assert.deepEqual(source.applyOutputSteeringJson(disabledBody, 'anthropic', { ...steering, enabled: false }), []);
  assert.equal(JSON.stringify(disabledBody), disabledBefore);

  // Use the same canonical adaptive path as prepare-{anthropic,openai,...}.
  // Empty messages avoid any paid compression/summarization or large fixture.
  const turn = enabled => {
    const state = source.createInitialState();
    const session = { id: 'synthetic-adaptive', state, metadata: { lastWindowSource: 'provider' } };
    return source.processAdaptiveTurn(source.createCore(), { messages: [], state, config: {
      ...resolved, adaptivePressure: { ...resolved.adaptivePressure, enabled },
    }, tokenCount: 0, renderTags: 'none' }, { session, enabled: true, nativeWindow: 1_000_000,
      protocol: 'anthropic', body: { model: 'synthetic-local-model', max_tokens: 1024 }, injectsCompress: false });
  };
  assert.ok(turn(true).epoch);
  assert.equal(turn(false).epoch, undefined);
  const threshold = source.adaptivePressureThreshold({ contextTokens: 1_000_000,
    outputTokens: 1024, ratio: resolved.nudge.maxContextLimitPct, maxTokens: resolved.adaptivePressure.maxTokens });
  assert.ok(threshold > 0 && threshold < 160_000);
  assert.equal(resolved.modelContextLimit, 1_000_000, 'adaptive policy must retain the real model window');

  // Actual rewrite admission with only injected, deterministic local functions.
  // Off must not consult even the synthetic catalog; auto must dispatch once.
  let catalogs = 0; let completions = 0; let persisted = 0;
  const scope = { route: 'http://127.0.0.1:1/synthetic', protocol: 'anthropic', authScope: 'synthetic-no-credentials' };
  const draft = 'Please tidy this draft request for the demonstration without changing its meaning or adding new tasks because the current wording repeats several ideas and could be easier to read.';
  const editor = source.createPromptRewriter({ timeoutMs: 500 });
  const input = {
    session: { id: 'synthetic-rewrite', metadata: {} }, text: draft, humanID: 'synthetic-human', harness: 'claude', scope,
    settings: () => ({ ...options.promptRewrite, mode: 'off' }),
    persist: async () => { persisted++; },
    listCatalog: async () => { catalogs++; return { scope, models: [{ id: 'synthetic-editor',
      cost: { input: 0, output: 0 }, outputLimit: 2048, available: true, textInput: true, textOutput: true }] }; },
    complete: async () => { completions++; return { text: '<edited_request>Make this demonstration request easier to read while preserving its meaning and avoiding extra tasks or repeated ideas.</edited_request>', complete: true }; },
  };
  assert.equal(await editor(input), undefined);
  assert.equal(catalogs, 0); assert.equal(completions, 0); assert.equal(persisted, 0);
  input.settings = () => options.promptRewrite;
  await editor(input);
  assert.equal(catalogs, 1); assert.equal(completions, 1);
  assert.ok(persisted >= 1, 'the real admission persists before its synthetic completion');
  assert.equal(source.passthroughState({ ...process.env, ACP_PASSTHROUGH: '1' }).enabled, true);

  assert.deepEqual(Object.fromEntries(Object.entries(process.env)), parentEnvironment);
  assert.equal(sha256(fs.readFileSync(configuration)), sha256(configBytes));
  for (const expected of sourceEvidence) assert.equal(sha256(fs.readFileSync(path.join(prepared, expected.path))), expected.sha256);
  assert.equal(sha256(fs.readFileSync(originalCliEntry)), pin.entrySha256);
  send({ phase: 'effective-source-complete', scope: 'canonical-source-runtime-not-cli',
    tiers: { resolved: true, realAdaptiveTurnUsed: true },
    adaptivePressure: { resolved: true, enabledPathObserved: true, offPathObserved: true, realWindowUnchanged: true,
      configuredTargetRatio: resolved.adaptivePressure.targetRatio,
      configuredGrowthWindow: resolved.adaptivePressure.growthWindow,
      effectiveCurveRatio: resolved.nudge.maxContextLimitPct,
      targetRatioControlQualified: false, growthWindowControlQualified: false },
    promptRewrite: { autoAdmissionObserved: true, offNoDispatchObserved: true, syntheticCompletions: completions, paidCalls: 0 },
    outputSteering: { verbosityMutationObserved: true, effortRoutingOffPreservesHigh: true,
      effortRoutingOnControlLowersEffort: true, disabledBodyUnchanged: true },
    compressionOff: { canonicalPassthroughResolverObserved: true, runningCliTrafficVerified: false },
    runningCliEffectiveConfigVerified: false, providerTrafficVerified: false,
    cliGap: 'No canonical API exposes the whole running resolved config; file/config GET cannot attest execution.',
    selectedPinnedSourceFilesUnchanged: true, originalCliEntryHashUnchanged: true, credentialsRead: false });
} catch {
  send({ phase: 'failed', stage });
  process.exitCode = 1;
} finally {
  esbuild.stop();
  process.stdin.pause();
  clearTimeout(watchdog);
}
