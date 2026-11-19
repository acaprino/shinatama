"""H27 strict check: would a real provider accept the request the destination sent?

The mock answers anything; the real APIs do not. This checks each resume request of the matrix
(verify/H27/out/2026-10-27/matrix/results.jsonl) against the pairing rules the providers enforce:

Anthropic Messages
  A1  every tool_use in an assistant message has a tool_result in the NEXT message (a user message)
  A2  every tool_result answers a tool_use of the PREVIOUS assistant message
  A3  a tool_use id is answered at most once
  A4  a thinking block carries a signature (the API refuses `thinking` without one)
OpenAI Responses
  R1  every function_call has a function_call_output with its call_id
  R2  every function_call_output follows a function_call with its call_id
  R3  (warning) a reasoning item without encrypted_content under store:false cannot be replayed

Usage: python -I strict.py [results.jsonl] [--json]
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def blocks(m):
    c = m.get("content")
    return [{"type": "text", "text": c}] if isinstance(c, str) else (c or [])


def check_anthropic(body):
    v = []
    msgs = [m for m in body.get("messages", []) if m.get("role") in ("user", "assistant")]
    answered = set()
    for i, m in enumerate(msgs):
        bs = blocks(m)
        if m["role"] == "assistant":
            for b in bs:
                if b.get("type") == "thinking" and not b.get("signature"):
                    v.append(f"A4 msg{i}: thinking without signature")
            uses = [b.get("id") for b in bs if b.get("type") == "tool_use"]
            if uses:
                nxt = msgs[i + 1] if i + 1 < len(msgs) else None
                got = [b.get("tool_use_id") for b in blocks(nxt)] if nxt and nxt["role"] == "user" else []
                for u in uses:
                    if u not in got:
                        v.append(f"A1 msg{i}: tool_use {u} has no tool_result in the next message")
        else:
            prev = msgs[i - 1] if i > 0 else None
            prev_uses = {b.get("id") for b in blocks(prev) if b.get("type") == "tool_use"} if prev and prev["role"] == "assistant" else set()
            for b in bs:
                if b.get("type") != "tool_result":
                    continue
                t = b.get("tool_use_id")
                if t not in prev_uses:
                    v.append(f"A2 msg{i}: tool_result {t} does not answer the previous assistant message")
                if t in answered:
                    v.append(f"A3 msg{i}: tool_use {t} answered twice")
                answered.add(t)
    return v, []


def check_responses(body):
    v, w = [], []
    items = body.get("input") or []
    if isinstance(items, str):
        return v, w
    calls, outs = [], set()
    for i, it in enumerate(items):
        t = it.get("type")
        if t in ("function_call", "custom_tool_call"):
            calls.append(it.get("call_id"))
        elif t in ("function_call_output", "custom_tool_call_output"):
            if it.get("call_id") not in calls:
                v.append(f"R2 item{i}: output {it.get('call_id')} has no earlier call")
            outs.add(it.get("call_id"))
        elif t == "reasoning" and not it.get("encrypted_content") and body.get("store") is False:
            w.append(f"R3 item{i}: reasoning without encrypted_content (store:false)")
    for c in calls:
        if c not in outs:
            v.append(f"R1: function_call {c} has no output")
    return v, w


def first_resume_request(path, marker="H27-RESUME"):
    for line in open(path, encoding="utf-8"):
        e = json.loads(line)
        b = e.get("body")
        if not isinstance(b, dict) or "count_tokens" in e.get("url", ""):
            continue
        if marker + " [" in json.dumps(b.get("messages") or b.get("input") or ""):
            return e
    return None


def main():
    res = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else os.path.join(HERE, "out", "2026-10-27", "matrix", "results.jsonl")
    out = []
    for line in open(res, encoding="utf-8-sig"):
        r = json.loads(line)
        tag = f"{r['variant']}-from-{r['source']}"
        cap = os.path.join(HERE, "out", "2026-10-27", r["dest"], "resume", f"{tag}.capture.jsonl")
        row = {"source": r["source"], "dest": r["dest"], "variant": r["variant"]}
        e = first_resume_request(cap) if os.path.exists(cap) else None
        if not e:
            row["error"] = "no resume request"
        else:
            body = e["body"]
            v, w = check_responses(body) if "/responses" in e["url"] else check_anthropic(body)
            row.update(violations=v, warnings=w)
        out.append(row)
    if "--json" in sys.argv:
        print(json.dumps(out, indent=1))
        return
    for r in out:
        status = "ERROR " + r["error"] if "error" in r else ("REJECT" if r["violations"] else "ok") + (f" ({len(r['warnings'])} warn)" if r.get("warnings") else "")
        print(f"{r['source']:>11}>{r['dest']:<11} {r['variant']:<8} {status}")
        for x in r.get("violations", []) + r.get("warnings", []):
            print("      ", x)


if __name__ == "__main__":
    main()
