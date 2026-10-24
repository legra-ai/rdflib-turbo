"""Owned-child guards must fail safely independently of benchmark outcomes."""

import importlib
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
comparison = importlib.import_module("inference_comparison")


class Child:
    pid = 12345

    def __init__(self, finished=False):
        self.finished = finished

    def poll(self):
        return 2 if self.finished else None

    def wait(self, timeout=None):
        self.finished = True
        return 2


class WatchdogTests(unittest.TestCase):
    def test_fixture_models_posix_boundary_when_host_has_no_process_groups(self):
        with (
            patch.object(comparison, "os", SimpleNamespace()),
            patch.object(comparison, "signal", SimpleNamespace()),
        ):
            result = self.invoke("12345 1 1\n", [0, 91, 92])
        self.assertEqual(result["guard"], "90second wall guard")

    def invoke(self, table, times, child=None, error=None):
        with tempfile.TemporaryDirectory() as directory:
            kill = Mock()
            signals = SimpleNamespace(SIGTERM=15, SIGKILL=9)
            with (
                patch.object(comparison.subprocess, "Popen", return_value=child or Child()),
                patch.object(
                    comparison.subprocess, "check_output", return_value=table, side_effect=error
                ),
                patch.object(comparison.time, "monotonic", side_effect=times),
                patch.object(comparison, "os", SimpleNamespace(killpg=kill)),
                patch.object(comparison, "signal", signals),
            ):
                if error:
                    with self.assertRaises(RuntimeError):
                        comparison.guarded(["worker"], Path(directory), 0)
                    kill.assert_called_once_with(12345, signals.SIGTERM)
                    return None
                result = comparison.guarded(["worker"], Path(directory), 0)
                kill.assert_called_once_with(12345, signals.SIGTERM)
                return result

    def test_wall_deadline_stops_only_owned_process_group(self):
        result = self.invoke("12345 1 1\n", [0, 91, 92])
        self.assertEqual(result["guard"], "90second wall guard")

    def test_rss_is_child_tree_kib_converted_to_bytes(self):
        result = self.invoke("12345 1 100\n12346 12345 1310700\n8888 1 9999999\n", [0, 1, 2])
        self.assertEqual(result["guard"], "1280MiB owned-child RSS guard")
        self.assertEqual(result["sampled_peak_bytes"], 1310800 * 1024)

    def test_sampler_failure_also_reaps_owned_child(self):
        self.invoke("", [0], error=RuntimeError("ps failed"))

    def test_child_exit_between_term_timeout_and_kill_is_reaped(self):
        child = Mock(pid=12345)
        child.wait.side_effect = [comparison.subprocess.TimeoutExpired("worker", 5), 0]
        with (
            patch.object(
                comparison,
                "os",
                SimpleNamespace(killpg=Mock(side_effect=[None, ProcessLookupError()])),
            ),
            patch.object(comparison, "signal", SimpleNamespace(SIGTERM=15, SIGKILL=9)),
        ):
            comparison.stop_child(child)
        self.assertEqual(child.wait.call_count, 2)


if __name__ == "__main__":
    unittest.main()
