// H27 codex leg - egress sink. env.ps1 points HTTP_PROXY/HTTPS_PROXY/ALL_PROXY here (NO_PROXY keeps
// 127.0.0.1 direct), so any request Codex tries to send to a real endpoint is logged and refused
// instead of leaving the machine. An empty log means no attempt went through the proxy settings.
// Usage: node egress-sink.mjs --port 4142 --log C:\path\egress.jsonl
import http from "node:http";
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const PORT = Number(args.port || 4142);
const LOG = args.log || "egress.jsonl";
const log = (entry) => fs.appendFileSync(LOG, JSON.stringify({ ts: new Date().toISOString(), ...entry }) + "\n");

const server = http.createServer((req, res) => {
  log({ method: req.method, url: req.url, host: req.headers.host || "" });
  res.writeHead(403, { "content-type": "text/plain" });
  res.end("h27 egress sink: blocked");
});
server.on("connect", (req, socket) => {
  log({ method: "CONNECT", url: req.url });
  socket.end("HTTP/1.1 403 Forbidden\r\n\r\n");
});
server.listen(PORT, "127.0.0.1", () => process.stdout.write(`h27 egress sink on 127.0.0.1:${PORT} log=${LOG}\n`));
