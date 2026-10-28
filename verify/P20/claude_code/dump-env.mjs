// P20 helper: writes the NAMES (never the values) of the environment variables this process
// received, one per line, to <dir>/<label>-<pid>.txt. Used as a hook command, from the shell
// tool, and (via mcp-env.mjs) as an MCP server. Usage: node dump-env.mjs <dir> <label>
import fs from "node:fs";
import path from "node:path";
const [dir, label = "env"] = process.argv.slice(2);
fs.mkdirSync(dir, { recursive: true });
const names = Object.keys(process.env).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
fs.writeFileSync(path.join(dir, `${label}-${process.pid}.txt`), names.join("\n") + "\n");
process.stdout.write(`${label}: ${names.length} names\n`);
