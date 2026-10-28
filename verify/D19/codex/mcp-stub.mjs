// D19 synthetic MCP server (stdio, newline-delimited JSON-RPC). Offers ONE tool named after the server, so the
// model request shows which MCP servers an instance loaded:  node mcp-stub.mjs d19-a-mcp  ->  tool d19_a_mcp_ping
// No network, no files; answers initialize, tools/list, tools/call, ping; ignores notifications.
import { createInterface } from "node:readline";

const server = process.argv[2] || "d19-unnamed-mcp";
const tool = server.replace(/[^A-Za-z0-9]/g, "_") + "_ping";
const send = (m) => process.stdout.write(JSON.stringify(m) + "\n");

createInterface({ input: process.stdin }).on("line", (line) => {
  let m;
  try { m = JSON.parse(line); } catch { return; }
  if (m.id === undefined || m.id === null) return; // notification
  switch (m.method) {
    case "initialize":
      return send({ jsonrpc: "2.0", id: m.id, result: {
        protocolVersion: m.params?.protocolVersion || "2025-06-18",
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: server, version: "1.0.0" },
      } });
    case "tools/list":
      return send({ jsonrpc: "2.0", id: m.id, result: { tools: [{
        name: tool, description: `D19 marker tool of MCP server ${server}`,
        inputSchema: { type: "object", properties: {}, additionalProperties: false },
      }] } });
    case "tools/call":
      return send({ jsonrpc: "2.0", id: m.id, result: { content: [{ type: "text", text: `${server} pong` }] } });
    case "ping":
      return send({ jsonrpc: "2.0", id: m.id, result: {} });
    default:
      return send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: `method not found: ${m.method}` } });
  }
});
