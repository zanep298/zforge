import os
import unittest
from urllib.parse import urlsplit

from checks.contract import HTTPContract


class ContainerHTTPTests(HTTPContract, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base_url = os.environ["API_URL"]
        url = urlsplit(cls.base_url)
        if (url.scheme != "http" or url.hostname not in ("api", "127.0.0.1", "localhost")
                or url.username or url.password or url.path or url.query or url.fragment):
            raise ValueError("API_URL must be the local fixture origin, never a production endpoint")
