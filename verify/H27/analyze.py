"""H27 capture analyzer: what did the destination model actually receive?

Usage: python -I analyze.py <capture.jsonl> [--marker H27-RESUME] [--json]

Picks the first captured model request whose LAST user turn contains the marker (the resume
prompt), normalises it across the three wire dialects (Anthropic Messages, OpenAI Responses,
Chat Completions) and reports the sequence of items, which scenario markers survived, whether
reasoning payloads were sent back, and whether every tool call is paired with its result.
"""

import json
import sys

MARKERS = [
    "ALPHA-CONTENT-42", "BETA-CONTENT-17", "H27-TOOLS", "H27-PRE", "H27-FINAL", "H27-T2",
    "H27-REPLY", "H27-SUMMARY", "H27-T3", "H27-PARTIAL", "H27-HANG", "H27-REASONING",
    "shinatama handoff",
]


def text_of(x):
    if x is None:
        return ""
    if isinstance(x, str):
        return x
    if isinstance(x, list):
        return "\n".join(text_of(i) for i in x)
    if isinstance(x, dict):
        parts = []
        for k in ("text", "thinking", "content", "output", "arguments", "summary", "input"):
            if k in x:
                v = x[k]
                parts.append(v if isinstance(v, str) else json.dumps(v))
        return "\n".join(parts)
    return str(x)


def items_anthropic(body):
    out = []
    for m in body.get("messages", []):
        c = m.get("content")
        blocks = [{"type": "text", "text": c}] if isinstance(c, str) else (c or [])
        for b in blocks:
            t = b.get("type")
            it = {"role": m.get("role"), "kind": t, "text": text_of(b)}
            if t == "thinking":
                it["opaque"] = bool(b.get("signature"))
            if t == "redacted_thinking":
                it["opaque"] = True
            if t == "tool_use":
                it["call_id"], it["name"] = b.get("id"), b.get("name")
            if t == "tool_result":
                it["call_id"] = b.get("tool_use_id")
            out.append(it)
        out.append({"role": m.get("role"), "kind": "--end-of-message--"})
    return out


def items_responses(body):
    out = []
    inp = body.get("input")
    inp = [{"role": "user", "content": inp}] if isinstance(inp, str) else (inp or [])
    for i in inp:
        t = i.get("type", "message")
        it = {"role": i.get("role", "assistant" if t != "function_call_output" else "tool"), "kind": t, "text": text_of(i)}
        if t == "reasoning":
            it["opaque"] = bool(i.get("encrypted_content"))
            it["text"] = text_of(i.get("summary")) + text_of(i.get("content"))
        if t in ("function_call", "custom_tool_call", "local_shell_call"):
            it["call_id"], it["name"] = i.get("call_id"), i.get("name")
        if t in ("function_call_output", "custom_tool_call_output"):
            it["call_id"] = i.get("call_id")
        out.append(it)
        out.append({"role": it["role"], "kind": "--end-of-message--"})
    return out


def items_chat(body):
    out = []
    for m in body.get("messages", []):
        out.append({"role": m.get("role"), "kind": "text", "text": text_of(m.get("content"))})
        if m.get("reasoning_content"):
            out.append({"role": m.get("role"), "kind": "reasoning", "text": m["reasoning_content"]})
        for tc in m.get("tool_calls") or []:
            out.append({"role": "assistant", "kind": "tool_use", "call_id": tc.get("id"), "name": tc.get("function", {}).get("name"), "text": tc.get("function", {}).get("arguments", "")})
        if m.get("role") == "tool":
            out[-1].update(kind="tool_result", call_id=m.get("tool_call_id"))
        out.append({"role": m.get("role"), "kind": "--end-of-message--"})
    return out


def dialect(entry):
    url = entry.get("url", "")
    if "/messages" in url and "count_tokens" not in url:
        return "anthropic"
    if "/responses" in url:
        return "responses"
    if "/chat/completions" in url:
        return "chat"
    return None


def last_user_text(items):
    for it in reversed(items):
        if it.get("role") == "user" and it["kind"] not in ("--end-of-message--",):
            return it.get("text", "")
    return ""


def analyze(entry):
    body = entry.get("body") or {}
    d = dialect(entry)
    items = {"anthropic": items_anthropic, "responses": items_responses, "chat": items_chat}[d](body)
    real = [i for i in items if i["kind"] != "--end-of-message--"]
    all_text = "\n".join(i.get("text", "") for i in real) + "\n" + text_of(body.get("system")) + text_of(body.get("instructions"))
    calls = [i for i in real if i["kind"] in ("tool_use", "function_call", "custom_tool_call", "local_shell_call")]
    results = [i for i in real if i["kind"] in ("tool_result", "function_call_output", "custom_tool_call_output")]
    call_ids = [c.get("call_id") for c in calls]
    result_ids = [r.get("call_id") for r in results]
    # parallel grouping: two calls inside one assistant message (anthropic/chat) or adjacent items (responses)
    groups, cur = [], 0
    for i in items:
        if i["kind"] in ("tool_use", "function_call"):
            cur += 1
        elif i["kind"] == "--end-of-message--" and d != "responses":
            if cur:
                groups.append(cur)
            cur = 0
        elif d == "responses" and i["kind"] not in ("--end-of-message--",):
            if cur:
                groups.append(cur)
            cur = 0
    if cur:
        groups.append(cur)
    seq = []
    for i in real:
        tag = f"{(i.get('role') or '?')[0]}:{i['kind']}"
        if i["kind"] in ("thinking", "reasoning"):
            tag += "+opaque" if i.get("opaque") else "-plain"
        seq.append(tag)
    return {
        "n": entry.get("n"),
        "dialect": d,
        "model": body.get("model"),
        "items": len(real),
        "sequence": " ".join(seq),
        "markers": {m: (m in all_text) for m in MARKERS},
        "reasoning_items": sum(1 for i in real if i["kind"] in ("thinking", "reasoning", "redacted_thinking")),
        "reasoning_opaque_sent": sum(1 for i in real if i.get("opaque")),
        "tool_calls": len(calls),
        "tool_results": len(results),
        "unpaired_calls": [c for c in call_ids if c not in result_ids],
        "orphan_results": [r for r in result_ids if r not in call_ids],
        "parallel_groups": groups,
        "tool_names": sorted({c.get("name") or "" for c in calls}),
        "last_user_text": last_user_text(items)[:300],
    }


def main():
    path = sys.argv[1]
    marker = sys.argv[sys.argv.index("--marker") + 1] if "--marker" in sys.argv else "H27-RESUME"
    entries = [json.loads(l) for l in open(path, encoding="utf-8") if l.strip()]
    hits = []
    for e in entries:
        if not dialect(e) or not isinstance(e.get("body"), dict):
            continue
        r = analyze(e)
        if marker in r["last_user_text"]:
            hits.append(r)
    if not hits:
        print(json.dumps({"capture": path, "error": f"no request whose last user turn contains {marker}", "requests": len(entries)}))
        sys.exit(1)
    r = hits[0]
    r["capture"] = path
    r["requests_total"] = len(entries)
    print(json.dumps(r, indent=None if "--json" in sys.argv else 2))


if __name__ == "__main__":
    main()
