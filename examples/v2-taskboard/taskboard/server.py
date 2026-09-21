import json
import os
import signal
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

from taskboard.errors import APIError
from taskboard.features.create import create_task
from taskboard.features.detail import get_task
from taskboard.features.listing import list_tasks
from taskboard.store import TaskStore

MAX_BODY_BYTES = 16_384


def make_server(host="127.0.0.1", port=8080, store=None):
    task_store = store if store is not None else TaskStore()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def respond(self, status, body):
            data = json.dumps(body, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(data)
            self.close_connection = True

        def read_payload(self):
            if self.headers.get("Transfer-Encoding") is not None:
                raise APIError(400, "unsupported_transfer_encoding")
            if self.headers.get_content_type() != "application/json":
                raise APIError(415, "json_required")
            lengths = self.headers.get_all("Content-Length", [])
            if len(lengths) != 1 or not lengths[0].isascii() or not lengths[0].isdigit():
                raise APIError(400, "invalid_content_length")
            length = int(lengths[0])
            if length > MAX_BODY_BYTES:
                raise APIError(413, "body_too_large")
            try:
                return json.loads(self.rfile.read(length).decode("utf-8"))
            except (UnicodeDecodeError, ValueError):
                raise APIError(400, "invalid_json") from None

        def dispatch(self, method):
            self.connection.settimeout(5)
            try:
                url = urlsplit(self.path)
                query = parse_qs(url.query, keep_blank_values=True)
                if method == "GET" and url.path == "/health":
                    if query:
                        raise APIError(400, "unsupported_query")
                    self.respond(200, {"status": "ok"})
                elif method == "GET" and url.path == "/tasks":
                    self.respond(200, list_tasks(task_store, query))
                elif method == "GET" and url.path.startswith("/tasks/"):
                    task_id = url.path.removeprefix("/tasks/")
                    if not task_id.isascii() or not task_id.isdigit() or len(task_id) > 10:
                        raise APIError(404, "task_not_found")
                    if query:
                        raise APIError(400, "unsupported_query")
                    self.respond(200, get_task(task_store, int(task_id)))
                elif method == "POST" and url.path == "/tasks":
                    if query:
                        raise APIError(400, "unsupported_query")
                    self.respond(201, create_task(task_store, self.read_payload()))
                else:
                    raise APIError(404, "route_not_found")
            except APIError as error:
                self.respond(error.status, {"error": error.code})
            except TimeoutError:
                self.respond(408, {"error": "request_timeout"})

        def do_GET(self):
            self.dispatch("GET")

        def do_POST(self):
            self.dispatch("POST")

        def do_PATCH(self):
            self.respond(405, {"error": "method_not_allowed"})

        do_PUT = do_PATCH
        do_DELETE = do_PATCH

    return ThreadingHTTPServer((host, port), Handler)


if __name__ == "__main__":
    def stop(_signal, _frame):
        raise SystemExit(0)

    signal.signal(signal.SIGTERM, stop)
    with make_server(os.environ.get("HOST", "127.0.0.1"), int(os.environ.get("PORT", "8080"))) as server:
        print("Taskboard fixture listening on {}:{}".format(*server.server_address), flush=True)
        server.serve_forever()
