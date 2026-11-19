"""T6: does the output-steering directive keep the prompt cache stable on Claude Code?

    python -X utf8 -I probe.py <control-config> <steered-config> <outdir> <model> <trials> <arm,arm,...>

Arms, one fresh Claude Code session and one temporary sidecar per trial, six turns each:
  control  no steering
  steered  steering from turn 1
  enabled  no steering on turns 1-2; the config is switched and reloaded before turn 3

The sidecar is the installed one, with its own state folder, so live sessions are not touched.
Trials run round-robin across arms to reduce drift over time. Logged-in Claude Code, no API key.

Fixed before the first run. Metric: hit rate = read / (read + created + input), per turn.
Pass: over turns 4-6 (turn 3 too, for `steered`), each steered or enabled trial's hit rate is at
least the lowest control hit rate of the same turn minus 1 percentage point. The turn where
steering is enabled may miss, and the size of that miss is recorded. A control turn that itself
misses, or a turn that ended in an error, makes the trial inconclusive rather than a pass.
"""
import json
import os
import pathlib
import random
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

CONTROL, STEERED, OUT, MODEL, TRIALS, ARMS = sys.argv[1:7]
TRIALS, ARMS, OUT = int(TRIALS), ARMS.split(","), pathlib.Path(OUT)
INDEX = os.environ["T6_BILI_INDEX"]  # the installed sidecar's dist/index.js
ROOT_VAR = os.environ["T6_BILI_ROOT_VAR"]  # the build's private-root variable, pointed at a temporary folder
CWD = pathlib.Path(r"D:\tmp-te\steer-test\wd")
CWD.mkdir(parents=True, exist_ok=True)
OUT.mkdir(parents=True, exist_ok=True)
DROP = ("https_proxy", "http_proxy", "all_proxy", "node_extra_ca_certs", "anthropic_base_url", "billion_context_proxy")
BASE_ENV = {k: v for k, v in os.environ.items() if k.lower() not in DROP}
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))
WORDS = ("amber bridge cobalt delta ember fjord glacier harbor indigo juniper kelp lantern meadow nectar orchid "
         "pebble quartz ridge sable tundra umber velvet willow xenon yarrow zephyr anchor basalt cinder dune").split()


def start(root, config, port):
    shutil.rmtree(root, ignore_errors=True)
    root.mkdir(parents=True)
    shutil.copy(config, root / "config.json")
    env = dict(BASE_ENV, ACP_PORT=str(port), ACP_HOST="127.0.0.1", BILI_CONFIG_FILE=str(root / "config.json"),
               BILI_SESSIONS_DIR=str(root / "sessions"), ACP_LOG_FILE=str(root / "bili.log"),
               ACP_RAW_DUMP_DIR=str(root / "raw"), ACP_DUMP_DIR=str(root / "dumps"), ACP_AUTO_UPDATE="0",
               NO_PROXY="127.0.0.1,localhost", **{ROOT_VAR: str(root / "root")})
    proc = subprocess.Popen(["node", INDEX, "start"], env=env, stdin=subprocess.DEVNULL,
                            stdout=open(root / "out.txt", "wb"), stderr=subprocess.STDOUT)
    for _ in range(80):
        try:
            OPENER.open(f"http://127.0.0.1:{port}/__bili/stats", timeout=2)
            return proc
        except urllib.error.HTTPError:
            return proc
        except Exception:
            time.sleep(0.5)
    raise RuntimeError("sidecar did not start")


def steered_count(root):
    path = root / "bili.log"
    if not path.exists():
        return 0
    return path.read_text(encoding="utf-8", errors="replace").count("[output-steering] applied")


def session(arm, trial, port):
    root = OUT / f"{MODEL}-{arm}-{trial}"
    seed = sum(map(ord, f"{MODEL}{arm}{trial}"))
    rnd = random.Random(seed)
    filler = " ".join(rnd.choice(WORDS) + str(rnd.randint(10, 999)) for _ in range(3200))
    proc = start(root, STEERED if arm == "steered" else CONTROL, port)
    env = dict(BASE_ENV, ANTHROPIC_BASE_URL=f"http://127.0.0.1:{port}")
    base = ["claude", "-p", "--model", MODEL, "--effort", "high", "--output-format", "json", "--strict-mcp-config",
            "--mcp-config", '{"mcpServers":{}}', "--disable-slash-commands", "--tools", "Read",
            "--setting-sources", "project", "--exclude-dynamic-system-prompt-sections"]
    sid, rows = None, []
    try:
        for turn in range(1, 7):
            if arm == "enabled" and turn == 3:
                shutil.copy(STEERED, root / "config.json")
                req = urllib.request.Request(f"http://127.0.0.1:{port}/__bili/config/reload", b"", method="POST")
                OPENER.open(req, timeout=15)
            prompt = (filler + "\n\nReply with exactly: A") if turn == 1 else f"Reply with exactly: {'ABCDEF'[turn - 1]}"
            cmd = base + (["--resume", sid] if sid else [])
            before = steered_count(root)
            done = subprocess.run(cmd, input=prompt, env=env, cwd=CWD, capture_output=True, text=True,
                                  encoding="utf-8", errors="replace", timeout=240)
            try:
                data = json.loads(done.stdout)
            except ValueError:
                rows.append({"turn": turn, "error": (done.stdout + done.stderr)[:300]})
                break
            sid = data.get("session_id", sid)
            usage = data.get("usage") or {}
            read, made, plain = (usage.get(k, 0) for k in
                                 ("cache_read_input_tokens", "cache_creation_input_tokens", "input_tokens"))
            total = read + made + plain
            rows.append({"turn": turn, "read": read, "created": made, "input": plain,
                         "hit": round(read / total, 4) if total else None,
                         "steered_requests": steered_count(root) - before, "is_error": data.get("is_error", False)})
            time.sleep(1)
    finally:
        proc.terminate()
        try:
            proc.wait(8)
        except Exception:
            proc.kill()
    return {"model": MODEL, "arm": arm, "trial": trial, "session": sid, "rows": rows}


# T6_FIRST_TRIAL: a rerun must use new trial numbers, because the seed gives the same filler and so a warm cache.
FIRST = int(os.environ.get("T6_FIRST_TRIAL", "1"))
results, port = [], 19100 + 40 * FIRST
for trial in range(FIRST, FIRST + TRIALS):
    for arm in ARMS:
        port += 2
        result = session(arm, trial, port)
        results.append(result)
        with open(OUT / f"results-{MODEL}.jsonl", "a", encoding="utf-8") as fh:
            fh.write(json.dumps(result) + "\n")
        cells = [f"t{r['turn']}:" + (f"{r['read']}/{r['created']}/s{r['steered_requests']}" if "read" in r else "ERR " + r["error"][:120])
                 for r in result["rows"]]
        print(MODEL, arm, trial, " ".join(cells), flush=True)
