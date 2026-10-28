"""Print one line per matrix result (verify/H27/out/2026-10-27/matrix/results.jsonl)."""
import json, sys
path = sys.argv[1] if len(sys.argv) > 1 else "results.jsonl"
for l in open(path, encoding="utf-8-sig"):
    r = json.loads(l)
    a = r.get("analysis")
    try:
        a = json.loads(a) if a else {}
    except Exception:
        a = {"error": a[:80]}
    mk = a.get("markers", {})
    yn = lambda k: "Y" if mk.get(k) else "-"
    err = a.get("error") or ("" if r.get("id") else r.get("write", "")[-160:].replace("\n", " "))
    print(f'{r["source"]:>11}>{r["dest"]:<11} {r["variant"]:<8} items={a.get("items")} reas={a.get("reasoning_items")}/{a.get("reasoning_opaque_sent")} '
          f'calls={a.get("tool_calls")}/{a.get("tool_results")} unp={len(a.get("unpaired_calls") or [])} grp={a.get("parallel_groups")} '
          f'A={yn("ALPHA-CONTENT-42")} PRE={yn("H27-PRE")} FIN={yn("H27-FINAL")} T2={yn("H27-T2")} SUM={yn("H27-SUMMARY")} T3={yn("H27-T3")} HANG={yn("H27-HANG")} REAS={yn("H27-REASONING")} HO={yn("shinatama handoff")} {err}')
