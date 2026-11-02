"""TE13 -- Shinatama's own tool-pruning evidence for OpenCode (ADR-0045 v2, rule 4).

TokenEconomizer skips its pruning stage on OpenCode (TE12), but its plugin still exports every
session to `<TOKENECONOMIZER_HOME>/opencode-sessions/<id>.json`. This script computes the
proposal TokenEconomizer would make, with its own rule and thresholds, and turns it into
`permissions` deny rules for the instance's `OPENCODE_CONFIG`.

    python opencode_prune.py propose --exports DIR --catalog FILE|--capture FILE [--out FILE]
    python opencode_prune.py apply   --proposal FILE --config opencode.json
    python opencode_prune.py selftest

Rule (TokenEconomizer `plugin/fold.py::enough_evidence`, defaults from `plugin/config.py`):
a never-used tool is proposed when it stayed unused over EVIDENCE prompts spread across at least
MIN_SESSIONS sessions, or over SESSIONS sessions however short. A session counts once; a prompt
is a user message in the export; a tool counts as used directly or through Code Mode
(`state.metadata.toolCalls` of `execute`). The catalog comes from `--catalog`: under Code Mode,
OpenCode's default for MCP servers, MCP tools are not in the request's tool list, so `--capture`
sees them only when the server sets `codemode: false`. Protected names are never proposed: OpenCode's work surface
and a person's tools, every MCP tool of BillionContext (`bili_*`), and whatever the caller adds.
Standard library only.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import pathlib
import re
import sys

EVIDENCE, MIN_SESSIONS, SESSIONS = 10, 2, 5  # TokenEconomizer plugin/config.py:153-155

# OpenCode 2.2.5 names. The work surface (shell, read, edit, write, patch, glob, grep, list),
# a person's tools (question, todo*, webfetch, websearch), orchestration (subagent, skill) and
# Code Mode (`execute`: every tool reachable only through it) are never proposed; this mirrors
# TokenEconomizer's PROTECTED_TOOLS and INTERACTIVE_PROTECTED for Claude Code.
PROTECTED = frozenset({
    "shell", "bash", "read", "edit", "write", "patch", "apply_patch", "glob", "grep", "list",
    "question", "todowrite", "todoread", "webfetch", "websearch",
    "subagent", "task", "skill", "execute",
})
PROTECTED_PATTERNS = ("bili_*", "mcp__bili__*")


def permission_key(name: str) -> str:
    """`acme.lookup` (Code Mode path) -> `acme_lookup` (permission key, `<server>_<tool>`)."""
    return re.sub(r"[^A-Za-z0-9_-]", "_", name)


def load_exports(exports_dir: pathlib.Path):
    sessions = []
    for path in sorted(exports_dir.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict) or "messages" not in data:
            continue  # not a session export
        prompts, used = 0, set()
        for msg in data.get("messages", []):
            info = msg.get("info") or {}
            if info.get("role") == "user":
                prompts += 1
            for part in msg.get("parts") or []:
                if part.get("type") == "tool" and part.get("tool"):
                    used.add(part["tool"])
                    # Code Mode: an MCP call is recorded as `execute`, its real target only here.
                    for call in ((part.get("state") or {}).get("metadata") or {}).get("toolCalls") or []:
                        if call.get("tool"):
                            used.add(permission_key(call["tool"]))
        sessions.append({"id": data.get("info", {}).get("id", path.stem),
                         "file": path.name, "prompts": prompts, "used": sorted(used)})
    return sessions


def load_catalog(catalog: pathlib.Path | None, capture: pathlib.Path | None):
    if catalog:
        data = json.loads(catalog.read_text(encoding="utf-8"))
        return sorted(set(data if isinstance(data, list) else data.get("tools", [])))
    names = set()
    for line in capture.read_text(encoding="utf-8").splitlines():
        if line.strip():
            names.update(json.loads(line).get("tools") or [])
    return sorted(names)


def protected(name: str, extra=()) -> bool:
    return name in PROTECTED or any(fnmatch.fnmatchcase(name, p) for p in PROTECTED_PATTERNS + tuple(extra))


def enough_evidence(sessions: int, prompts: int) -> bool:
    return (sessions >= MIN_SESSIONS and prompts >= EVIDENCE) or sessions >= SESSIONS


def propose(exports_dir, catalog, capture, extra_protect=()):
    sessions = load_exports(exports_dir)
    names = load_catalog(catalog, capture)
    n_sessions = len(sessions)
    n_prompts = sum(s["prompts"] for s in sessions)
    used = sorted({t for s in sessions for t in s["used"]})
    never_used = [t for t in names if t not in used]
    kept_protected = [t for t in never_used if protected(t, extra_protect)]
    candidates = [t for t in never_used if not protected(t, extra_protect)]
    enough = enough_evidence(n_sessions, n_prompts)
    return {
        "rule": {"evidence_prompts": EVIDENCE, "min_sessions": MIN_SESSIONS, "sessions": SESSIONS},
        "evidence": {"sessions": n_sessions, "prompts": n_prompts, "enough": enough,
                     "per_session": sessions},
        "catalog": names, "used_tools": used,
        "never_used": never_used, "protected_kept": kept_protected,
        "candidates": candidates,
        "proposal": candidates if enough else [],
    }


def apply(proposal: dict, config_path: pathlib.Path):
    """Write the proposal as deny rules. `Permission.Rule` admits no extra key, so the rules this
    script owns are named in a sidecar and replaced on the next apply; a person's rules stay."""
    cfg = json.loads(config_path.read_text(encoding="utf-8"))
    sidecar = config_path.with_name(config_path.name + ".toolprune.json")
    owned = set(json.loads(sidecar.read_text(encoding="utf-8"))) if sidecar.exists() else set()
    rules = [r for r in cfg.get("permissions", []) if not (r.get("effect") == "deny" and r.get("action") in owned)]
    rules.extend({"action": t, "resource": "*", "effect": "deny"} for t in proposal["proposal"])
    cfg["permissions"] = rules
    config_path.write_text(json.dumps(cfg, indent=1) + chr(10), encoding="utf-8")
    sidecar.write_text(json.dumps(proposal["proposal"]) + chr(10), encoding="utf-8")
    return rules


def selftest():
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        d = pathlib.Path(tmp)
        for i in range(5):
            (d / f"s{i}.json").write_text(json.dumps({"info": {"id": f"s{i}"}, "messages": [
                {"info": {"role": "user"}, "parts": [{"type": "text", "text": "x"}]},
                {"info": {"role": "assistant"}, "parts": [{"type": "tool", "tool": "shell"}]}]}),
                encoding="utf-8")
        cat = d / "cat.json"
        cat.write_text(json.dumps(["shell", "read", "bili_compress", "bili_acp_status",
                                   "mcp__bili__decompress", "experimental_image_generate",
                                   "acme_lookup"]), encoding="utf-8")
        p = propose(d, cat, None)
        assert p["evidence"]["enough"], p["evidence"]
        assert p["proposal"] == ["acme_lookup", "experimental_image_generate"], p["proposal"]
        assert set(p["protected_kept"]) == {"read", "bili_compress", "bili_acp_status", "mcp__bili__decompress"}
        # An MCP tool used only through Code Mode counts as used, under its permission key.
        (d / "s0.json").write_text(json.dumps({"info": {"id": "s0"}, "messages": [
            {"info": {"role": "user"}, "parts": []},
            {"info": {"role": "assistant"}, "parts": [{"type": "tool", "tool": "execute", "state": {
                "metadata": {"toolCalls": [{"tool": "acme.lookup", "status": "completed"}]}}}]}]}), encoding="utf-8")
        assert propose(d, cat, None)["proposal"] == ["experimental_image_generate"]
        # Four sessions, four prompts: not enough under either arm.
        (d / "s4.json").unlink()
        assert not propose(d, cat, None)["evidence"]["enough"]
        assert propose(d, cat, None)["proposal"] == []
        # apply replaces its own rules and keeps a person's.
        conf = d / "opencode.json"
        conf.write_text(json.dumps({"permissions": [{"action": "shell", "resource": "git push*", "effect": "deny"}]}), encoding="utf-8")
        apply({"proposal": ["a", "b"]}, conf)
        apply({"proposal": ["b"]}, conf)
        got = json.loads(conf.read_text(encoding="utf-8"))["permissions"]
        assert [r["action"] for r in got] == ["shell", "b"], got
    print("selftest ok")


def main(argv=None):
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("propose")
    p.add_argument("--exports", required=True, type=pathlib.Path)
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--catalog", type=pathlib.Path)
    g.add_argument("--capture", type=pathlib.Path)
    p.add_argument("--protect", nargs="*", default=())
    p.add_argument("--out", type=pathlib.Path)
    a = sub.add_parser("apply")
    a.add_argument("--proposal", required=True, type=pathlib.Path)
    a.add_argument("--config", required=True, type=pathlib.Path)
    sub.add_parser("selftest")
    args = ap.parse_args(argv)
    if args.cmd == "selftest":
        return selftest()
    if args.cmd == "propose":
        result = propose(args.exports, args.catalog, args.capture, tuple(args.protect))
        text = json.dumps(result, indent=1)
        if args.out:
            args.out.write_text(text + "\n", encoding="utf-8")
        print(text)
        return 0
    rules = apply(json.loads(args.proposal.read_text(encoding="utf-8")), args.config)
    print(json.dumps(rules, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
