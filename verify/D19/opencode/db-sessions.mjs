// D19: list the sessions in an OpenCode 2.x database, opened READ-ONLY (node:sqlite, Node >= 22.5).
//   node db-sessions.mjs <opencode.db>
// Prints JSON: { tables: [...session-like tables], sessions: [{id, title, ...}] }. Reads ids and titles only.
import { DatabaseSync } from "node:sqlite";

const db = new DatabaseSync(process.argv[2], { readOnly: true });
const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE '%session%' ORDER BY name").all().map((r) => r.name);
const table = ["session_v2", "session"].find((t) => tables.includes(t)) ?? tables[0];
let sessions = [];
if (table) {
  const cols = db.prepare(`PRAGMA table_info("${table}")`).all().map((c) => c.name);
  const pick = ["id", "title", "directory", "time_created", "time_updated"].filter((c) => cols.includes(c));
  sessions = db.prepare(`SELECT ${pick.map((c) => `"${c}"`).join(",")} FROM "${table}"`).all();
}
db.close();
process.stdout.write(JSON.stringify({ tables, table, sessions }, null, 1) + "\n");
