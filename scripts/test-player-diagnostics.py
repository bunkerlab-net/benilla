#!/usr/bin/env python3
"""Authored child fixtures for bounded player smoke; no app build or game data."""
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "player_diagnostics", Path(__file__).with_name("check-player-diagnostics.py"))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


def report_rows():
    rows = []
    for stage in ("selection", "terrain_capture", "capture", "placement", "assembly",
                  "validation", "probe", "composition", "static_probe", "composition_freshness",
                  "navigation_neighbor_0", "navigation_neighbor_1", "navigation_neighbor_2",
                  "navigation_neighbor_3", "navigation_terrain", "navigation_input",
                  "navigation_generation", "navigation_validation", "navigation_probe", "navigation_freshness",
                  "freshness", "complete"):
        row = {"format": "benilla-collision-diagnostics-v3", "stage": stage,
               "status": "unavailable", "map": 0, "tile_x": 48, "tile_y": 32,
               "elapsed_ms": 0, "code": "collision.prerequisite", "source_stage": None,
               "metrics": {}, "hashes": {}, "probe": None, "gameplay_admitted": False}
        if stage == "selection":
            row.update(status="passed", code=None)
        elif stage == "terrain_capture":
            row.update(status="failed", code="source.open_patch_chain")
        elif stage == "capture":
            row.update(status="failed", code="vmap.required_member_absent",
                       source_stage="LiquidTypes", metrics={"member_lookups": 1,
                       "read_work_bytes": 0, "complete_raw_products": 0, "adt_attempts": 0})
        elif stage == "complete":
            row.update(status="failed", code=None,
                       metrics={"completed_probes": 0, "completed_static_probes": 0,
                                "completed_navigation_probes": 0,
                                "failed_stages": 2})
        rows.append(row)
    return rows


def report_bytes(rows=None):
    return ("\n".join(json.dumps(row) for row in (report_rows() if rows is None else rows))
            + "\n").encode("utf-8")


class ReportTests(unittest.TestCase):
    def test_accepts_complete_redacted_failure(self):
        self.assertEqual(len(report_rows()), 22)
        smoke.validate_report(report_bytes())

    def test_rejects_incomplete_chatter_and_invalid_json_without_echoing(self):
        output = report_bytes()
        for invalid in (b"", output[:-1], output.split(b"\n", 1)[1],
                        b"private-path-secret\n" + output,
                        output.replace(b'"selection"', b'"private-path-secret"'),
                        output.replace(b'"format":', b'"format": "secret", "format":', 1),
                        output.replace(b'"selection"', b'"\xff"'),
                        b"secret\n" * 8):
            with self.subTest(size=len(invalid)):
                with self.assertRaises(smoke.SmokeError) as raised:
                    smoke.validate_report(invalid)
                self.assertNotIn("secret", str(raised.exception))

    def test_rejects_schema_data_and_type_changes(self):
        variants = [(0, "map", False), (0, "elapsed_ms", True),
                    (0, "elapsed_ms", -1), (0, "elapsed_ms", 100000),
                    (1, "code", "collision.timeout"), (1, "path", "/private/secret"),
                    (2, "metrics", {"member_lookups": 1, "read_work_bytes": 0,
                                    "complete_raw_products": 0, "adt_attempts": False}),
                    (8, "probe", {"kind": "static_height"}),
                    (18, "probe", {"kind": "nearest"}),
                    (21, "metrics", {"completed_probes": 0, "failed_stages": 1}),
                    (21, "status", "passed"), (21, "gameplay_admitted", True)]
        for index, key, value in variants:
            with self.subTest(index=index, key=key):
                rows = copy.deepcopy(report_rows())
                rows[index][key] = value
                with self.assertRaises(smoke.SmokeError):
                    smoke.validate_report(report_bytes(rows))
        rows = report_rows()
        rows[0]["elapsed_ms"] = 2
        with self.assertRaises(smoke.SmokeError):
            smoke.validate_report(report_bytes(rows))

    def test_rejects_legacy_or_mixed_versions(self):
        for version in ("v1", "v2"):
            for index in (None, 0, 1, 8, 10, 18, 19, 21):
                rows = report_rows()
                for row in rows if index is None else [rows[index]]:
                    row["format"] = "benilla-collision-diagnostics-" + version
                with self.subTest(version=version, index=index), \
                        self.assertRaisesRegex(smoke.SmokeError, "requires v3"):
                    smoke.validate_report(report_bytes(rows))

    def test_requires_empty_install_navigation_counts_and_no_generated_evidence(self):
        for key, value in (("completed_navigation_probes", None),
                           ("completed_navigation_probes", 1),
                           ("completed_navigation_probes", False),
                           ("failed_stages", 3)):
            rows = report_rows()
            if value is None:
                del rows[-1]["metrics"][key]
            else:
                rows[-1]["metrics"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(smoke.SmokeError):
                smoke.validate_report(report_bytes(rows))
        for index in range(10, 20):
            rows = report_rows()
            rows[index].update(status="passed", code=None)
            with self.subTest(stage=rows[index]["stage"]), self.assertRaises(smoke.SmokeError):
                smoke.validate_report(report_bytes(rows))

    def test_requires_both_independent_failures_and_all_blocked_stages(self):
        rows = report_rows()
        for index in range(1, len(rows)):
            with self.subTest(omitted=rows[index]["stage"]):
                with self.assertRaises(smoke.SmokeError):
                    smoke.validate_report(report_bytes(rows[:index] + rows[index + 1:]))
        for index in (1, 2):
            changed = copy.deepcopy(rows)
            changed[index].update(status="unavailable", code="collision.prerequisite")
            with self.subTest(changed=rows[index]["stage"]):
                with self.assertRaises(smoke.SmokeError):
                    smoke.validate_report(report_bytes(changed))


class CollectorContractTests(unittest.TestCase):
    """Cross-language closed-schema contracts; no PowerShell execution implied."""

    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[1]
        cls.collector = (cls.root / "scripts/Collect-BenillaTerrainDiagnostics.ps1").read_text()

    def values(self, name):
        match = re.search(r"\$" + name + r" = @\((.*?)\)", self.collector, re.S)
        self.assertIsNotNone(match)
        return re.findall(r"'([^']+)'", match.group(1))

    def test_collector_safe_codes_exactly_match_rust_closed_list(self):
        codes_path = self.root / ("crates/benilla-app/src/net/p2p/world/"
                                  "native_geometry/products/vmap/diagnostic_codes.json")
        expected = json.loads(codes_path.read_text())
        actual = self.values("SafeCollisionIds")
        self.assertEqual(len(actual), len(set(actual)))
        self.assertEqual(set(actual), set(expected))

    def test_collector_stage_order_matches_player_smoke(self):
        self.assertEqual(tuple(self.values("CollisionStages")), smoke.STAGES)

    def test_collector_allowlists_new_metrics_and_hashes(self):
        self.assertTrue({"terrain_tiles", "terrain_bytes", "completed_static_probes",
                         "neighbor_present", "neighbor_index", "navigation_tiles",
                         "solid_vertices", "solid_triangles", "liquid_vertices",
                         "liquid_triangles", "request_bytes", "native_peak_bytes",
                         "navigation_output_bytes", "navigation_polygons", "navigation_status",
                         "polygon_ref_low", "polygon_ref_high", "completed_navigation_probes"}
                        <= set(self.values("CollisionMetricNames")))
        self.assertTrue({"terrain_source_epoch", "composition", "navigation_input", "navigation_output"}
                        <= set(self.values("CollisionHashNames")))

    def test_collector_preserves_byte_limits_and_raises_only_record_limit(self):
        self.assertRegex(self.collector, r"record_limit = 64; line_byte_limit = 16384; accepted_byte_limit = 262144")
        self.assertEqual(smoke.MAX_LINE, 16384)
        self.assertEqual(smoke.MAX_STDOUT, 262144)
        self.assertEqual(smoke.HARD_TIMEOUT, 20.0)

    def test_collector_serializes_compact_json_with_native_byte_bound_regressions(self):
        # Source contracts only: the authored Windows smoke exercises both native
        # serializers, the 46-record success report, and the full 64-record cap.
        self.assertIn("$json = ConvertTo-Json -InputObject $report -Depth 16 -Compress",
                      self.collector)
        authored = (self.root / "scripts/Test-BenillaTerrainDiagnostics.ps1").read_text(encoding="utf-8")
        self.assertIn("$byteCount = $utf8.GetByteCount($Raw)", authored)
        self.assertIn("$byteCount -lt 100000", authored)
        self.assertIn("Assert-ReportSerialization $output $raw $Mode", authored)
        self.assertIn("Assert-ReportSerialization $output $raw ('collision ' + $Mode)", authored)
        self.assertIn("$retainedBytes -le $r.collision_diagnostics.accepted_bytes", authored)

    def test_collector_only_accepts_v3(self):
        self.assertIn("$Object.format -cne 'benilla-collision-diagnostics-v3'", self.collector)
        self.assertNotIn("benilla-collision-diagnostics-v1", self.collector)
        self.assertNotIn("benilla-collision-diagnostics-v2", self.collector)

    def test_collector_help_discloses_sibling_execution_and_trust_limits(self):
        help_text = self.collector.split("#>", 1)[0]
        self.assertIn("automatically executes its sibling benilla-native-navigation.exe", help_text)
        self.assertIn("matching trusted package", help_text)
        self.assertIn("does not separately\nhash or authenticate", help_text)
        self.assertIn("not a sandbox", help_text)
        self.assertIn("replacement races", help_text)
        self.assertIn("checks its explicit Data, Helper,\nPlayer, and report paths", help_text)


class ProcessTests(unittest.TestCase):
    def run_child(self, code, **kwargs):
        with tempfile.TemporaryDirectory() as directory:
            return smoke.run_bounded([sys.executable, "-c", code], cwd=directory,
                                     env=os.environ.copy(), **kwargs)

    def test_direct_child_report_survives_flush_and_immediate_exit(self):
        expected = report_bytes()
        actual = self.run_child(
            f"import os,sys; sys.stdout.buffer.write({expected!r}); "
            "sys.stdout.buffer.flush(); os._exit(0)")
        smoke.validate_report(actual)
        self.assertEqual(actual, expected)

    def test_nonzero_exit_is_not_a_successful_report(self):
        with self.assertRaisesRegex(smoke.SmokeError, "exit successfully"):
            self.run_child(f"import sys; sys.stdout.buffer.write({report_bytes()!r}); sys.exit(7)")

    def test_zero_exit_with_invalid_report_is_rejected_without_echoing(self):
        output = self.run_child("print('/private/secret')")
        with self.assertRaises(smoke.SmokeError) as raised:
            smoke.validate_report(output)
        self.assertNotIn("secret", str(raised.exception))

    def test_stderr_is_never_echoed(self):
        with self.assertRaises(smoke.SmokeError) as raised:
            self.run_child("import sys; sys.stderr.write('/private/secret')")
        self.assertNotIn("secret", str(raised.exception))
        self.assertIn("withheld", str(raised.exception))

    def test_hung_child_has_a_hard_parent_deadline(self):
        start = time.monotonic()
        with self.assertRaisesRegex(smoke.SmokeError, "hard deadline"):
            self.run_child("import time; time.sleep(30)", timeout=0.2)
        self.assertLess(time.monotonic() - start, 7)

    def test_saturation_of_either_or_both_pipes_is_bounded(self):
        for channels in [(1,), (2,), (1, 2)]:
            with self.subTest(channels=channels):
                code = ("import os,threading,time\n"
                        "def flood(fd):\n"
                        " while True: os.write(fd, b'secret' * 2000)\n"
                        f"for fd in {channels!r}:\n"
                        " threading.Thread(target=flood,args=(fd,),daemon=True).start()\n"
                        "time.sleep(30)\n")
                start = time.monotonic()
                with self.assertRaises(smoke.SmokeError) as raised:
                    self.run_child(code, timeout=3, stdout_limit=1024, stderr_limit=1024)
                self.assertNotIn("secret", str(raised.exception))
                self.assertIn("limit", str(raised.exception))
                self.assertLess(time.monotonic() - start, 7)

    def test_drain_never_retains_more_than_its_cap(self):
        failed = threading.Event()
        drain = smoke.Drain(io.BytesIO(b"x" * 1000000), 1234, failed)
        drain.thread.join(timeout=2)
        self.assertTrue(drain.done.is_set())
        self.assertTrue(failed.is_set())
        self.assertTrue(drain.overflow)
        self.assertEqual(len(drain.data), 1234)

    @unittest.skipIf(os.name == "nt", "POSIX process-group cleanup fixture")
    def test_descendant_holding_pipes_cannot_hang_parent(self):
        code = ("import subprocess,sys\n"
                "subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\n")
        start = time.monotonic()
        with self.assertRaisesRegex(smoke.SmokeError, "hard deadline"):
            self.run_child(code, timeout=0.3)
        self.assertLess(time.monotonic() - start, 5)


class IsolationTests(unittest.TestCase):
    def with_player(self, run):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / ("benilla.exe" if os.name == "nt" else "benilla")
            binary.write_bytes(b"authored executable placeholder")
            with patch.object(smoke, "run_bounded", side_effect=run):
                smoke.check_player(binary)
            self.assertEqual(binary.read_bytes(), b"authored executable placeholder")
            self.assertEqual(list(binary.parent.iterdir()), [binary])

    def test_copied_binary_empty_data_and_separate_state_locations(self):
        def run(command, *, cwd, env):
            self.assertEqual(Path(command[0]).parent, cwd)
            self.assertEqual(command[1:], ["diagnose-collision", "--data", command[3],
                                         "--map", "0", "--tile", "48,32",
                                         "--timeout-seconds", "10"])
            self.assertEqual(list(Path(command[3]).iterdir()), [])
            self.assertNotEqual(env["HOME"], env["APPDATA"])
            self.assertNotEqual(Path(env["HOME"]), cwd)
            self.assertNotIn("WOW_PASS", env)
            self.assertNotIn("BENILLA_HOME", env)
            self.assertNotIn("DISPLAY", env)
            return report_bytes()
        with patch.dict(os.environ, {"WOW_PASS": "secret", "BENILLA_HOME": "/private",
                                     "DISPLAY": ":0"}):
            self.with_player(run)

    def test_profile_config_data_home_and_temp_writes_fail(self):
        for location in ("player", "data", "HOME", "APPDATA", "LOCALAPPDATA", "TMP"):
            with self.subTest(location=location):
                def run(command, *, cwd, env):
                    root = cwd if location == "player" else Path(
                        command[3] if location == "data" else env[location])
                    (root / "unexpected-secret-profile").write_text("secret", encoding="utf-8")
                    return report_bytes()
                with self.assertRaises(smoke.SmokeError) as raised:
                    self.with_player(run)
                self.assertNotIn("secret", str(raised.exception))

    def test_main_withholds_paths_on_setup_failure(self):
        output = io.StringIO()
        with patch.object(smoke, "check_player", side_effect=OSError("/private/secret")), \
                contextlib.redirect_stderr(output):
            self.assertEqual(smoke.main(["--binary", "unused"]), 1)
        self.assertNotIn("secret", output.getvalue())


if __name__ == "__main__":
    unittest.main()
