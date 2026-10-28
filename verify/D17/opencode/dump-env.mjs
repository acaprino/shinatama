// D17/P20: run inside an agent's shell tool (or a plain child) to record what that child sees.
//   node <this file> <out.json>
// Writes: every environment variable NAME (values never, except the allowlist below, which only holds
// paths and agent-marker flags), git's config files as `git config --show-origin --list --name-only`
// (names + origin files; no values) and which config files npm would read.
import fs from "node:fs";
import { execFileSync } from "node:child_process";

const out = process.argv[2];
if (!out) throw new Error("usage: node dump-env.mjs <out.json>");
const VALUE_ALLOW = /^(XDG_[A-Z_]+|HOME|USERPROFILE|APPDATA|LOCALAPPDATA|TEMP|TMP|GIT_CONFIG_GLOBAL|GIT_CONFIG_SYSTEM|GIT_CONFIG_NOSYSTEM|OPENCODE|OPENCODE_TERMINAL|OPENCODE_SESSION_ID|OPENCODE_CLIENT|OPENCODE_DB|OPENCODE_TEST_HOME|OPENCODE_CONFIG_DIR|OPENCODE_CONFIG|AGENT|AI_AGENT|CLAUDECODE|CLAUDE_CODE_ENTRYPOINT|TERM)$/;
function run(cmd, args) {
  try {
    return { ok: true, out: execFileSync(cmd, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 20000 }) };
  } catch (e) {
    return { ok: false, code: e.status ?? null, out: String(e.stdout ?? ""), err: String(e.stderr ?? e.message).slice(0, 400) };
  }
}
const names = Object.keys(process.env).sort((a, b) => a.localeCompare(b));
const values = Object.fromEntries(names.filter((n) => VALUE_ALLOW.test(n)).map((n) => [n, process.env[n]]));
// --name-only: names of keys with their origin file; never the values.
const git = run("git", ["config", "--show-origin", "--list", "--name-only"]);
const gitOrigins = {};
for (const line of git.out.split(/\r?\n/).filter(Boolean)) {
  const [origin, key] = line.split("\t");
  (gitOrigins[origin] ??= []).push(key);
}
const npm = (what) => (process.platform === "win32" ? run("cmd.exe", ["/d", "/s", "/c", `npm config get ${what}`]) : run("npm", ["config", "get", what]));
const npmUser = npm("userconfig");
const npmCache = npm("cache");
const result = {
  pid: process.pid,
  ppid: process.ppid,
  cwd: process.cwd(),
  nameCount: names.length,
  names,
  values,
  git: { ok: git.ok, code: git.code, err: git.err, originCounts: Object.fromEntries(Object.entries(gitOrigins).map(([k, v]) => [k, v.length])), keysByOrigin: gitOrigins },
  npm: { userconfig: npmUser.out.trim() || npmUser.err, cache: npmCache.out.trim() || npmCache.err },
};
fs.writeFileSync(out, JSON.stringify(result, null, 2));
console.log(`SHN-DUMP written ${out} names=${names.length} gitOrigins=${Object.keys(gitOrigins).length}`);
