// P20 helper: listens on a named pipe (the dummy CLAUDE_CODE_MESSAGING_SOCKET handed to the
// child) and logs every connection and the number of bytes received, as JSON lines.
// It never answers. Usage: node pipe-listener.mjs <\.\pipe\name> <log.jsonl>
import net from "node:net";
import fs from "node:fs";
const [pipe, log] = process.argv.slice(2);
const w = (e) => fs.appendFileSync(log, JSON.stringify({ ts: new Date().toISOString(), ...e }) + "\n");
const server = net.createServer((sock) => {
  let bytes = 0;
  let head = "";
  w({ event: "connect" });
  sock.on("data", (d) => { bytes += d.length; if (head.length < 120) head += d.toString("utf8").slice(0, 120 - head.length); });
  sock.on("close", () => w({ event: "close", bytes, headShape: head.replace(/[A-Za-z0-9+/=_-]{16,}/g, "<long>") }));
  sock.on("error", (e) => w({ event: "error", message: String(e.message) }));
});
server.listen(pipe, () => { w({ event: "listening", pipe }); process.stdout.write("listening\n"); });
