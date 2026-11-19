// D19 fixture: a trivial MCP server over stdio (newline-delimited JSON-RPC 2.0). It offers ONE tool named
// "<server name>-tool", so the tool name in a model request shows which MCP servers an instance loaded.
//   node mcp-server.mjs <server name>
// No network, no files: it only answers initialize, tools/list, tools/call and ping.
import readline from "node:readline";

const name = process.argv[2] || "d19-unnamed-mcp";
const tool = `${name}-tool`;
const send = (msg) => process.stdout.write(JSON.stringify(msg) + "\n");

readline.createInterface({ input: process.stdin }).on("line", (line) => {
  let msg;
  try { msg = JSON.parse(line); } catch { return; }
  if (msg.id === undefined) return; // notifications
  switch (msg.method) {
    case "initialize":
      return send({ jsonrpc: "2.0", id: msg.id, result: {
        protocolVersion: msg.params?.protocolVersion ?? "2025-06-18",
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name, version: "0.0.1" } } });
    case "tools/list":
      return send({ jsonrpc: "2.0", id: msg.id, result: { tools: [{
        name: tool, description: `D19 MCP probe tool of server ${name}`,
        inputSchema: { type: "object", properties: {}, additionalProperties: false } }] } });
    case "tools/call":
      return send({ jsonrpc: "2.0", id: msg.id, result: { content: [{ type: "text", text: `${tool} ok` }] } });
    case "ping":
      return send({ jsonrpc: "2.0", id: msg.id, result: {} });
    default:
      return send({ jsonrpc: "2.0", id: msg.id, error: { code: -32601, message: `method not found: ${msg.method}` } });
  }
});
