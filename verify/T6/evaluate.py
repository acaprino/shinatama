"""Apply the T6 pass rule (see probe.py) to results.jsonl and print the per-model table.

Excluded, with the reason printed: trials with an errored turn, and trials named in EXCLUDE.
"""
import json
import pathlib

EXCLUDE = {("haiku", "enabled", 1): "same filler as the pilot, so it read the pilot's cache"}
rows = [json.loads(l) for l in open(pathlib.Path(__file__).with_name("results.jsonl"), encoding="utf-8")]
valid = []
for r in rows:
    key = (r["model"], r["arm"], r["trial"])
    if key in EXCLUDE:
        print("excluded", key, EXCLUDE[key])
    elif any(x.get("is_error") or "error" in x for x in r["rows"]):
        print("excluded", key, "errored turns", [x["turn"] for x in r["rows"] if x.get("is_error")])
    else:
        valid.append(r)


def hits(model, arm, turn):
    return [next(x["hit"] for x in r["rows"] if x["turn"] == turn) for r in valid
            if r["model"] == model and r["arm"] == arm]


for model in ("sonnet", "haiku", "opus", "fable"):
    n = {a: len([r for r in valid if r["model"] == model and r["arm"] == a]) for a in ("control", "steered", "enabled")}
    worst = {t: min(hits(model, "control", t)) for t in (3, 4, 5, 6)}
    ok = True
    for arm, turns in (("steered", (3, 4, 5, 6)), ("enabled", (4, 5, 6))):
        for t in turns:
            ok &= min(hits(model, arm, t)) >= worst[t] - 0.01
    fmt = lambda arm, turns: f"{min(min(hits(model, arm, t)) for t in turns):.4f}"
    made = [x["created"] for r in valid if r["model"] == model and r["arm"] == "enabled" for x in r["rows"] if x["turn"] == 3]
    read = [x["read"] for r in valid if r["model"] == model and r["arm"] == "enabled" for x in r["rows"] if x["turn"] == 3]
    steered_turns = [x["steered_requests"] for r in valid if r["arm"] != "control" and r["model"] == model
                     for x in r["rows"] if x["turn"] >= 3]
    print(f"{model}: trials {n} control worst t3-6 {fmt('control', (3, 4, 5, 6))} steered worst t3-6 {fmt('steered', (3, 4, 5, 6))} "
          f"enabled worst t4-6 {fmt('enabled', (4, 5, 6))} miss read {read} created {made} "
          f"directive on every later turn {all(s >= 1 for s in steered_turns)} pass {ok}")
