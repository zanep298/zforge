import threading
import unittest

from checks.contract import HTTPContract
from taskboard.server import make_server


class HTTPTests(HTTPContract, unittest.TestCase):
    def setUp(self):
        self.server = make_server(port=0)
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01})
        self.thread.start()
        self.base_url = "http://127.0.0.1:{}".format(self.server.server_port)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
