// H27 mock model server. Speaks three wire dialects so the four harnesses can run
// with no account and no cost:
//   Anthropic Messages      POST …/v1/messages            (Claude Code, OpenCode, Pi)
//   OpenAI Responses        POST …/responses               (Codex)
//   OpenAI Chat Completions POST …/chat/completions        (fallback)
// Every request is appended to the capture file as one JSON line, so a test can
// read exactly what a harness sent to the model (the resume context).
//
// Replies are scripted from the LAST user turn of the request, never from state:
//   "H27-TOOLS"   → reasoning + text + TWO parallel calls of the harness's read tool
//                   (or shell tool) on every *.txt absolute path found in the prompt
//   after tool results → "H27-FINAL …" (end of turn)
//   "H27-HANG"    → headers, one text delta, then never finishes (interruption tests)
//   summary/compaction request → "<summary>H27-SUMMARY …</summary>"
//   anything else → "H27-REPLY: <echo>"
//
// Usage: node server.mjs --port 4127 --capture C:\path\capture.jsonl

import http from "node:http";
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const PORT = Number(args.port || process.env.H27_PORT || 4127);
const CAPTURE = args.capture || process.env.H27_CAPTURE || "capture.jsonl";
let counter = 0;

const SUMMARY_RE = /summar|compact|checkpoint|handoff|continue the conversation|continuing our conversation/i;
const PATH_RE = /([A-Za-z]:[\\/][^\s"'`<>|]+?\.txt|\/[^\s"'`<>|]+?\.txt)/g;

function capture(entry) {
  fs.appendFileSync(CAPTURE, JSON.stringify(entry) + "\n");
}

function redactHeaders(h) {
  const out = { ...h };
  for (const k of ["authorization", "x-api-key", "api-key", "cookie"]) if (out[k]) out[k] = "<redacted>";
  return out;
}

// ---------- reading the last user turn in each dialect ----------

function textOf(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((b) => (typeof b === "string" ? b : b.text ?? (typeof b.content === "string" ? b.content : "")))
    .join("\n");
}

function lastTurnAnthropic(body) {
  // Claude Code 2.1.311 appends a role:"system" message after each user turn; it is not a turn.
  const msgs = (body.messages || []).filter((m) => m.role !== "system");
  const last = msgs[msgs.length - 1];
  if (!last) return { kind: "text", text: "" };
  if (Array.isArray(last.content) && last.content.some((b) => b.type === "tool_result")) return { kind: "after-tools" };
  return { kind: "text", text: textOf(last.content) };
}

function lastTurnResponses(body) {
  const input = Array.isArray(body.input) ? body.input : [{ role: "user", content: body.input }];
  const last = input[input.length - 1];
  if (!last) return { kind: "text", text: "" };
  if (last.type === "function_call_output" || last.type === "custom_tool_call_output") return { kind: "after-tools" };
  for (let i = input.length - 1; i >= 0; i--) {
    if (input[i].role === "user") return { kind: "text", text: textOf(input[i].content) };
  }
  return { kind: "text", text: "" };
}

function lastTurnChat(body) {
  const msgs = body.messages || [];
  const last = msgs[msgs.length - 1];
  if (!last) return { kind: "text", text: "" };
  if (last.role === "tool") return { kind: "after-tools" };
  return { kind: "text", text: textOf(last.content) };
}

// ---------- choosing the tool the harness itself offered ----------

// tools: [{name, schema}] normalised from any dialect.
function pickTool(tools) {
  const byName = (names) => tools.find((t) => names.includes(t.name.toLowerCase()));
  const read = byName(["read", "read_file", "view", "readfile"]);
  if (read) {
    const props = read.schema?.properties || {};
    const key = ["file_path", "filePath", "path", "file", "absolute_path"].find((k) => k in props) || "file_path";
    return { tool: read, input: (p) => ({ [key]: p }) };
  }
  const shell = byName(["shell", "shell_command", "exec_command", "bash", "local_shell", "powershell"]);
  if (shell) {
    const props = shell.schema?.properties || {};
    const key = ["command", "cmd"].find((k) => k in props) || "command";
    const isArray = props[key]?.type === "array";
    const ps = (p) => `Get-Content -Raw '${p}'`;
    return {
      tool: shell,
      input: (p) => ({ [key]: isArray ? ["powershell.exe", "-NoProfile", "-Command", ps(p)] : ps(p) }),
    };
  }
  return null;
}

function decide(turn, tools) {
  if (turn.kind === "after-tools") return { text: "H27-FINAL: both files were read.", calls: [], stop: "end" };
  const t = turn.text || "";
  // H27/pi local change: Pi's compaction request offers no tools and quotes the whole
  // conversation (including "H27-TOOLS …") in one user turn; answer it as a summary.
  if (!tools.length && SUMMARY_RE.test(t)) return summaryPlan();
  if (t.includes("H27-HANG")) return { hang: true };
  if (t.includes("H27-TOOLS")) {
    const paths = [...new Set(t.match(PATH_RE) || [])].slice(0, 2);
    const pick = pickTool(tools);
    if (pick && paths.length) {
      return {
        reasoning: "H27-REASONING: the user wants two files; read them in parallel.",
        text: "H27-PRE: reading the files.",
        calls: paths.map((p, i) => ({ id: `h27call${counter}x${i}`, name: pick.tool.name, input: pick.input(p) })),
        stop: "tool",
      };
    }
    return { text: `H27-NOTOOL: no read or shell tool offered (tools: ${tools.map((x) => x.name).join(",")})`, calls: [], stop: "end" };
  }
  if (SUMMARY_RE.test(t)) return summaryPlan();
  return { text: `H27-REPLY: ${t.replace(/\s+/g, " ").slice(0, 80)}`, calls: [], stop: "end" };
}

function summaryPlan() {
  return {
    text: "<summary>H27-SUMMARY: the user asked to read alpha.txt and beta.txt; both were read; then said hello.</summary>",
    calls: [],
    stop: "end",
  };
}

// ---------- Anthropic Messages ----------

function sse(res, event, data) {
  res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
}

function anthropic(body, res) {
  const tools = (body.tools || []).map((t) => ({ name: t.name || "", schema: t.input_schema }));
  const plan = decide(lastTurnAnthropic(body), tools);
  const id = `msg_h27_${counter}`;
  const model = body.model || "h27-mock";
  const blocks = [];
  if (plan.reasoning) blocks.push({ type: "thinking", thinking: plan.reasoning, signature: "h27-mock-signature" });
  if (plan.text) blocks.push({ type: "text", text: plan.text });
  for (const c of plan.calls || []) blocks.push({ type: "tool_use", id: `toolu_${c.id}`, name: c.name, input: c.input });
  const stop_reason = plan.stop === "tool" ? "tool_use" : "end_turn";
  const usage = { input_tokens: 100, output_tokens: 20, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };

  if (!body.stream) {
    res.writeHead(200, { "content-type": "application/json" });
    return res.end(JSON.stringify({ id, type: "message", role: "assistant", model, content: blocks, stop_reason, stop_sequence: null, usage }));
  }
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  sse(res, "message_start", {
    type: "message_start",
    message: { id, type: "message", role: "assistant", model, content: [], stop_reason: null, stop_sequence: null, usage: { ...usage, output_tokens: 1 } },
  });
  if (plan.hang) {
    sse(res, "content_block_start", { type: "content_block_start", index: 0, content_block: { type: "text", text: "" } });
    sse(res, "content_block_delta", { type: "content_block_delta", index: 0, delta: { type: "text_delta", text: "H27-PARTIAL: started " } });
    return; // never finishes
  }
  blocks.forEach((b, index) => {
    if (b.type === "thinking") {
      sse(res, "content_block_start", { type: "content_block_start", index, content_block: { type: "thinking", thinking: "", signature: "" } });
      sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "thinking_delta", thinking: b.thinking } });
      sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "signature_delta", signature: b.signature } });
    } else if (b.type === "text") {
      sse(res, "content_block_start", { type: "content_block_start", index, content_block: { type: "text", text: "" } });
      sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "text_delta", text: b.text } });
    } else {
      sse(res, "content_block_start", { type: "content_block_start", index, content_block: { type: "tool_use", id: b.id, name: b.name, input: {} } });
      sse(res, "content_block_delta", { type: "content_block_delta", index, delta: { type: "input_json_delta", partial_json: JSON.stringify(b.input) } });
    }
    sse(res, "content_block_stop", { type: "content_block_stop", index });
  });
  sse(res, "message_delta", { type: "message_delta", delta: { stop_reason, stop_sequence: null }, usage: { output_tokens: 20 } });
  sse(res, "message_stop", { type: "message_stop" });
  res.end();
}

// ---------- OpenAI Responses (Codex) ----------

function responses(body, res) {
  const tools = (body.tools || []).map((t) => ({ name: t.name || t.function?.name || t.type || "", schema: t.parameters || t.function?.parameters }));
  const plan = decide(lastTurnResponses(body), tools);
  const respId = `resp_h27_${counter}`;
  const items = [];
  if (plan.reasoning)
    items.push({
      type: "reasoning",
      id: `rs_h27_${counter}`,
      summary: [{ type: "summary_text", text: plan.reasoning }],
      encrypted_content: "h27-mock-encrypted-reasoning",
    });
  if (plan.text)
    items.push({ type: "message", id: `msg_h27_${counter}`, role: "assistant", status: "completed", content: [{ type: "output_text", text: plan.text, annotations: [] }] });
  for (const c of plan.calls || [])
    items.push({ type: "function_call", id: `fc_${c.id}`, call_id: `call_${c.id}`, name: c.name, arguments: JSON.stringify(c.input), status: "completed" });
  const usage = { input_tokens: 100, input_tokens_details: { cached_tokens: 0 }, output_tokens: 20, output_tokens_details: { reasoning_tokens: 0 }, total_tokens: 120 };
  const response = { id: respId, object: "response", status: "completed", model: body.model || "h27-mock", output: items, usage };

  if (body.stream === false) {
    res.writeHead(200, { "content-type": "application/json" });
    return res.end(JSON.stringify(response));
  }
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  const send = (data) => sse(res, data.type, data);
  send({ type: "response.created", response: { ...response, status: "in_progress", output: [] } });
  if (plan.hang) {
    const item = { type: "message", id: `msg_h27_${counter}`, role: "assistant", status: "in_progress", content: [] };
    send({ type: "response.output_item.added", output_index: 0, item });
    send({ type: "response.output_text.delta", item_id: item.id, output_index: 0, content_index: 0, delta: "H27-PARTIAL: started " });
    return;
  }
  items.forEach((item, output_index) => {
    send({ type: "response.output_item.added", output_index, item: { ...item, status: "in_progress" } });
    if (item.type === "message")
      send({ type: "response.output_text.delta", item_id: item.id, output_index, content_index: 0, delta: item.content[0].text });
    if (item.type === "reasoning")
      send({ type: "response.reasoning_summary_text.delta", item_id: item.id, output_index, summary_index: 0, delta: item.summary[0].text });
    send({ type: "response.output_item.done", output_index, item });
  });
  send({ type: "response.completed", response });
  res.end();
}

// ---------- OpenAI Chat Completions (fallback) ----------

function chat(body, res) {
  const tools = (body.tools || []).map((t) => ({ name: t.function?.name || "", schema: t.function?.parameters }));
  const plan = decide(lastTurnChat(body), tools);
  const id = `chatcmpl_h27_${counter}`;
  const created = Math.floor(Date.now() / 1000);
  const model = body.model || "h27-mock";
  const tool_calls = (plan.calls || []).map((c, index) => ({ index, id: `call_${c.id}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.input) } }));
  const finish_reason = plan.stop === "tool" ? "tool_calls" : "stop";
  const usage = { prompt_tokens: 100, completion_tokens: 20, total_tokens: 120 };
  if (!body.stream) {
    res.writeHead(200, { "content-type": "application/json" });
    const message = { role: "assistant", content: plan.text || null, ...(plan.reasoning ? { reasoning_content: plan.reasoning } : {}), ...(tool_calls.length ? { tool_calls: tool_calls.map(({ index, ...t }) => t) } : {}) };
    return res.end(JSON.stringify({ id, object: "chat.completion", created, model, choices: [{ index: 0, message, finish_reason }], usage }));
  }
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  const chunk = (delta, fr = null, extra = {}) =>
    res.write(`data: ${JSON.stringify({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta, finish_reason: fr }], ...extra })}\n\n`);
  chunk({ role: "assistant", content: "" });
  if (plan.hang) return chunk({ content: "H27-PARTIAL: started " });
  if (plan.reasoning) chunk({ reasoning_content: plan.reasoning });
  if (plan.text) chunk({ content: plan.text });
  if (tool_calls.length) chunk({ tool_calls });
  chunk({}, finish_reason);
  res.write(`data: ${JSON.stringify({ id, object: "chat.completion.chunk", created, model, choices: [], usage })}\n\n`);
  res.write("data: [DONE]\n\n");
  res.end();
}

// ---------- server ----------

const server = http.createServer((req, res) => {
  let raw = "";
  req.on("data", (c) => (raw += c));
  req.on("end", () => {
    counter += 1;
    let body = null;
    try {
      body = raw ? JSON.parse(raw) : null;
    } catch {
      body = { _unparsed: raw.slice(0, 2000) };
    }
    const url = req.url || "";
    capture({ n: counter, ts: new Date().toISOString(), method: req.method, url, headers: redactHeaders(req.headers), body });
    try {
      if (req.method === "POST" && /\/messages\/count_tokens/.test(url)) {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ input_tokens: 100 }));
      }
      if (req.method === "POST" && /\/messages(\?|$)/.test(url)) return anthropic(body || {}, res);
      if (req.method === "POST" && /\/responses(\?|$)/.test(url)) return responses(body || {}, res);
      if (req.method === "POST" && /\/chat\/completions(\?|$)/.test(url)) return chat(body || {}, res);
      if (req.method === "GET" && /\/models/.test(url)) {
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ object: "list", data: [{ id: "h27-mock", object: "model", type: "model", display_name: "H27 mock", created_at: "2026-10-27T00:00:00Z" }], models: [] }));
      }
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: { type: "not_found", message: `h27 mock: no route for ${req.method} ${url}` } }));
    } catch (e) {
      res.writeHead(500, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: { type: "h27_mock_error", message: String(e) } }));
    }
  });
});

server.listen(PORT, "127.0.0.1", () => {
  process.stdout.write(`h27 mock listening on http://127.0.0.1:${PORT} capture=${CAPTURE}\n`);
});
