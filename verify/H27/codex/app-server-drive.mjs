// H27 codex leg - drive an existing thread over `codex app-server` (stdio JSON-RPC), for the two steps
// `codex exec` cannot do non-interactively:
//   --action compact    thread/resume -> thread/compact/start, wait for the contextCompaction item and
//                       turn/completed (Codex's own compaction; exec has no /compact).
//   --action interrupt  thread/resume -> turn/start {--prompt} -> wait --after seconds -> turn/interrupt,
//                       wait for turn/completed (the graceful interrupt the TUI's Esc performs).
//   --action list       thread/list with scan-and-repair from the rollouts (useStateDbOnly false); no
//                       --thread needed. Prints id, path and preview of each thread.
// Inherits CODEX_HOME etc. from the caller (gen-source.ps1 dot-sources env.ps1 first). Every message in
// both directions is logged to --log.
// Usage: node app-server-drive.mjs --codex <exe> --thread <id> --cwd <dir> --log <file>
//          --action compact | --action interrupt --prompt <text> --after <sec> | --action list
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import fs from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, v, i, a) => (v.startsWith("--") ? [...acc, [v.slice(2), a[i + 1]]] : acc), []),
);
const { codex, thread, cwd, log, action } = args;
if (!codex || !cwd || !log || !["compact", "interrupt", "list"].includes(action) || (!thread && action !== "list")) {
  console.error("usage: node app-server-drive.mjs --codex <exe> [--thread <id>] --cwd <dir> --log <file> --action compact|interrupt|list [--prompt <text> --after <sec>]");
  process.exit(2);
}
fs.writeFileSync(log, "");
const note = (dir, msg) => fs.appendFileSync(log, `${new Date().toISOString()} ${dir} ${JSON.stringify(msg)}\n`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const child = spawn(codex, ["app-server"], { cwd, env: process.env, stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
child.stderr.on("data", (d) => note("stderr", d.toString().trim().slice(0, 2000)));

let nextId = 1;
const pending = new Map();
const waiters = [];
createInterface({ input: child.stdout }).on("line", (line) => {
  let m;
  try {
    m = JSON.parse(line);
  } catch {
    return note("<-", { raw: line.slice(0, 500) });
  }
  note("<-", m);
  if (m.id !== undefined && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  } else if (m.method) {
    for (const w of [...waiters]) if (w.test(m)) { waiters.splice(waiters.indexOf(w), 1); w.resolve(m); }
  }
});

function send(method, params) {
  const id = nextId++;
  const msg = { id, method, params };
  note("->", msg);
  child.stdin.write(JSON.stringify(msg) + "\n");
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => pending.delete(id) && reject(new Error(`timeout: ${method}`)), 60000);
    pending.set(id, (m) => (clearTimeout(t), m.error ? reject(new Error(`${method}: ${JSON.stringify(m.error)}`)) : resolve(m.result)));
  });
}
function waitFor(test, ms, label) {
  return new Promise((resolve, reject) => {
    const w = { test, resolve };
    waiters.push(w);
    setTimeout(() => waiters.includes(w) && (waiters.splice(waiters.indexOf(w), 1), reject(new Error(`timeout waiting for ${label}`))), ms);
  });
}

async function compact() {
  const item = waitFor((m) => m.method === "item/completed" && m.params?.item?.type === "contextCompaction", 120000, "contextCompaction item");
  const done = waitFor((m) => m.method === "turn/completed", 120000, "turn/completed");
  await send("thread/compact/start", { threadId: thread });
  console.log(`compaction item completed: ${JSON.stringify((await item).params.item).slice(0, 300)}`);
  console.log(`compaction turn completed: status=${(await done).params?.turn?.status ?? "?"}`);
}

async function interrupt() {
  const started = waitFor((m) => m.method === "turn/started", 60000, "turn/started");
  const done = waitFor((m) => m.method === "turn/completed", 180000, "turn/completed");
  await send("turn/start", { threadId: thread, input: [{ type: "text", text: args.prompt ?? "H27-HANG", text_elements: [] }] });
  const turnId = (await started).params?.turn?.id;
  console.log(`turn started: ${turnId}; interrupting after ${args.after ?? 15}s`);
  await sleep(Number(args.after ?? 15) * 1000);
  await send("turn/interrupt", { threadId: thread, turnId });
  console.log(`turn completed: status=${(await done).params?.turn?.status ?? "?"}`);
}

async function list() {
  // Default filters first (interactive sources only, so `codex exec` threads are hidden), then every source kind.
  for (const [label, extra] of [["default filters", {}], ["all sources, all providers", { sourceKinds: ["cli", "vscode", "exec", "appServer", "unknown"], modelProviders: [] }]]) {
    const threads = (await send("thread/list", { limit: 50, useStateDbOnly: false, ...extra }))?.data ?? [];
    console.log(`thread/list (${label}): ${threads.length} thread(s)`);
    for (const t of threads) console.log(`  ${t.id} source=${JSON.stringify(t.source)} path=${t.path ?? "?"} preview=${JSON.stringify((t.preview ?? "").slice(0, 60))}`);
  }
}

async function main() {
  await send("initialize", { clientInfo: { name: "h27_codex_driver", title: "H27 bench app-server driver", version: "1.0" }, capabilities: { experimentalApi: true } });
  child.stdin.write(JSON.stringify({ method: "initialized" }) + "\n");
  if (action === "list") return list();
  const resumed = await send("thread/resume", { threadId: thread, cwd });
  console.log(`resumed ${resumed?.thread?.id ?? thread} path=${resumed?.thread?.path ?? "?"}`);
  await (action === "compact" ? compact() : interrupt());
}

main()
  .then(() => (process.exitCode = 0))
  .catch((e) => {
    console.error(`FAIL: ${e.message}`);
    process.exitCode = 1;
  })
  .finally(() => {
    // Close stdin so the app-server flushes and exits; exit explicitly, the request timers would keep node alive.
    child.on("exit", () => process.exit(process.exitCode ?? 0));
    child.stdin.end();
    setTimeout(() => (child.kill(), process.exit(process.exitCode ?? 0)), 5000);
  });
