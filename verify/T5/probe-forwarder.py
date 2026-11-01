"""T5 probe v2 (controlled). One trial = one fresh Claude Code session, small stable prefix, large first user message.

Usage: python -X utf8 -I t5-probe2.py <workdir> <model> <effort,effort,...> <direct|bili> [seed]
Step 1 sends ~8K tokens of filler plus 'Reply with exactly: A'; later steps send one short line.
The call just before the first effort change is the CONTROL (same effort, warm cache). The first call at the new
effort is the FLIP. Output: cache_read / cache_creation for every call, plus verdict inputs.
Prefix flags keep the system prompt + tools around 5K tokens. No API key: logged-in Claude Code.
"""
import json, os, random, subprocess, sys, time

workdir, model, seq, mode = sys.argv[1], sys.argv[2], sys.argv[3].split(","), sys.argv[4]
seed = int(sys.argv[5]) if len(sys.argv) > 5 else 1
os.makedirs(workdir, exist_ok=True)

rnd = random.Random(seed)
words = ["amber", "bridge", "cobalt", "delta", "ember", "fjord", "glacier", "harbor", "indigo", "juniper", "kelp",
         "lantern", "meadow", "nectar", "orchid", "pebble", "quartz", "ridge", "sable", "tundra", "umber", "velvet",
         "willow", "xenon", "yarrow", "zephyr", "anchor", "basalt", "cinder", "dune", "estuary", "flint"]
filler = " ".join(rnd.choice(words) + str(rnd.randint(10, 999)) for _ in range(3200))

env = dict(os.environ)
if mode.startswith("http"):
    for k in ("HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NODE_EXTRA_CA_CERTS"):
        env.pop(k, None)
    env["ANTHROPIC_BASE_URL"] = mode
elif mode == "direct":
    for k in ("HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NODE_EXTRA_CA_CERTS", "ANTHROPIC_BASE_URL"):
        env.pop(k, None)

base = ["claude", "-p", "--model", model, "--output-format", "json", "--strict-mcp-config", "--mcp-config",
        '{"mcpServers":{}}', "--disable-slash-commands", "--tools", "Read", "--setting-sources", "project",
        "--exclude-dynamic-system-prompt-sections"]
prng = random.Random(7)
probs = [tuple(prng.randint(1100, 9899) for _ in range(6)) for _ in range(len(seq))]
sid = None
rows = []
for i, eff in enumerate(seq):
    label = chr(65 + i)
    prompt = (f"Reference text, do not summarize it:\n{filler}\n\nReply with exactly: {label}" if i == 0
              else ("Reply with exactly: " + label if i < 2 else "Without showing work, compute %d*%d + %d*%d - %d*%d and reply with only the number, then the letter %s." % (*probs[i], label)))
    cmd = base + ["--effort", eff] + (["--resume", sid] if sid else [])
    t0 = time.time()
    p = subprocess.run(cmd, input=prompt, cwd=workdir, capture_output=True, text=True, encoding="utf-8",
                       timeout=300, env=env)
    try:
        j = json.loads(p.stdout)
    except Exception:
        print(label, "FAILED rc", p.returncode, (p.stdout or "")[:300], (p.stderr or "")[:300])
        sys.exit(1)
    sid = sid or j.get("session_id")
    u = j.get("usage", {})
    rows.append(dict(label=label, effort=eff, read=u.get("cache_read_input_tokens"),
                     create=u.get("cache_creation_input_tokens"), out=u.get("output_tokens"), think=(u.get("output_tokens_details") or {}).get("thinking_tokens"), ans=str(j.get("result"))[:30],
                     secs=round(time.time() - t0, 1)))
    print(rows[-1], flush=True)

flip = next((k for k in range(1, len(seq)) if seq[k] != seq[k - 1]), None)
if flip is not None and flip >= 2:
    ctl, fl = rows[flip - 1], rows[flip]
    total_ctl = ctl["read"] + ctl["create"]
    print(f"VERDICT-INPUT mode={mode} {seq[flip-1]}->{seq[flip]} control_read={ctl['read']} control_create={ctl['create']} "
          f"flip_read={fl['read']} flip_create={fl['create']} flip_read_share_of_prev_total={fl['read']/total_ctl:.2f}")
