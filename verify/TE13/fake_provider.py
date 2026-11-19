"""TE13 rig: a scripted OpenAI-compatible provider that also records what OpenCode sends.

    python fake_provider.py <port> <capture.jsonl>

A prompt containing `TOOL:<name>` gets one call to that tool, `CODE:<server>.<tool>` one call
to `execute` that awaits that MCP tool, then the text "ok"; anything else gets the text "ok". Every request
is logged with its tool names and their size, and the newest request with tools is saved whole,
because Code Mode lists MCP tools in the instructions. No model, no network, no credentials read.
"""
import json
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, CAPTURE = int(sys.argv[1]), sys.argv[2]
WATCH = ("acme_lookup", "lookup", "bili_compress", "compress")


def fill(schema):
    """Arguments for a tool, from its JSON schema: only the required properties."""
    props, out = schema.get("properties") or {}, {}
    for key in schema.get("required") or []:
        low, kind = key.lower(), (props.get(key) or {}).get("type")
        if "path" in low or "file" in low:
            out[key] = "README.md"
        elif "pattern" in low:
            out[key] = "*.md"
        elif "command" in low:
            out[key] = "git log --oneline -1"
        elif kind == "boolean":
            out[key] = False
        elif kind in ("integer", "number"):
            out[key] = 1
        elif kind == "array":
            out[key] = []
        elif kind == "object":
            out[key] = {}
        else:
            out[key] = "x"
    return out


def chunk(model, delta, finish=None, usage=None):
    body = {"id": "chatcmpl-te13", "object": "chat.completion.chunk", "created": int(time.time()),
            "model": model, "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}
    if usage:
        body["usage"] = usage
    return "data: " + json.dumps(body) + "\n\n"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def log_message(self, fmt, *args):
        pass

    def do_GET(self):
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        data = json.loads(self.rfile.read(int(self.headers.get("Content-Length") or 0)))
        tools = {}
        for tool in data.get("tools") or []:
            fn = tool.get("function") or {}
            tools[fn.get("name")] = fn
        messages = data.get("messages") or []
        entry = {"ts": time.time(), "tools": sorted(tools), "n_tools": len(tools),
                 "n_messages": len(messages), "bytes_tools": len(json.dumps(data.get("tools") or [])),
                 "mentions": {name: {w: w in (fn.get("description") or "") for w in WATCH}
                              for name, fn in tools.items() if name == "execute"}}
        with open(CAPTURE, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry) + "\n")

        if tools:  # the newest request with tools, whole, for reading the Code Mode surface
            with open(CAPTURE + ".last.json", "w", encoding="utf-8") as fh:
                json.dump(data, fh, indent=1)

        last = messages[-1] if messages else {}
        text = last.get("content")
        if isinstance(text, list):
            text = "".join(part.get("text", "") for part in text if isinstance(part, dict))
        match = re.search(r"TOOL:([A-Za-z0-9_]+)", text or "") if last.get("role") == "user" else None
        code = re.search(r"CODE:([A-Za-z0-9_.]+)", text or "") if last.get("role") == "user" else None
        model = data.get("model", "fake")
        if code and "execute" in tools:
            key = next(iter((tools["execute"].get("parameters") or {}).get("required") or ["code"]))
            delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_te13", "type": "function",
                                                          "function": {"name": "execute",
                                                                       "arguments": json.dumps({key: "return await tools.%s({})" % code.group(1)})}}]}
            out = chunk(model, delta) + chunk(model, {}, "tool_calls")
        elif match and match.group(1) in tools:
            name = match.group(1)
            args = json.dumps(fill(tools[name].get("parameters") or {}))
            delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_te13", "type": "function",
                                                          "function": {"name": name, "arguments": args}}]}
            out = chunk(model, delta) + chunk(model, {}, "tool_calls")
        else:
            out = chunk(model, {"role": "assistant", "content": "ok"}) + chunk(model, {}, "stop")
        out += "data: [DONE]\n\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(out.encode("utf-8"))


ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
