// D16 / Pi - a proxy that forwards nothing: logs every request/CONNECT target it is asked for
// (one JSON line per attempt) and refuses it. Used as HTTP(S)_PROXY to see which hosts Pi tries
// to reach (version check, telemetry, catalog refresh) without any traffic leaving the machine.
// Usage: node logproxy.mjs --port 4169 --log <file.jsonl>
import http from "node:http";
import fs from "node:fs";

const a = process.argv.slice(2);
const opt = (k, d) => (a.includes(k) ? a[a.indexOf(k) + 1] : d);
const PORT = Number(opt("--port", 4169));
const LOG = opt("--log", "proxy.jsonl");
const log = (e) => fs.appendFileSync(LOG, JSON.stringify({ ts: new Date().toISOString(), ...e }) + "\n");

const server = http.createServer((req, res) => {
  log({ kind: "http", method: req.method, url: req.url, ua: req.headers["user-agent"] || "" });
  res.writeHead(502, { "content-type": "text/plain" });
  res.end("d16 logproxy: refused");
});
server.on("connect", (req, socket) => {
  log({ kind: "connect", target: req.url, ua: req.headers["user-agent"] || "" });
  socket.end("HTTP/1.1 403 Forbidden\r\n\r\n");
});
server.listen(PORT, "127.0.0.1", () => process.stdout.write(`d16 logproxy on ${PORT} -> ${LOG}\n`));
