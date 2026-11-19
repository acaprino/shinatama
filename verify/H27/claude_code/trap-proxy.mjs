// H27 trap proxy. HTTPS_PROXY points here during the Claude Code runs, so any request the
// harness tries to make to a real endpoint (api.anthropic.com, statsig, sentry, ...) is
// refused and logged as one JSON line instead of leaving the machine. An empty log is the
// evidence that the run talked only to the mock.
// Usage: node trap-proxy.mjs --port 4191 --log C:\path\trap.jsonl
import http from "node:http";
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const PORT = Number(args.port || 4191);
const LOG = args.log || "trap.jsonl";
const log = (entry) => fs.appendFileSync(LOG, JSON.stringify({ ts: new Date().toISOString(), ...entry }) + "\n");

const server = http.createServer((req, res) => {
  log({ method: req.method, url: req.url, host: req.headers.host });
  res.writeHead(403, { "content-type": "text/plain" });
  res.end("h27 trap proxy: real endpoints are blocked\n");
});
server.on("connect", (req, socket) => {
  log({ method: "CONNECT", target: req.url });
  socket.end("HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n");
});
server.listen(PORT, "127.0.0.1", () => process.stdout.write(`h27 trap proxy on http://127.0.0.1:${PORT} log=${LOG}\n`));
