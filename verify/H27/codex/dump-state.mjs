// H27 codex leg - dump what the scratch CODEX_HOME's SQLite stores hold about one thread, so the
// findings can say which files Codex needs to list and resume it. Read-only; scratch home only.
// For every *.sqlite in --home: tables with columns and row counts, plus the rows of each table that
// mention the thread id (long values shortened).
// Usage: node dump-state.mjs --home <CODEX_HOME> --thread <id> --out <file.json>
import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";
import path from "node:path";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const { home, thread, out } = args;
const short = (v) => (typeof v === "string" && v.length > 600 ? `${v.slice(0, 600)}…<${v.length} chars>` : v instanceof Uint8Array ? `<blob ${v.length} bytes>` : v);

const result = {};
for (const file of fs.readdirSync(home).filter((f) => f.endsWith(".sqlite"))) {
  const db = new DatabaseSync(path.join(home, file), { readOnly: true });
  const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").all().map((r) => r.name);
  result[file] = {};
  for (const t of tables) {
    const cols = db.prepare(`PRAGMA table_info("${t}")`).all().map((c) => `${c.name} ${c.type}`);
    const count = db.prepare(`SELECT COUNT(*) AS n FROM "${t}"`).get().n;
    const textCols = db.prepare(`PRAGMA table_info("${t}")`).all().filter((c) => !/BLOB|INT|REAL/i.test(c.type)).map((c) => c.name);
    let rows = [];
    if (thread && textCols.length) {
      const where = textCols.map((c) => `CAST("${c}" AS TEXT) LIKE ?`).join(" OR ");
      rows = db.prepare(`SELECT * FROM "${t}" WHERE ${where} LIMIT 60`).all(...textCols.map(() => `%${thread}%`));
      rows = rows.map((r) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, short(v)])));
    }
    result[file][t] = { columns: cols, rows: count, threadRows: rows };
  }
  db.close();
}
fs.writeFileSync(out, JSON.stringify(result, null, 2));
console.log(`state dump: ${out} (${Object.keys(result).join(", ")})`);
