"""H27 tool census: which tool names and argument keys the four harnesses really write.

Reads the user's real stores READ-ONLY and prints STRUCTURE ONLY (tool name, sorted argument key set,
count, newest harness version seen); no argument values, no text. Rerunnable.

  Claude Code  ~/.claude/projects/*/*.jsonl            assistant content[] tool_use {name, input}
  Codex        ~/.codex/sessions/**/rollout-*.jsonl     response_item function_call / custom_tool_call
  OpenCode     ~/.local/share/opencode/opencode*.db     session_message.data content[] type "tool"
  Pi           ~/.pi/agent/sessions/**/*.jsonl          message.content[] type "toolCall"

Usage: python -I tool-census.py [--max-files N] > out/<date>/tool-census.json
"""

import collections
import glob
import json
import os
import sqlite3
import sys

HOME = os.path.expanduser("~")
MAX = int(sys.argv[sys.argv.index("--max-files") + 1]) if "--max-files" in sys.argv else 400


def newest(paths):
    return sorted(paths, key=lambda p: os.path.getmtime(p), reverse=True)[:MAX]


def keys_of(v):
    if isinstance(v, str):
        try:
            v = json.loads(v)
        except Exception:
            return "<freeform>"
    return ",".join(sorted(v.keys())) if isinstance(v, dict) else f"<{type(v).__name__}>"


def add(c, name, keys, version):
    k = (name, keys)
    c[k]["n"] += 1
    if version and version > c[k]["version"]:
        c[k]["version"] = version


def census_claude():
    c = collections.defaultdict(lambda: {"n": 0, "version": ""})
    files = newest(glob.glob(os.path.join(HOME, ".claude", "projects", "*", "*.jsonl")))
    for f in files:
        for line in open(f, encoding="utf-8", errors="replace"):
            if '"tool_use"' not in line:
                continue
            try:
                r = json.loads(line)
            except Exception:
                continue
            for b in (r.get("message") or {}).get("content") or []:
                if isinstance(b, dict) and b.get("type") == "tool_use":
                    name = b.get("name", "")
                    if name.startswith("mcp__"):
                        name = "mcp__*"
                    add(c, name, keys_of(b.get("input")) if name != "mcp__*" else "*", r.get("version", ""))
    return len(files), c


def census_codex():
    c = collections.defaultdict(lambda: {"n": 0, "version": ""})
    files = newest(glob.glob(os.path.join(HOME, ".codex", "sessions", "**", "rollout-*.jsonl"), recursive=True))
    for f in files:
        version = ""
        for line in open(f, encoding="utf-8", errors="replace"):
            if '"session_meta"' in line[:80]:
                try:
                    version = json.loads(line)["payload"].get("cli_version", "")
                except Exception:
                    pass
            if '"function_call"' not in line and '"custom_tool_call"' not in line:
                continue
            try:
                p = json.loads(line).get("payload") or {}
            except Exception:
                continue
            if p.get("type") == "function_call":
                add(c, p.get("name", ""), keys_of(p.get("arguments")), version)
            elif p.get("type") == "custom_tool_call":
                add(c, p.get("name", "") + " (custom)", "<freeform>", version)
    return len(files), c


def census_opencode():
    c = collections.defaultdict(lambda: {"n": 0, "version": ""})
    dbs = [p for p in glob.glob(os.path.join(HOME, ".local", "share", "opencode", "opencode*.db"))]
    for db in dbs:
        con = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
        try:
            versions = dict(con.execute("select id, version from session_v2"))
            rows = con.execute("select session_id, data from session_message where type='assistant'")
        except sqlite3.Error:
            continue
        for sid, data in rows:
            try:
                d = json.loads(data)
            except Exception:
                continue
            for part in d.get("content") or []:
                if isinstance(part, dict) and part.get("type") == "tool":
                    name = part.get("name", "")
                    if name.startswith("mcp") or "_" in name and name.split("_")[0] not in ("todo",):
                        pass
                    add(c, name, keys_of((part.get("state") or {}).get("input")), versions.get(sid, ""))
        con.close()
    return len(dbs), c


def census_pi():
    c = collections.defaultdict(lambda: {"n": 0, "version": ""})
    files = newest(glob.glob(os.path.join(HOME, ".pi", "agent", "sessions", "**", "*.jsonl"), recursive=True))
    for f in files:
        for line in open(f, encoding="utf-8", errors="replace"):
            if '"toolCall"' not in line:
                continue
            try:
                m = json.loads(line).get("message") or {}
            except Exception:
                continue
            for b in m.get("content") or []:
                if isinstance(b, dict) and b.get("type") == "toolCall":
                    add(c, b.get("name", ""), keys_of(b.get("arguments")), "")
    return len(files), c


def main():
    out = {}
    for h, fn in (("claude_code", census_claude), ("codex", census_codex), ("opencode", census_opencode), ("pi", census_pi)):
        try:
            n, c = fn()
        except Exception as e:  # a missing store is a result, not a crash
            out[h] = {"error": str(e)}
            continue
        rows = sorted(({"tool": k[0], "keys": k[1], **v} for k, v in c.items()), key=lambda r: -r["n"])
        out[h] = {"sources_read": n, "tools": rows}
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
