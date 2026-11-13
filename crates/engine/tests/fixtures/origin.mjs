// Synthetic local model origin for the sidecar integrity proofs. It never leaves the
// machine: it listens on 127.0.0.1 only, on a port the operating system chooses, and
// answers every Messages request with one fixed text block as a valid Anthropic SSE
// stream. Every request is kept on disk exactly as received, so a test can compare what
// the sidecar sent with what the client handed it.
//
//   node origin.mjs --dir <path> [--delay-ms <n>] [--no-record]
//
// stdout: one line {"port":N} once listening.
// <dir>/<n>.body  raw request body bytes; <dir>/<n>.json  {method, path, headers}
// (the authorization and x-api-key headers are never written). n starts at 1.
// --delay-ms: pause before the last stream event (message_stop), default 0.
// --no-record: keep nothing on disk (a long soak would otherwise store every body).
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const args = process.argv.slice(2);
const option = name => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const dir = option('--dir');
if (!dir || !path.isAbsolute(dir)) {
  console.error('--dir <absolute path> is required');
  process.exit(2);
}
const keepRequests = !args.includes('--no-record');
const delayMs = Number(option('--delay-ms') ?? 0);
if (!Number.isInteger(delayMs) || delayMs < 0) {
  console.error('--delay-ms must be a non-negative integer');
  process.exit(2);
}
fs.mkdirSync(dir, { recursive: true });

const SECRET_HEADERS = new Set(['authorization', 'x-api-key']);
const REPLY = 'synthetic origin reply';
// What the real provider lists, so the sidecar sees a model catalog and behaves as it
// does in production instead of skipping its catalog-dependent paths.
const CATALOG = {
  data: [
    { type: 'model', id: 'claude-sonnet-4-5-20250929', display_name: 'Claude Sonnet 4.5', created_at: '2025-09-29T00:00:00Z' },
    { type: 'model', id: 'claude-haiku-4-5-20251001', display_name: 'Claude Haiku 4.5', created_at: '2025-10-01T00:00:00Z' },
  ],
  has_more: false,
  first_id: 'claude-sonnet-4-5-20250929',
  last_id: 'claude-haiku-4-5-20251001',
};
let counter = 0;

// The sidecar may ask a model to edit a draft before forwarding it (prompt rewrite). The
// origin plays that model: it returns the draft between the requested tags with only its
// first semicolon turned into a sentence break and a closing thanks, which is a real
// change (a punctuation-only edit is discarded), so a rewrite note can be told from the
// original. A draft without a semicolon comes back verbatim, as the contract allows.
const REWRITE_SYSTEM = "You edit a user's draft request";
function rewriteReply(request) {
  const system = typeof request.system === 'string' ? request.system : JSON.stringify(request.system ?? '');
  if (!system.includes(REWRITE_SYSTEM)) return undefined;
  const draft = request.messages?.[0]?.content;
  if (typeof draft !== 'string') return undefined;
  const split = /;\s+(\p{L})/u;
  const edited = split.test(draft) ? draft.replace(split, (_, letter) => '. ' + letter.toUpperCase()) + ' Grazie.' : draft;
  return `<edited_request>${edited}</edited_request>`;
}

// A request whose newest message is text containing RETRIEVE_NOW makes the origin play a
// model that asks for the stored original of the first stored-content placeholder it finds
// in the conversation (the retrieve tool the sidecar registered), once. The sidecar's own
// follow-up request ends in the tool result, so it does not trigger a second call.
function retrieveCall(request) {
  const messages = Array.isArray(request.messages) ? request.messages : [];
  const last = messages[messages.length - 1];
  const lastText = JSON.stringify(last?.content ?? '');
  if (last?.role !== 'user' || !lastText.includes('RETRIEVE_NOW')) return undefined;
  const found = /([a-z_]+)\(\\"(m\d+)\\"\) returns the full text/.exec(JSON.stringify(messages));
  return found ? { tool: found[1], ref: found[2] } : undefined;
}

const frame = (event, data) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
const message = model => ({
  id: 'msg_synthetic',
  type: 'message',
  role: 'assistant',
  model,
  content: [],
  stop_reason: null,
  stop_sequence: null,
  usage: { input_tokens: 1, output_tokens: 1 },
});

function record(req, body) {
  if (!keepRequests) return;
  const n = ++counter;
  const headers = {};
  for (const [name, value] of Object.entries(req.headers)) {
    if (!SECRET_HEADERS.has(name)) headers[name] = value;
  }
  fs.writeFileSync(path.join(dir, `${n}.body`), body);
  fs.writeFileSync(path.join(dir, `${n}.json`), JSON.stringify({ method: req.method, path: req.url, headers }));
}

const server = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', chunk => chunks.push(chunk));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    record(req, body);
    const route = (req.url ?? '').split('?')[0];
    if (req.method === 'GET' && route.endsWith('/v1/models')) {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify(CATALOG));
      return;
    }
    if (req.method !== 'POST' || !route.endsWith('/v1/messages')) {
      res.writeHead(404, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ type: 'error', error: { type: 'not_found_error', message: 'synthetic origin' } }));
      return;
    }
    let request = {};
    try {
      request = JSON.parse(body.toString('utf8'));
    } catch {
      // An unreadable body still gets the fixed reply; the test reads the stored bytes.
    }
    const model = typeof request.model === 'string' ? request.model : 'synthetic-model';
    if (request.stream === false) {
      res.writeHead(200, { 'content-type': 'application/json' });
      const text = rewriteReply(request) ?? REPLY;
      res.end(JSON.stringify({ ...message(model), content: [{ type: 'text', text }], stop_reason: 'end_turn' }));
      return;
    }
    res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
    res.write(frame('message_start', { type: 'message_start', message: message(model) }));
    const retrieve = retrieveCall(request);
    if (retrieve) {
      res.write(frame('content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'tool_use', id: 'toolu_retrieve', name: retrieve.tool, input: {} } }));
      res.write(frame('content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'input_json_delta', partial_json: JSON.stringify({ ref: retrieve.ref }) } }));
      res.write(frame('content_block_stop', { type: 'content_block_stop', index: 0 }));
      res.write(frame('message_delta', { type: 'message_delta', delta: { stop_reason: 'tool_use', stop_sequence: null }, usage: { output_tokens: 4 } }));
      setTimeout(() => {
        res.end(frame('message_stop', { type: 'message_stop' }));
      }, delayMs);
      return;
    }
    res.write(frame('content_block_start', { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }));
    res.write(frame('ping', { type: 'ping' }));
    res.write(frame('content_block_delta', { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: REPLY } }));
    res.write(frame('content_block_stop', { type: 'content_block_stop', index: 0 }));
    res.write(frame('message_delta', { type: 'message_delta', delta: { stop_reason: 'end_turn', stop_sequence: null }, usage: { output_tokens: 4 } }));
    setTimeout(() => {
      res.end(frame('message_stop', { type: 'message_stop' }));
    }, delayMs);
  });
});

server.listen(0, '127.0.0.1', () => {
  process.stdout.write(JSON.stringify({ port: server.address().port }) + '\n');
});
