"""H27 policy variants over a txcript Common dump ({"meta", "messages"}).

Usage: python -I transform.py <common.json> <full|portable|tail> <out.json> [--cwd DIR]

full      the history exactly as txcript read it
portable  compaction carried portably: one user message holding the source's summary, marked
          `[shinatama handoff v1]`, then the tail that followed the source's compaction
          (from the `H27-T3` turn on). In the library the summary and the boundary come from the
          source's native compaction record; here the bench scenario fixes them.
tail      no summary: a marker message saying how many messages were left out, then the tail
          from the `H27-T2` turn on.
Each variant gets a fresh session id so writing several variants into one store never collides.
--cwd sets the session's working directory (a hop keeps the project; the bench's harnesses each
have their own work folder, and a destination resumes by id from its own folder).
"""

import json
import sys
import uuid

SUMMARY = (
    "H27-SUMMARY: the user asked to read alpha.txt and beta.txt; both were read in parallel "
    "(ALPHA-CONTENT-42, BETA-CONTENT-17); then the user said hello."
)


def first_index(messages, needle, role="user"):
    for i, m in enumerate(messages):
        if m.get("role") != role:
            continue
        for b in m.get("content", []):
            if b.get("type") == "text" and needle in b.get("text", ""):
                return i
    return None


def user_text(text, ts):
    return {"role": "user", "content": [{"type": "text", "text": text}], "timestamp": ts}


def main():
    src, variant, out = sys.argv[1], sys.argv[2], sys.argv[3]
    doc = json.load(open(src, encoding="utf-8-sig"))
    msgs = doc["messages"]
    meta = dict(doc["meta"])
    meta["id"] = str(uuid.uuid4())
    meta["title"] = f"H27 {variant} ({meta.get('title') or 'source'})"
    meta.pop("lineage", None)
    if "--cwd" in sys.argv:
        meta["cwd"] = sys.argv[sys.argv.index("--cwd") + 1]

    if variant == "full":
        new = msgs
    elif variant == "portable":
        cut = first_index(msgs, "H27-T3")
        if cut is None:
            sys.exit(f"{src}: no H27-T3 turn, cannot place the compaction boundary")
        head = user_text(
            "[shinatama handoff v1] Summary of the earlier conversation, written by the source "
            "agent's own compaction:\n" + SUMMARY,
            msgs[cut]["timestamp"],
        )
        new = [head] + msgs[cut:]
    elif variant == "tail":
        cut = first_index(msgs, "H27-T2")
        if cut is None:
            sys.exit(f"{src}: no H27-T2 turn")
        head = user_text(f"[shinatama handoff v1] {cut} earlier messages were left out of this copy.", msgs[cut]["timestamp"])
        new = [head] + msgs[cut:]
    else:
        sys.exit(f"unknown variant {variant}")

    json.dump({"meta": meta, "messages": new}, open(out, "w", encoding="utf-8"), indent=1)
    print(f"{variant}: {len(msgs)} -> {len(new)} messages -> {out}")


if __name__ == "__main__":
    main()
