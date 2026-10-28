// H27 / OpenCode: read-only dump of one session's rows from an OpenCode 2.1 SQLite store.
// Usage: node --no-warnings dump-rows.mjs <opencode.db> <sessionID> <out.json>
// JSON columns (data, payload, model, metadata, ...) are parsed so the file reads as the stored shape.
import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";

const [dbPath, sessionID, outFile] = process.argv.slice(2);
if (!dbPath || !sessionID || !outFile) {
  console.error("usage: node dump-rows.mjs <opencode.db> <sessionID> <out.json>");
  process.exit(2);
}
const db = new DatabaseSync(dbPath, { readOnly: true });
const tables = new Set(db.prepare("select name from sqlite_master where type='table'").all().map((r) => r.name));

function parsed(row) {
  const out = {};
  for (const [k, v] of Object.entries(row)) {
    if (typeof v === "string" && /^[[{]/.test(v)) {
      try {
        out[k] = JSON.parse(v);
        continue;
      } catch {}
    }
    out[k] = v;
  }
  return out;
}
function rows(sql, ...args) {
  return db.prepare(sql).all(...args).map(parsed);
}

const result = { db: dbPath, sessionID };
if (tables.has("session_v2")) result.session_v2 = rows("select * from session_v2 where id = ?", sessionID);
if (tables.has("session_message"))
  result.session_message = rows("select * from session_message where session_id = ? order by seq", sessionID);
for (const t of ["session_inbox", "session_pending"])
  if (tables.has(t)) result[t] = rows(`select * from ${t} where session_id = ?`, sessionID);
for (const t of ["instruction_entry", "instruction_state"])
  if (tables.has(t)) result[t] = rows(`select * from ${t} where session_id = ?`, sessionID);
if (tables.has("event")) result.event = rows("select * from event where aggregate_id = ? order by seq", sessionID);
fs.writeFileSync(outFile, JSON.stringify(result, null, 2));
const types = (result.session_message || []).map((m) => `${m.seq}:${m.type}`).join(" ");
console.log(`rows: session_v2=${(result.session_v2 || []).length} session_message=${(result.session_message || []).length} [${types}] -> ${outFile}`);
if (!(result.session_v2 || []).length) process.exit(3);
