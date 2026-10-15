#!/usr/bin/env python3
"""Explicitly authorized, request-bounded synthetic real-model acceptance.

Only the selected provider configuration is read from the personal DB, read-only.
A loopback relay holds real credentials in memory, enforces the authorized request
and output limits, and never prints URLs, keys, request bodies or upstream errors.
Rust creates all test records in temporary databases and sees only the relay URL.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import threading
import urllib.error
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("--provider-db", type=Path, required=True)
parser.add_argument("--allow-live-model", action="store_true")
parser.add_argument("--max-requests", type=int, default=48)
parser.add_argument("--test-filter", action="append",
                    help="Repeat to run selected scenarios under one shared request cap")
args = parser.parse_args()
if not args.allow_live_model:
    parser.error("--allow-live-model requires explicit permission to send synthetic material")
if not 1 <= args.max_requests <= 48:
    parser.error("request limit must be within the authorized range 1..48")
path = args.provider_db.resolve(strict=True)
with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True) as database:
    row = database.execute(
        "SELECT provider_type,base_url,model,COALESCE(api_key,''),temperature,max_tokens,context_window "
        "FROM ai_provider_configs WHERE is_active=1 LIMIT 1"
    ).fetchone()
if row is None:
    raise SystemExit("No active provider; no requests sent")
kind, base_url, model, api_key, temperature, max_tokens, window = row
if kind not in ("openai-compatible", "openai"):
    raise SystemExit("This fixture requires an OpenAI-compatible provider; no requests sent")
output_limit = min(max_tokens or 4096, 4096)
lock = threading.Lock()
metrics = []
quota_rejections = 0


class Relay(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"

    def log_message(self, *_):
        pass

    def do_POST(self):
        global quota_rejections
        if self.path != "/v1/chat/completions":
            self.send_error(404, "unsupported test route")
            return
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > 4 * 1024 * 1024:
            self.send_error(413, "test request too large")
            return
        payload = json.loads(self.rfile.read(length))
        payload["max_tokens"] = min(payload.get("max_tokens") or output_limit, output_limit)
        with lock:
            if len(metrics) >= args.max_requests:
                quota_rejections += 1
                self.send_error(429, "authorized acceptance request limit reached")
                return
            metric = {"request": len(metrics) + 1, "cache_tokens": None, "prompt_tokens": None,
                      "completion_tokens": None, "status": "started"}
            metrics.append(metric)
        headers = {"Content-Type": "application/json"}
        if api_key:
            headers["Authorization"] = "Bearer " + api_key
        request = urllib.request.Request(base_url.rstrip("/") + "/chat/completions",
                                         data=json.dumps(payload).encode(), headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=90) as response:
                self.send_response(response.status)
                self.send_header("Content-Type", response.headers.get("Content-Type", "application/json"))
                self.end_headers()
                received = bytearray()
                while chunk := response.read1(8192):
                    # Only response usage is retained; output is synthetic but
                    # bounded here as well. Body content is never logged by relay.
                    if len(received) < 1024 * 1024:
                        received.extend(chunk)
                    self.wfile.write(chunk)
                    self.wfile.flush()
                values = []
                text = received.decode("utf-8", errors="replace")
                if "text/event-stream" in response.headers.get("Content-Type", ""):
                    for line in text.splitlines():
                        if line.startswith("data:") and line[5:].strip() != "[DONE]":
                            try:
                                values.append(json.loads(line[5:]))
                            except ValueError:
                                pass
                else:
                    try:
                        values.append(json.loads(text))
                    except ValueError:
                        pass
                for value in values:
                    usage = value.get("usage") or {}
                    for key in ("prompt_tokens", "completion_tokens"):
                        if isinstance(usage.get(key), int):
                            metric[key] = max(metric[key] or 0, usage[key])
                    cached = (usage.get("prompt_tokens_details") or {}).get("cached_tokens")
                    if isinstance(cached, int):
                        metric["cache_tokens"] = max(metric["cache_tokens"] or 0, cached)
                metric["status"] = "completed"
        except urllib.error.HTTPError as error:
            metric["status"] = f"http_{error.code}"
            self.send_error(error.code, "upstream acceptance request rejected")
        except (BrokenPipeError, ConnectionResetError):
            metric["status"] = "client_stopped"
        except Exception:
            metric["status"] = "transport_error"
            self.send_error(502, "upstream acceptance transport failed")


relay = ThreadingHTTPServer(("127.0.0.1", 0), Relay)
thread = threading.Thread(target=relay.serve_forever, daemon=True)
thread.start()
config = dict(base_url=f"http://127.0.0.1:{relay.server_port}/v1", model=model,
              api_key="synthetic-relay", temperature=temperature,
              max_tokens=output_limit, context_window=window)
environment = os.environ.copy()
environment["ELSEWHEN_ACCEPTANCE_PROVIDER_JSON"] = json.dumps(config)
environment["NO_PROXY"] = environment["no_proxy"] = "127.0.0.1,localhost,::1"
environment.pop("ELSEWHEN_DEBUG", None)
print(json.dumps({"fixture": "synthetic-chat-v2", "model": model, "context_window": window,
                  "output_reserve": output_limit, "request_limit": args.max_requests,
                  "personal_data_sent": False}, ensure_ascii=False), flush=True)
root = Path(__file__).resolve().parent.parent
exit_code = 0
try:
    filters = args.test_filter or ["real_model_chat_"]
    for index, test_filter in enumerate(filters):
        if len(metrics) >= args.max_requests:
            print("CHAT_REAL_INCOMPLETE " + json.dumps({
                "reason": "authorized_request_limit", "unstarted_filters": filters[index:],
            }), flush=True)
            exit_code = 1
            break
        result = subprocess.run(
            ["cargo", "test", "--lib", test_filter, "--", "--ignored", "--nocapture", "--test-threads=1"],
            cwd=root, env=environment, check=False,
        )
        if result.returncode:
            exit_code = result.returncode
finally:
    relay.shutdown()
    relay.server_close()
    print("CHAT_REAL_TRANSPORT " + json.dumps(metrics), flush=True)
    print("CHAT_REAL_QUOTA " + json.dumps({
        "limit": args.max_requests, "actual_requests": len(metrics),
        "local_quota_rejections": quota_rejections,
    }), flush=True)
raise SystemExit(exit_code)
