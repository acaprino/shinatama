// Real installed Pi -> generated extension -> local synthetic provider. No external model requests.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp, writeFile, readFile, rm, mkdir } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

const cli = process.env.SHINATAMA_PI_SMOKE_CLI;
if (!cli) throw new Error('Set SHINATAMA_PI_SMOKE_CLI to the installed Pi CLI JavaScript entry.');
const fixture = await mkdtemp(resolve(tmpdir(), 'shinatama-model-provider-'));
const secret = '!literal-$PATH-${HOME}';
const modelId = 'org/smoke-model:v2';
const requests = [];
const server = createServer(async (request, response) => {
  let body = ''; for await (const chunk of request) body += chunk;
  requests.push({ path: request.url, key: request.headers.authorization, body: JSON.parse(body) });
  response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' });
  const chunk = (delta, finish_reason = null) => ({ id: 'smoke', object: 'chat.completion.chunk', created: 1, model: modelId, choices: [{ index: 0, delta, finish_reason }] });
  response.write(`data: ${JSON.stringify(chunk({ role: 'assistant', content: 'smoke-response' }))}\n\n`);
  response.write(`data: ${JSON.stringify(chunk({}, 'stop'))}\n\n`);
  response.end('data: [DONE]\n\n');
});
await new Promise(yes => server.listen(0, '127.0.0.1', yes));
const baseUrl = `http://127.0.0.1:${server.address().port}/v1`;
try {
  const selection = { connectionId: 'fixture', modelId };
  const configuration = { connections: [{ id: 'fixture', name: 'Smoke', api: 'openai-completions', baseUrl,
    models: [{ id: modelId, contextWindow: 32000, maxTokens: 4000, reasoning: false, vision: false }] }],
    assignments: { opencode: selection, pi: selection, hermes: null } };
  const translated = JSON.parse(execFileSync('cargo', ['run', '--quiet', '--locked', '-p', 'shinatama-engine', '--example', 'model_config_fixture'], {
    input: JSON.stringify(configuration), encoding: 'utf8', windowsHide: true, timeout: 180000,
  }));
  assert.ok(!JSON.stringify(translated).includes(secret));
  const extension = resolve(fixture, 'provider.mjs'); await writeFile(extension, translated.pi);
  const home = resolve(fixture, 'agent'); await mkdir(home);
  await writeFile(resolve(home, 'auth.json'), JSON.stringify({ 'shinatama-fixture': { type: 'api_key', key: 'stale-native-key' } }));
  const env = { ...process.env, PI_CODING_AGENT_DIR: home, PI_OFFLINE: '1', PI_SKIP_VERSION_CHECK: '1', PI_TELEMETRY: '0', SHINATAMA_MODEL_API_KEY: secret };
  // No project configuration, discovered extension or command-line secret enters this process.
  delete env.NODE_OPTIONS; delete env.NODE_PATH;
  const result = await new Promise((yes, no) => {
    const child = spawn(process.execPath, [cli, '--no-extensions', '--no-skills', '--no-prompt-templates', '-e', extension,
      '--provider', 'shinatama-fixture', '--model', modelId, '--mode', 'json', '-p', 'Reply with the test response.'], {
      cwd: fixture, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
    });
    let out = '', error = '';
    child.stdout.on('data', chunk => { out += chunk; }); child.stderr.on('data', chunk => { error += chunk; });
    const timer = setTimeout(() => { child.kill(); no(new Error('Pi provider smoke timed out after 60 seconds.')); }, 60000);
    child.on('error', no); child.on('close', code => { clearTimeout(timer); yes({ code, out, error }); });
  });
  assert.equal(result.code, 0, result.error.replaceAll(secret, '[redacted]'));
  assert.ok(result.out.includes('smoke-response'), result.error.replaceAll(secret, '[redacted]'));
  assert.equal(requests.length, 1);
  assert.equal(requests[0].path, '/v1/chat/completions');
  assert.equal(requests[0].key, `Bearer ${secret}`);
  assert.equal(requests[0].body.model, modelId);
  assert.ok(!result.out.includes(secret));
  assert.equal(JSON.parse(await readFile(resolve(home, 'auth.json'), 'utf8'))['shinatama-fixture'].key, 'stale-native-key');
  console.log('PASS installed Pi: generated provider, selected model, literal !/$ key, stale native key overridden without changing auth.json');
  if (process.env.SHINATAMA_OPENCODE_SMOKE_EXE) {
    requests.length = 0;
    const opencodeEnv = { ...env, HOME: fixture, USERPROFILE: fixture, APPDATA: fixture, LOCALAPPDATA: fixture,
      XDG_CONFIG_HOME: resolve(fixture, 'config'), XDG_DATA_HOME: resolve(fixture, 'data'), XDG_STATE_HOME: resolve(fixture, 'state'), XDG_CACHE_HOME: resolve(fixture, 'cache'),
      OPENCODE_CONFIG_DIR: resolve(fixture, 'config'), OPENCODE_DISABLE_PROJECT_CONFIG: '1', OPENCODE_DISABLE_AUTOUPDATE: '1', OPENCODE_DISABLE_MODELS_FETCH: '1',
      OPENCODE_CONFIG_CONTENT: JSON.stringify({ ...translated.opencode, plugin: [], permission: 'deny' }) };
    const result = await new Promise((yes, no) => {
      const child = spawn(process.env.SHINATAMA_OPENCODE_SMOKE_EXE, ['run', '--standalone', '--format', 'json', '--title', 'Model settings smoke', 'Reply with the test response.'], {
        cwd: fixture, env: opencodeEnv, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
      });
      let out = '', error = '';
      child.stdout.on('data', chunk => { out += chunk; }); child.stderr.on('data', chunk => { error += chunk; });
      const timer = setTimeout(() => { try { execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' }); } catch {} no(new Error('OpenCode provider smoke timed out after 60 seconds.')); }, 60000);
      child.on('error', no); child.on('close', code => { clearTimeout(timer); yes({ code, out, error }); });
    });
    assert.equal(result.code, 0, result.error.replaceAll(secret, '[redacted]'));
    assert.ok(result.out.includes('smoke-response'), (result.error + result.out).replaceAll(secret, '[redacted]'));
    assert.ok(requests.length > 0);
    for (const request of requests) { assert.equal(request.path, '/v1/chat/completions'); assert.equal(request.key, `Bearer ${secret}`); assert.equal(request.body.model, modelId); }
    assert.ok(!result.out.includes(secret));
    console.log('PASS installed OpenCode: generated configuration, selected model, literal key, loopback requests');
  }
} finally {
  server.closeAllConnections(); await new Promise(yes => server.close(yes));
  await rm(fixture, { recursive: true, force: true, maxRetries: 3 });
}
