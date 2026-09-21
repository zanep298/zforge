import subprocess
import unittest
from unittest.mock import patch

from scripts.check import main


class RunnerTests(unittest.TestCase):
    def setUp(self):
        # Expected failure diagnostics from mocked runs must not look like a real
        # environment leak in the enclosing Docker test log.
        quiet = patch("scripts.check.print")
        quiet.start()
        self.addCleanup(quiet.stop)

    @patch("scripts.check.subprocess.run")
    def test_success_cleans_only_its_project(self, run):
        run.return_value.returncode = 0
        self.assertEqual(main(), 0)
        start, cleanup = [call.args[0] for call in run.call_args_list]
        self.assertEqual(start[:6], cleanup[:6])
        self.assertTrue(start[3].startswith("zforge-tb-"))
        self.assertIn("--exit-code-from", start)
        self.assertEqual(cleanup[6:], ["down", "--volumes", "--remove-orphans"])

    @patch("scripts.check.subprocess.run")
    def test_failed_tests_remain_failed_after_cleanup(self, run):
        run.side_effect = [subprocess.CompletedProcess([], 7), subprocess.CompletedProcess([], 0)]
        self.assertEqual(main(), 7)

    @patch("scripts.check.subprocess.run")
    def test_timeout_still_cleans_up(self, run):
        run.side_effect = [subprocess.TimeoutExpired([], 300), subprocess.CompletedProcess([], 0)]
        self.assertEqual(main(), 1)
        self.assertIn("down", run.call_args.args[0])

    @patch("scripts.check.subprocess.run")
    def test_cleanup_timeout_fails_the_run(self, run):
        run.side_effect = [subprocess.CompletedProcess([], 0), subprocess.TimeoutExpired([], 60)]
        self.assertEqual(main(), 1)

    @patch("scripts.check.subprocess.run")
    def test_each_run_has_a_unique_project(self, run):
        run.return_value.returncode = 0
        self.assertEqual(main(), 0)
        self.assertEqual(main(), 0)
        self.assertNotEqual(run.call_args_list[0].args[0][3], run.call_args_list[2].args[0][3])
