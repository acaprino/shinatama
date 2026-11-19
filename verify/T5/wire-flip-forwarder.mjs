// Logging forwarder: Claude Code -> this (http, 127.0.0.1:PORT) -> upstream (bili base-URL listener) -> Anthropic.
// Logs only structural fields of /v1/messages requests and cache usage of responses. No auth headers, no content.
// Usage: node wire-log.mjs <port> <upstream-base e.g. http://127.0.0.1:18794> <logfile>
import http from "node:http";
import https from "node:https";
import { appendFileSync } from "node:fs";
const [port, upstream, logfile] = [Number(process.argv[2]), new URL(process.argv[3]), process.argv[4]];
const flipAt = Number(process.argv[5] ?? 0), flipTo = process.argv[6] ?? "low"; let mainCount = 0; const strip = process.argv[7] === "strip"; const modelKey = process.argv[8] ?? "sonnet";
const lib = upstream.protocol === "https:" ? https : http;
const log = (o) => appendFileSync(logfile, JSON.stringify(o) + "\n");
http.createServer((req, res) => {
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    let body = Buffer.concat(chunks);
    let info = null;
    if (req.method === "POST" && req.url.startsWith("/v1/messages") && !req.url.includes("count_tokens")) {
      try {
        const j = JSON.parse(body.toString("utf8"));
        let flipped = false;
        if (String(j.model).includes(modelKey)) { mainCount++;
          if (strip) { j.messages = (j.messages ?? []).filter((m) => !(m.role === "system" && m.output_config)); }
          if (flipAt > 0 && mainCount >= flipAt && j.output_config) { j.output_config.effort = flipTo; flipped = true; }
          if (strip || flipped) body = Buffer.from(JSON.stringify(j), "utf8"); }
        const roles = (j.messages ?? []).map((m) => m.role[0] + (m.role === "system" ? ":" + JSON.stringify(m.output_config ?? null) : "")).join(",");
        const sysLen = Array.isArray(j.system) ? j.system.reduce((n, b) => n + (b.text?.length ?? 0), 0) : String(j.system ?? "").length;
        const cc = (j.messages ?? []).map((m, i) => Array.isArray(m.content) ? m.content.some((b) => b.cache_control) ? i : -1 : -1).filter((i) => i >= 0);
        info = { flipped, roles, model: j.model, output_config: j.output_config ?? null, thinking: j.thinking ?? null, tools: (j.tools ?? []).length,
                 system_chars: sysLen, system_cc: Array.isArray(j.system) ? j.system.filter((b) => b.cache_control).length : 0,
                 messages: (j.messages ?? []).length, cc_msg_idx: cc, max_tokens: j.max_tokens, stream: j.stream ?? false };
      } catch { info = { parse_error: true }; }
    }
    const headers = { ...req.headers, host: upstream.host };
    delete headers["content-length"];
    const up = lib.request({ hostname: upstream.hostname, port: upstream.port || (upstream.protocol === "https:" ? 443 : 80), path: req.url, method: req.method, headers: { ...headers, "content-length": body.length } }, (ur) => {
      res.writeHead(ur.statusCode, ur.headers);
      const out = [];
      ur.on("data", (c) => { out.push(c); res.write(c); });
      ur.on("end", () => {
        res.end();
        if (info) {
          const t = Buffer.concat(out).toString("utf8");
          const num = (k) => { const m = [...t.matchAll(new RegExp(`"${k}"\\s*:\\s*(\\d+)`, "g"))]; return m.length ? Number(m[m.length - 1][1]) : null; };
          log({ t: new Date().toISOString(), status: ur.statusCode, ...info, cache_read: num("cache_read_input_tokens"), cache_creation: num("cache_creation_input_tokens"), input: num("input_tokens") });
        }
      });
    });
    up.on("error", (e) => { res.writeHead(502); res.end(String(e)); log({ error: String(e) }); });
    up.end(body);
  });
}).listen(port, "127.0.0.1", () => console.log("listening", port));
