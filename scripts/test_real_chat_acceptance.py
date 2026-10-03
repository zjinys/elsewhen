"""Offline checks for the explicit live-acceptance request gate; no real DB/network."""
import contextlib
import io
import json
from pathlib import Path
import runpy
import tempfile
import types
import unittest
from unittest.mock import MagicMock, patch


SCRIPT = Path(__file__).with_name("real-chat-acceptance.py")


class AcceptanceGateTests(unittest.TestCase):
    def test_live_permission_is_required_before_reading_config(self):
        with patch("sys.argv", [str(SCRIPT), "--provider-db", "unused.db"]), \
                patch("sqlite3.connect") as connect, \
                contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as raised:
                runpy.run_path(str(SCRIPT), run_name="__main__")
        self.assertEqual(raised.exception.code, 2)
        connect.assert_not_called()

    def test_filters_share_one_cap_and_report_local_rejections(self):
        with tempfile.TemporaryDirectory() as directory:
            database = Path(directory) / "synthetic.db"
            database.touch()
            connection = MagicMock()
            connection.__enter__.return_value.execute.return_value.fetchone.return_value = (
                "openai-compatible", "https://synthetic.invalid/v1", "synthetic-model",
                "synthetic-key", 0.2, 8192, 65536,
            )
            handlers = []
            invocations = []
            requests = []
            rejection_codes = []

            def server_factory(address, handler):
                self.assertEqual(address[0], "127.0.0.1")
                handlers.append(handler)
                return MagicMock(server_port=12345)

            def upstream(request, timeout):
                requests.append(json.loads(request.data))
                response = MagicMock()
                response.status = 200
                response.headers = {"Content-Type": "application/json"}
                response.read1.side_effect = [b'{"usage":{"prompt_tokens":10}}', b""]
                response.__enter__.return_value = response
                return response

            def scenario(command, **kwargs):
                invocations.append(command)
                for _ in range(2):
                    payload = json.dumps({"max_tokens": 9000, "messages": []}).encode()
                    handler = handlers[0].__new__(handlers[0])
                    handler.path = "/v1/chat/completions"
                    handler.headers = {"Content-Length": str(len(payload))}
                    handler.rfile = io.BytesIO(payload)
                    handler.wfile = io.BytesIO()
                    handler.send_response = MagicMock()
                    handler.send_header = MagicMock()
                    handler.end_headers = MagicMock()
                    handler.send_error = lambda code, _: rejection_codes.append(code)
                    handler.do_POST()
                return types.SimpleNamespace(returncode=1 if rejection_codes else 0)

            argv = [str(SCRIPT), "--provider-db", str(database), "--allow-live-model",
                    "--max-requests", "3", "--test-filter", "first",
                    "--test-filter", "second", "--test-filter", "third"]
            output = io.StringIO()
            with patch("sys.argv", argv), patch("sqlite3.connect", return_value=connection), \
                    patch("http.server.ThreadingHTTPServer", side_effect=server_factory), \
                    patch("threading.Thread"), patch("urllib.request.urlopen", side_effect=upstream), \
                    patch("subprocess.run", side_effect=scenario), contextlib.redirect_stdout(output):
                with self.assertRaises(SystemExit) as raised:
                    runpy.run_path(str(SCRIPT), run_name="__main__")
            self.assertEqual(raised.exception.code, 1)
            self.assertEqual(len(invocations), 2)
            self.assertEqual(len(requests), 3)
            self.assertTrue(all(request["max_tokens"] == 4096 for request in requests))
            self.assertEqual(rejection_codes, [429])
            lines = output.getvalue().splitlines()
            quota = json.loads(next(line.removeprefix("CHAT_REAL_QUOTA ") for line in lines
                                    if line.startswith("CHAT_REAL_QUOTA ")))
            self.assertEqual(quota, {"limit": 3, "actual_requests": 3, "local_quota_rejections": 1})
            self.assertIn('"unstarted_filters": ["third"]', output.getvalue())
            self.assertNotIn("synthetic-key", output.getvalue())


if __name__ == "__main__":
    unittest.main()
