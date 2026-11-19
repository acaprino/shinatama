// H27 codex leg - point a thread's row in a PROBE home's state_5.sqlite at another rollout path
// (probe-store.ps1 only; never run on the main scratch home or a real one). Prints the old and new path.
// Usage: node set-rollout-path.mjs --db <home\state_5.sqlite> --thread <id> --path <rollout path>
import { DatabaseSync } from "node:sqlite";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const db = new DatabaseSync(args.db);
const before = db.prepare("SELECT rollout_path FROM threads WHERE id = ?").get(args.thread);
db.prepare("UPDATE threads SET rollout_path = ? WHERE id = ?").run(args.path, args.thread);
db.close();
console.log(`threads.rollout_path: ${before?.rollout_path ?? "<no row>"} -> ${args.path}`);
