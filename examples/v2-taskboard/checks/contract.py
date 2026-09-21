import json
from concurrent.futures import ThreadPoolExecutor
from urllib.error import HTTPError
from urllib.request import ProxyHandler, Request, build_opener


class HTTPContract:
    """Used against a fresh local server and a separate Docker API container."""

    def request(self, method, path, payload=None, raw=None, content_type="application/json"):
        data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
        request = Request(self.base_url + path, data=data, method=method,
                          headers={"Content-Type": content_type})
        # Never use ambient HTTP proxies for these local-only tests.
        try:
            response = build_opener(ProxyHandler({})).open(request, timeout=5)
        except HTTPError as error:
            response = error
        with response:
            self.assertTrue(response.headers["Content-Type"].startswith("application/json"))
            return response.status, json.load(response)

    def test_health(self):
        self.assertEqual(self.request("GET", "/health"), (200, {"status": "ok"}))

    def test_create_read_and_list(self):
        status, created = self.request("POST", "/tasks", {"title": "  Viết tests ✓  "})
        self.assertEqual(status, 201)
        self.assertEqual(created["title"], "Viết tests ✓")
        self.assertEqual(created["status"], "open")
        self.assertIsInstance(created["id"], int)
        self.assertEqual(self.request("GET", "/tasks/{}".format(created["id"])), (200, created))
        status, result = self.request("GET", "/tasks")
        self.assertEqual(status, 200)
        self.assertIn(created, result["tasks"])
        ids = [task["id"] for task in result["tasks"]]
        self.assertEqual(ids, sorted(set(ids)))

    def test_explicit_done_status(self):
        status, task = self.request("POST", "/tasks", {"title": "Finished", "status": "done"})
        self.assertEqual(status, 201)
        self.assertEqual(task["status"], "done")

    def test_invalid_payloads(self):
        cases = [([], "invalid_payload"), ({}, "invalid_title"),
                 ({"title": " \t "}, "invalid_title"), ({"title": 12}, "invalid_title"),
                 ({"title": "Task", "status": "invalid"}, "invalid_status"),
                 ({"title": "Task", "extra": True}, "unknown_field")]
        for payload, code in cases:
            with self.subTest(payload=payload):
                self.assertEqual(self.request("POST", "/tasks", payload), (400, {"error": code}))

    def test_malformed_json(self):
        self.assertEqual(self.request("POST", "/tasks", raw=b"{"), (400, {"error": "invalid_json"}))
        self.assertEqual(self.request("POST", "/tasks", raw=b"\xff"), (400, {"error": "invalid_json"}))

    def test_non_json_body(self):
        self.assertEqual(self.request("POST", "/tasks", raw=b"text", content_type="text/plain"),
                         (415, {"error": "json_required"}))

    def test_oversized_body(self):
        self.assertEqual(self.request("POST", "/tasks", raw=b"x" * 16_385),
                         (413, {"error": "body_too_large"}))

    def test_unknown_routes_and_tasks(self):
        for path in ("/tasks/9999999999", "/tasks/nope", "/tasks/-1"):
            with self.subTest(path=path):
                self.assertEqual(self.request("GET", path), (404, {"error": "task_not_found"}))
        self.assertEqual(self.request("GET", "/unknown"), (404, {"error": "route_not_found"}))

    def test_unknown_query(self):
        self.assertEqual(self.request("GET", "/tasks?unknown=x"), (400, {"error": "unsupported_query"}))

    def test_parallel_creates(self):
        def create(index):
            return self.request("POST", "/tasks", {"title": "Parallel {}".format(index)})
        with ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(create, range(12)))
        self.assertTrue(all(status == 201 for status, _ in results))
        ids = [task["id"] for _, task in results]
        self.assertEqual(len(set(ids)), 12)
        for _, task in results:
            self.assertEqual(self.request("GET", "/tasks/{}".format(task["id"])), (200, task))
