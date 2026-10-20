#!/usr/bin/env python3
"""Packaging and resolved feature-graph tests: no app build or game data."""
import hashlib
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import zipfile
from datetime import datetime, timezone
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location("package_player", Path(__file__).with_name("package-player.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)
SHA = "a" * 40


def executables(platform):
    suffix = ".exe" if platform == "windows-amd64" else ""
    return tuple(name + suffix for name in packager.EXECUTABLES)


_path_read_text = Path.read_text


def read_text_with_legacy_default(path, *args, **kwargs):
    """Exercise source scanning with a Windows-style default on every host."""
    kwargs.setdefault("encoding", "cp1252")
    return _path_read_text(path, *args, **kwargs)


class PlayerFeatureGraphTests(unittest.TestCase):
    """Resolve the actual player/dev graphs without compiling or linking the app."""

    def assert_dev_features(self, packages, enabled, *, test_dependencies=False):
        root = Path(__file__).resolve().parents[1]
        command = ["cargo", "tree", "--locked", "--no-default-features",
                   "--prefix", "none", "--format", "{p}|{f}",
                   "--edges", "normal,build,dev" if test_dependencies else "normal,build"]
        for package in packages:
            command.extend(["-p", package])
        if enabled:
            command.extend(["--features", "dev"])
        result = subprocess.run(command, cwd=root, check=True, capture_output=True, text=True)
        features = {}
        for line in result.stdout.splitlines():
            package, separator, selected = line.partition("|")
            if separator:
                features.setdefault(package.split()[0], set()).update(
                    filter(None, selected.removesuffix(" (*)").strip().split(",")))
        if not test_dependencies:
            self.assertIn("benilla-native-vmap-source", features)
            self.assertNotIn("synthetic-map-tests", features["benilla-native-vmap-source"])
        for package in ["benilla-app", "benilla-assets", "benilla-formats", "benilla-world"]:
            with self.subTest(package=package):
                self.assertIn(package, features, "the resolved graph must contain the dev seam")
                self.assertEqual("dev" in features[package], enabled,
                                 f"{package}: selected features {sorted(features[package])}")

    def test_player_launcher_does_not_enable_dev_through_dependencies(self):
        self.assert_dev_features(["benilla"], False)

    def test_packaged_player_and_helper_do_not_enable_dev_through_dependencies(self):
        self.assert_dev_features(["benilla", "benilla-native-terrain"], False)

    def test_player_test_roots_do_not_enable_dev_through_dependencies(self):
        self.assert_dev_features(["benilla-formats", "benilla-app"], False,
                                 test_dependencies=True)

    def test_explicit_dev_launcher_forwards_the_dev_seam(self):
        self.assert_dev_features(["benilla"], True)

    def test_standalone_helper_graph_has_no_player_or_bevy(self):
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run([
            "cargo", "tree", "--locked", "--no-default-features", "-p", "benilla-native-terrain",
            "--prefix", "none", "--format", "{p}|{f}", "--edges", "normal,build",
        ], cwd=root, check=True, capture_output=True, text=True)
        packages = {}
        for line in result.stdout.splitlines():
            package, separator, features = line.partition("|")
            if separator:
                packages.setdefault(package.split()[0], set()).update(
                    filter(None, features.removesuffix(" (*)").strip().split(",")))
        self.assertIn("benilla-native-terrain", packages)
        self.assertIn("benilla-mpq", packages)
        self.assertIn("benilla-formats", packages)
        self.assertNotIn("dev", packages["benilla-formats"])
        self.assertFalse({"benilla", "benilla-app", "bevy"} & packages.keys())
        self.assertFalse(any(name.startswith("bevy_") for name in packages))


    @patch.object(Path, "read_text", new=read_text_with_legacy_default)
    def test_shared_geometry_test_includes_declare_direct_dependencies(self):
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"],
            cwd=root, check=True, capture_output=True, text=True, timeout=60)
        packages = json.loads(result.stdout)["packages"]
        shared = root / "crates/benilla-app/src/net/p2p/world/native_geometry/products/mod.rs"
        consumers = []
        for package in packages:
            directory = Path(package["manifest_path"]).parent
            includes = any(
                (path.parent / relative).resolve() == shared.resolve()
                for path in directory.rglob("*.rs")
                for relative in re.findall(r'#\[path\s*=\s*"([^"]+)"\]', path.read_text(encoding="utf-8")))
            if package["name"] != "benilla-app" and not includes:
                continue
            consumers.append(package["name"])
            dependencies = package["dependencies"]
            with self.subTest(package=package["name"]):
                sources = [d for d in dependencies if d["name"] == "benilla-native-vmap-source"]
                self.assertEqual(len(sources), 2 if package["name"] == "benilla-app" else 1)
                development = [d for d in sources if d["kind"] == "dev"]
                self.assertEqual(len(development), 1)
                self.assertIn("synthetic-map-tests", development[0]["features"])
                if package["name"] == "benilla-app":
                    normal = [d for d in sources if d["kind"] is None]
                    self.assertEqual(len(normal), 1)
                    self.assertNotIn("synthetic-map-tests", normal[0]["features"])
                json_deps = [d for d in dependencies if d["name"] == "serde_json"]
                self.assertEqual(len(json_deps), 1)
                self.assertEqual(json_deps[0]["kind"], None if package["name"] == "benilla-app" else "dev")
                for name in ("benilla-native-nav-generator", "libc"):
                    direct = [d for d in dependencies if d["name"] == name]
                    self.assertEqual(len(direct), 1)
                    self.assertEqual(direct[0]["kind"], None if package["name"] == "benilla-app" else "dev")
                model = [d for d in dependencies if d["name"] == "benilla-native-vmap"]
                self.assertEqual(len(model), 1)
                self.assertEqual(model[0]["kind"], None if package["name"] == "benilla-app" else "dev")
        self.assertIn("benilla-app", consumers)
        self.assertGreater(len(consumers), 1, "discover actual include harnesses, not only the app")

    def test_collision_diagnostics_dispatch_precedes_persistent_or_window_setup(self):
        root = Path(__file__).resolve().parents[1]
        code = (root / "crates/benilla-app/src/lib.rs").read_text(encoding="utf-8")
        launch = code.split("fn launch(", 1)[1]
        diagnostic = launch.index('OsStr::new("diagnose-collision")')
        recovery = launch.index("profile_recovery::run(build)")
        self.assertLess(diagnostic, recovery)
        branch = launch[diagnostic:recovery]
        self.assertIn("return match net::run_collision_diagnostics", branch)
        self.assertIn("std::io::stdout().lock()", branch)
        self.assertNotIn("local_state", branch)
        self.assertNotIn("App::new", branch)
        codes = json.loads((root / "crates/benilla-app/src/net/p2p/world/native_geometry/products/vmap/diagnostic_codes.json").read_text(encoding="utf-8"))
        collector = (root / "scripts/Collect-BenillaTerrainDiagnostics.ps1").read_text(encoding="utf-8")
        for code in codes:
            self.assertIn("'" + code + "'", collector)


class PackagingTests(unittest.TestCase):
    def fixture(self, root, platform="linux-amd64"):
        executable = executables(platform)[0]
        binaries = [root / "target" / packager.TARGETS[platform] / "release" / name
                    for name in (*executables(platform), "benilla-native-terrain" + (".exe" if platform == "windows-amd64" else ""),
                                 "benilla-native-navigation" + (".exe" if platform == "windows-amd64" else ""))]
        binary = binaries[0]
        source = packager.source_dir(root, SHA)
        for path in [*binaries, *(source / p for p in packager.REQUIRED_SOURCE),
                     source / "vendor/fake-1.0/src/lib.rs", source / "vendor/fake-1.0/LICENSE",
                     source / "vendor/fake-1.0/.cargo-checksum.json"]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture", encoding="utf-8")
        for path in binaries:
            path.write_text(f"fixture:{path.name}", encoding="utf-8")
            path.chmod(0o755)
        self.seal(source)
        for name in ["WoW/Data/game.MPQ", "benilla-config/config.toml", "config.toml", ".probe-identity"]:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("must not ship", encoding="utf-8")
        return binary, executable, source

    def seal(self, source):
        (source / packager.MANIFEST).write_text(json.dumps({"commit": SHA, "files": packager.inventory(source)}))

    def test_all_platforms(self):
        for platform in packager.TARGETS:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary, _, source = self.fixture(root, platform)
                for name in executables(platform):
                    (binary.parent / name).chmod(0o644)  # Windows cannot set POSIX executable bits.
                # Packaging copies binary bytes; it must never execute either helper or player.
                with patch.object(packager.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                    archive = packager.package(root, platform, SHA)
                prefix = f"benilla-{platform}-{SHA}"
                expected = {f"{prefix}/{p}" for p in [*executables(platform), "config.toml", "README.md"]}
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        self.assertEqual(set(output.namelist()), expected)
                        for name in executables(platform):
                            self.assertEqual(output.read(f"{prefix}/{name}"), f"fixture:{name}".encode())
                        self.assertIn(SHA, output.read(f"{prefix}/README.md").decode())
                else:
                    with tarfile.open(archive) as output:
                        self.assertEqual(set(output.getnames()), expected)
                        for name in executables(platform):
                            self.assertEqual(output.getmember(f"{prefix}/{name}").mode, 0o755)
                            self.assertEqual(output.extractfile(f"{prefix}/{name}").read(), f"fixture:{name}".encode())
                        self.assertIn(SHA, output.extractfile(f"{prefix}/README.md").read().decode())
                with tarfile.open(packager.source_archive_path(root, platform, SHA)) as output:
                    wanted = {f"benilla-source-{SHA}/{p.relative_to(source).as_posix()}" for p in source.rglob("*") if p.is_file()}
                    self.assertEqual(set(output.getnames()), wanted)
                self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                 f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")

    def test_player_does_not_require_diagnostic_helpers_or_copy_build_extras(self):
        for platform in packager.TARGETS:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                player, _, _ = self.fixture(root, platform)
                for path in player.parent.iterdir():
                    if path != player:
                        path.unlink()
                for name in ("debug.pdb", "unused.dll", "source.rs", "Data/asset.mpq"):
                    extra = player.parent / name
                    extra.parent.mkdir(exist_ok=True)
                    extra.write_bytes(b"must not ship")
                archive = packager.package(root, platform, SHA)
                prefix = f"benilla-{platform}-{SHA}/"
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        names = output.namelist()
                else:
                    with tarfile.open(archive) as output:
                        names = output.getnames()
                self.assertEqual(set(names), {prefix + player.name, prefix + "config.toml", prefix + "README.md"})

    def test_player_config_has_no_active_p2p_or_data_override(self):
        root = Path(__file__).resolve().parents[1]
        original = (root / "config.example.toml").read_bytes()
        self.assertIn("p2p", tomllib.loads(original.decode()))
        config = packager.player_config(root)
        self.assertEqual(tomllib.loads(config), {})
        for setting in ('# [p2p]', '# psk = "REPLACE_WITH_A_PRIVATE_RANDOM_PASSPHRASE"',
                        '# [wow]', '# data = "WoW/Data"', '# [ruleset]'):
            self.assertIn(setting, config)
        self.assertIn("section header (for example [wow]) together with its settings", config)
        self.assertEqual((root / "config.example.toml").read_bytes(), original)

    def test_player_readme_preserves_each_legal_notice_byte_for_byte(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            for number, name in enumerate(packager.LEGAL_DOCUMENTS):
                encoding = packager.LEGAL_ENCODINGS.get(name, "utf-8")
                (source / name).write_bytes(f"Notice {number}: © attribution\r\nLicense terms\r\n".encode(encoding))
            self.seal(source)
            archive = packager.package(root, "linux-amd64", SHA)
            with tarfile.open(archive) as output:
                readme = output.extractfile(f"benilla-linux-amd64-{SHA}/README.md").read()
            for name in packager.LEGAL_DOCUMENTS:
                notice = (source / name).read_bytes()
                if name in packager.LEGAL_ENCODINGS:
                    notice = notice.decode(packager.LEGAL_ENCODINGS[name]).encode("utf-8")
                self.assertIn(notice, readme)
            self.assertIn(b"Source commit: " + SHA.encode(), readme)
            self.assertIn(packager.digest(packager.source_archive_path(root, "linux-amd64", SHA)).encode(), readme)

    def test_real_player_notices_form_readable_utf8_without_modifying_source(self):
        root = Path(__file__).resolve().parents[1]
        originals = {name: (root / name).read_bytes() for name in packager.LEGAL_DOCUMENTS}
        readme = packager.player_readme(root, "windows-amd64", SHA, "source fixture")
        text = readme.decode("utf-8")
        self.assertIn("Copyright © 2000-2010", text)
        self.assertIn("GNU GENERAL PUBLIC LICENSE", text)
        for name, original in originals.items():
            self.assertEqual((root / name).read_bytes(), original)
            encoded = original.decode(packager.LEGAL_ENCODINGS.get(name, "utf-8")).encode("utf-8")
            self.assertIn(encoded, readme)

    def test_diagnostic_download_keeps_both_helpers_separate_on_every_platform(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        for platform in packager.TARGETS:
            for separate in (False, True):
                with self.subTest(platform=platform, separate=separate), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    player, _, _ = self.fixture(root, platform)
                    player.unlink()
                    if separate:
                        packager.package_source(root, platform, SHA)
                    with patch.object(packager.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                        archive = packager.package(root, platform, SHA, url if separate else None,
                                                   diagnostics_only=True)
                    prefix = f"benilla-diagnostics-{platform}-{SHA}/"
                    suffix = ".exe" if platform == "windows-amd64" else ""
                    source_name = "SOURCE-DOWNLOAD.txt" if separate else "corresponding-source.tar.gz"
                    expected = {prefix + p for p in [*(n + suffix for n in packager.DIAGNOSTIC_EXECUTABLES),
                                                     *packager.DOCUMENTS, source_name]}
                    if platform == "windows-amd64":
                        with zipfile.ZipFile(archive) as output:
                            self.assertEqual(set(output.namelist()), expected)
                    else:
                        with tarfile.open(archive) as output:
                            self.assertEqual(set(output.getnames()), expected)
                            for helper in packager.DIAGNOSTIC_EXECUTABLES:
                                self.assertEqual(output.getmember(prefix + helper).mode, 0o755)

    def test_diagnostics_missing_helpers_fail_closed(self):
        for platform in packager.TARGETS:
            suffix = ".exe" if platform == "windows-amd64" else ""
            for missing in packager.DIAGNOSTIC_EXECUTABLES:
                with self.subTest(platform=platform, missing=missing), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    player, _, _ = self.fixture(root, platform)
                    (player.parent / (missing + suffix)).unlink()
                    with self.assertRaises(ValueError):
                        packager.package(root, platform, SHA, diagnostics_only=True)
                    self.assertFalse((root / "dist").exists())

    def test_diagnostics_reject_source_operation_flags(self):
        for option in ("--prepare-source", "--source-only", "--helper-only"):
            with self.subTest(option=option):
                result = subprocess.run([
                    sys.executable, str(Path(packager.__file__)), "--diagnostics-only", option,
                    "--commit", SHA, "--platform", "windows-amd64",
                ], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn("cannot be combined", result.stderr)

    def test_helper_only_needs_no_player_and_preserves_source_and_notices(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        for platform in packager.TARGETS:
            for separate in (False, True):
                with self.subTest(platform=platform, separate=separate), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    player, _, source = self.fixture(root, platform)
                    player.unlink()
                    helper = "benilla-native-terrain" + (".exe" if platform == "windows-amd64" else "")
                    (player.parent / helper).chmod(0o644)
                    source_archive = packager.package_source(root, platform, SHA) if separate else None
                    before = source_archive.read_bytes() if separate else None
                    with patch.object(packager.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                        archive = packager.package(root, platform, SHA, url if separate else None,
                                                   helper_only=True)
                    prefix = f"benilla-native-terrain-{platform}-{SHA}"
                    extension = ".zip" if platform == "windows-amd64" else ".tar.gz"
                    self.assertEqual(archive.name, prefix + extension)
                    source_name = "SOURCE-DOWNLOAD.txt" if separate else "corresponding-source.tar.gz"
                    expected = {f"{prefix}/{p}" for p in [helper, *packager.DOCUMENTS, source_name]}
                    if platform == "windows-amd64":
                        with zipfile.ZipFile(archive) as output:
                            self.assertEqual(set(output.namelist()), expected)
                            self.assertEqual(output.read(f"{prefix}/{helper}"), f"fixture:{helper}".encode())
                            payload = output.read(f"{prefix}/{source_name}")
                    else:
                        with tarfile.open(archive) as output:
                            self.assertEqual(set(output.getnames()), expected)
                            self.assertEqual(output.getmember(f"{prefix}/{helper}").mode, 0o755)
                            self.assertEqual(output.extractfile(f"{prefix}/{helper}").read(), f"fixture:{helper}".encode())
                            payload = output.extractfile(f"{prefix}/{source_name}").read()
                    if separate:
                        notice = payload.decode()
                        self.assertEqual(notice.splitlines()[0], "Corresponding Source for benilla-native-terrain")
                        for required in (url, SHA, source_archive.name, hashlib.sha256(before).hexdigest()):
                            self.assertIn(required, notice)
                        self.assertEqual(source_archive.read_bytes(), before)
                    else:
                        with tarfile.open(fileobj=io.BytesIO(payload)) as output:
                            wanted = {f"benilla-source-{SHA}/{p.relative_to(source).as_posix()}"
                                      for p in source.rglob("*") if p.is_file()}
                            self.assertEqual(set(output.getnames()), wanted)
                    self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                     f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")

    def test_helper_notice_generated_with_lf_or_crlf(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        original_write_text = Path.write_text
        for newline in ("\n", "\r\n"):
            with self.subTest(newline=repr(newline)), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                player, _, _ = self.fixture(root, "windows-amd64")
                player.unlink()
                source_archive = packager.package_source(root, "windows-amd64", SHA)
                source_before = source_archive.read_bytes()
                notice_writes = []

                def write_notice(path, data, *args, **kwargs):
                    if path.name.endswith("-SOURCE-DOWNLOAD.txt"):
                        kwargs["newline"] = newline
                        notice_writes.append(path)
                    return original_write_text(path, data, *args, **kwargs)

                # Exercise the real notice writer and ZIP path with either host's
                # text newline convention, without executing a packaged binary.
                with patch.object(Path, "write_text", write_notice), \
                     patch.object(packager.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                    archive = packager.package(root, "windows-amd64", SHA, url, helper_only=True)
                self.assertEqual(len(notice_writes), 1)
                prefix = f"benilla-native-terrain-windows-amd64-{SHA}"
                with zipfile.ZipFile(archive) as output:
                    payload = output.read(f"{prefix}/SOURCE-DOWNLOAD.txt")
                self.assertIn(newline.encode(), payload)
                self.assertNotIn(b"\n", payload.replace(newline.encode(), b""))
                self.assertNotIn(b"\r", payload.replace(newline.encode(), b""))
                lines = payload.decode().splitlines()
                self.assertEqual(lines[0], "Corresponding Source for benilla-native-terrain")
                for line in (f"Source commit: {SHA}", "Platform: windows-amd64", f"Download: {url}",
                             f"Archive: {source_archive.name}", f"SHA-256: {hashlib.sha256(source_before).hexdigest()}"):
                    self.assertIn(line, lines)
                self.assertEqual(source_archive.read_bytes(), source_before)
                self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                 f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")

    def test_helper_only_keeps_source_and_binary_checks_fail_closed(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        for failure in ("helper", "license", "notice", "source", "checksum", "commit", "url"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                player, _, source = self.fixture(root, "windows-amd64")
                archive = packager.package_source(root, "windows-amd64", SHA)
                if failure == "helper":
                    (player.parent / "benilla-native-terrain.exe").unlink()
                elif failure == "license":
                    (source / "LICENSE-GPL-3.0").unlink()
                elif failure == "notice":
                    (source / "crates/benilla-native-terrain/CMANGOS-NOTICE.txt").unlink()
                elif failure == "source":
                    (source / "Cargo.lock").write_text("changed")
                elif failure == "checksum":
                    archive.write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    packager.package(root, "windows-amd64", "b" * 40 if failure == "commit" else SHA,
                                     "https://example.com/source" if failure == "url" else url,
                                     helper_only=True)
                self.assertFalse((root / "dist" / f"benilla-native-terrain-windows-amd64-{SHA}.zip").exists())

    def test_helper_only_rejects_source_operation_flags(self):
        for option in ("--prepare-source", "--source-only", "--diagnostics-only"):
            with self.subTest(option=option):
                result = subprocess.run([
                    sys.executable, str(Path(packager.__file__)), "--helper-only", option,
                    "--commit", SHA, "--platform", "windows-amd64",
                ], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn("cannot be combined", result.stderr)

    def test_authored_terrain_fixtures_survive_source_packaging(self):
        repository = Path(__file__).resolve().parents[1]
        relative = Path("crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid")
        products = sorted((repository / relative).iterdir())
        expected = {f"{i}.map.test-fixture" for i in range(29)} | {
            "LiquidType.dbc.test-fixture", "AreaTable.dbc.test-fixture"}
        self.assertEqual({path.name for path in products}, expected)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            for product in products:
                destination = source / relative / product.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(product.read_bytes())
            self.seal(source)
            archive = packager.package_source(root, "linux-amd64", SHA)
            with tarfile.open(archive) as output:
                for product in products:
                    name = f"benilla-source-{SHA}/{relative.as_posix()}/{product.name}"
                    self.assertEqual(output.extractfile(name).read(), product.read_bytes())

    def test_native_terrain_source_and_notices_are_required(self):
        required = {
            "crates/benilla-native-terrain/Cargo.toml",
            "crates/benilla-native-terrain/src/main.rs",
            "crates/benilla-native-terrain/src/lib.rs",
            "crates/benilla-native-terrain/src/source.rs",
            "crates/benilla-native-terrain/src/writer.rs",
            "crates/benilla-native-terrain/README.md",
            "crates/benilla-native-terrain/CMANGOS-NOTICE.txt",
            "crates/benilla-sim/licenses/LICENSE",
            "crates/benilla-sim/licenses/GPL-3.0.txt",
            "crates/benilla-sim/licenses/COPYRIGHT.md",
            "crates/benilla-sim/licenses/AUTHORS.md",
            "LICENSE-GPL-3.0",
        }
        self.assertLessEqual(required, set(packager.REQUIRED_SOURCE))
        self.assertIn("crates/benilla-native-terrain/CMANGOS-NOTICE.txt", packager.DOCUMENTS)
        self.assertIn("crates/benilla-native-terrain/README.md", packager.DOCUMENTS)

    def test_native_terrain_corresponding_source_is_preserved_byte_for_byte(self):
        repository = Path(__file__).resolve().parents[1]
        relative = Path("crates/benilla-native-terrain")
        products = sorted(path for path in (repository / relative).rglob("*")
                          if path.is_file() and "__pycache__" not in path.parts)
        self.assertTrue(products)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            for product in products:
                name = product.relative_to(repository)
                packager.safe_path(name.as_posix())
                destination = source / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(product.read_bytes())
            self.seal(source)
            archive = packager.package_source(root, "linux-amd64", SHA)
            with tarfile.open(archive) as output:
                for product in products:
                    name = f"benilla-source-{SHA}/{product.relative_to(repository).as_posix()}"
                    self.assertEqual(output.extractfile(name).read(), product.read_bytes())

    def test_game_asset_extensions_still_fail_even_in_fixture_directory(self):
        fixture = "crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid"
        for directory in ["data", fixture, "crates/benilla-native-terrain/tools/fixtures",
                          "vendor/example-1.0"]:
            for extension in ["mpq", "blp", "m2", "adt", "wdt", "wmo", "dbc"]:
                for suffix in [extension, extension.upper()]:
                    name = f"{directory}/game.{suffix}"
                    with self.subTest(name=name), self.assertRaisesRegex(ValueError, "forbidden source input"):
                        packager.safe_path(name)

    def test_separate_source_all_platforms(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        for platform in packager.TARGETS:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, platform)
                source_archive = packager.package_source(root, platform, SHA)
                before = source_archive.read_bytes()
                with patch.object(packager.subprocess, "Popen", side_effect=AssertionError("must not execute")):
                    archive = packager.package(root, platform, SHA, url)
                prefix = f"benilla-{platform}-{SHA}"
                expected = {f"{prefix}/{p}" for p in [*executables(platform), "config.toml", "README.md"]}
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        self.assertEqual(set(output.namelist()), expected)
                        for name in executables(platform):
                            self.assertEqual(output.read(f"{prefix}/{name}"), f"fixture:{name}".encode())
                        notice = output.read(f"{prefix}/README.md").decode()
                else:
                    with tarfile.open(archive) as output:
                        self.assertEqual(set(output.getnames()), expected)
                        for name in executables(platform):
                            self.assertEqual(output.getmember(f"{prefix}/{name}").mode, 0o755)
                            self.assertEqual(output.extractfile(f"{prefix}/{name}").read(), f"fixture:{name}".encode())
                        notice = output.extractfile(f"{prefix}/README.md").read().decode()
                self.assertIn(url, notice)
                self.assertIn("native diagnostic helpers", notice)
                self.assertIn(SHA, notice)
                self.assertIn(hashlib.sha256(before).hexdigest(), notice)
                self.assertEqual(source_archive.read_bytes(), before)
                self.assertEqual(source_archive.with_name(source_archive.name + ".sha256").read_text(),
                                 f"{hashlib.sha256(before).hexdigest()}  {source_archive.name}\n")

    def test_separate_source_fails_closed(self):
        url = "https://github.com/bunkerlab-net/benilla/actions/runs/123/artifacts/456"
        for failure in ["missing", "corrupt", "changed_source", "invalid_url", "wrong_repo"]:
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                _, _, source = self.fixture(root)
                archive = packager.package_source(root, "linux-amd64", SHA)
                if failure == "missing":
                    archive.unlink()
                elif failure == "corrupt":
                    archive.write_bytes(b"corrupt")
                elif failure == "changed_source":
                    (source / "Cargo.lock").write_text("changed")
                with self.assertRaises(ValueError):
                    packager.package(root, "linux-amd64", SHA,
                                     "https://example.com/source" if failure == "invalid_url" else
                                     url.replace("bunkerlab-net", "wrong-owner") if failure == "wrong_repo" else url)
                self.assertFalse((root / "dist" / f"benilla-linux-amd64-{SHA}.tar.gz").exists())

    def test_packaging_under_symlinked_ancestor(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            real = parent / "real"
            real.mkdir()
            alias = parent / "alias"
            try:
                alias.symlink_to(real, target_is_directory=True)
            except OSError as error:
                self.skipTest(str(error))
            root = alias / "repo"
            self.fixture(root)
            self.assertTrue(packager.package(root, "linux-amd64", SHA).is_file())

    def test_source_script_modes_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            script = source / "scripts/package-player.py"
            script.chmod(0o755)
            if not script.stat().st_mode & 0o111:
                self.skipTest("filesystem does not support POSIX executable bits")
            packager.package(root, "linux-amd64", SHA)
            archive = root / "dist" / f"benilla-linux-amd64-{SHA}-source.tar.gz"
            with tarfile.open(archive) as output:
                self.assertEqual(output.getmember(f"benilla-source-{SHA}/scripts/package-player.py").mode, 0o755)
                self.assertEqual(output.getmember(f"benilla-source-{SHA}/Cargo.toml").mode, 0o644)

    def test_input_directory_symlinks_fail(self):
        for target in ["binary_parent", "source", "source_parent", "source_child"]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                parent = Path(directory)
                root = parent / "repo"
                binary, _, source = self.fixture(root)
                original = {"binary_parent": binary.parent, "source": source,
                            "source_parent": source.parent, "source_child": source / "vendor"}[target]
                moved = parent / "moved"
                original.rename(moved)
                try:
                    original.symlink_to(moved, target_is_directory=True)
                except OSError as error:
                    self.skipTest(str(error))
                with self.assertRaises(ValueError):
                    packager.package(root, "linux-amd64", SHA)
                self.assertFalse((root / "dist").exists())

    def test_regular_rejects_paths_outside_trusted_root(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            root = parent / "repo"
            root.mkdir()
            outside = parent / "outside"
            outside.write_text("must not ship", encoding="utf-8")
            for path in [outside, root / ".." / "outside"]:
                with self.subTest(path=path), self.assertRaises(ValueError):
                    packager.regular(path, root)

    def test_required_inputs_fail_closed(self):
        for missing in [*executables("linux-amd64"), packager.MANIFEST, *packager.REQUIRED_SOURCE,
                        "vendor/fake-1.0/src/lib.rs", "vendor/fake-1.0/.cargo-checksum.json"]:
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary, _, source = self.fixture(root)
                (binary.parent / missing if missing in executables("linux-amd64") else source / missing).unlink()
                with self.assertRaises(ValueError):
                    packager.package(root, "linux-amd64", SHA)
                self.assertFalse((root / "dist").exists())

    def test_changed_source_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            (source / "Cargo.lock").write_text("changed")
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)

    def test_commit_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            data = json.loads((source / packager.MANIFEST).read_text())
            data["commit"] = "b" * 40
            (source / packager.MANIFEST).write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)

    def test_invalid_commit_fails(self):
        with self.assertRaises(ValueError):
            packager.package(Path("."), "linux-amd64", "../bad")

    def test_forbidden_inputs_fail(self):
        for name in ["../escape", "/absolute", "a\\b", "WoW/Data/game.MPQ", "Data/game.mpq",
                     "benilla-config/config.toml", "config.toml", ".probe-identity", ".env", ".git/config"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                packager.safe_path(name)
        for name in ["crates/benilla-config", "crates/benilla-config/src/lib.rs", "config.example.toml",
                     "vendor/convert_case-0.10.0/.envrc", "vendor/voracious_radix_sort-1.2.0/.env",
                     "vendor/windows-0.62.2/src/Windows/Security/Credentials/mod.rs"]:
            packager.safe_path(name)

    def test_source_symlinks_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            try:
                (source / "link").symlink_to(root / "config.toml")
            except OSError as error:
                self.skipTest(str(error))
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)

    def test_binary_symlinks_fail(self):
        for name in executables("linux-amd64"):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                player, _, _ = self.fixture(root)
                binary = player.parent / name
                binary.unlink()
                try:
                    binary.symlink_to(root / "config.toml")
                except OSError as error:
                    self.skipTest(str(error))
                with self.assertRaises(ValueError):
                    packager.package(root, "linux-amd64", SHA)
                self.assertFalse((root / "dist").exists())

    def test_no_vendored_dependency_fails_even_if_resealed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, _, source = self.fixture(root)
            (source / "vendor/fake-1.0/.cargo-checksum.json").unlink()
            self.seal(source)
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)

    def test_prepare_refuses_dirty_or_mismatched_commit(self):
        for responses in [[b"b" * 40], [SHA.encode(), b" M Cargo.toml"]]:
            with tempfile.TemporaryDirectory() as directory, patch.object(packager, "run", side_effect=responses):
                with self.assertRaises(ValueError):
                    packager.prepare_source(Path(directory), SHA)

    def test_private_profile_and_backup_never_enter_source_bundle(self):
        for path in ["profile.json", "other/profile.previous.json", "p2p-profile/profile.lock", "p2p-profile/notes.txt"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                packager.safe_path(path)
        packager.safe_path("crates/benilla-profile/src/lib.rs")
        packager.safe_path("crates/benilla-profile/README.md")

    def test_private_world_saves_never_enter_source_bundle(self):
        for path in ["world.checkpoint", "other/world.previous", "world.lock",
                     "other/.world-checkpoint.pending", "p2p-worlds/notes.txt"]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                packager.safe_path(path)
        packager.safe_path("crates/benilla-world-store/src/lib.rs")
        packager.safe_path("crates/benilla-world-runtime/README.md")

    def test_source_path_is_stable_and_outside_build_cache(self):
        root = Path("repo")
        source = packager.source_dir(root, SHA)
        self.assertEqual(source, root / ".player-source" / "current")
        self.assertEqual(source, packager.source_dir(root, "b" * 40))
        self.assertNotIn(root / "target", source.parents)
        with self.assertRaises(ValueError):
            packager.source_dir(root, "../bad")

    def test_prepare_refuses_existing_staging_for_any_commit(self):
        for commit in [SHA, "b" * 40]:
            with self.subTest(commit=commit), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                _, _, source = self.fixture(root)
                before = (source / packager.MANIFEST).read_bytes()
                with patch.object(packager, "run", side_effect=[commit.encode(), b""]) as run:
                    with self.assertRaisesRegex(ValueError, "source staging already exists"):
                        packager.prepare_source(root, commit)
                self.assertEqual(run.call_count, 2)
                self.assertEqual((source / packager.MANIFEST).read_bytes(), before)

    def test_prepare_snapshots_vendors_and_validates_offline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            blob = io.BytesIO()
            with tarfile.open(fileobj=blob, mode="w") as archive:
                for name in packager.REQUIRED_SOURCE:
                    if name in {".cargo/vendor-config.toml", "BUILD-INFO.txt"}:
                        continue
                    info = tarfile.TarInfo(name)
                    info.size = 7
                    archive.addfile(info, io.BytesIO(b"fixture"))
                fixtures = Path(__file__).resolve().parents[1] / "crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid"
                for product in sorted(fixtures.iterdir()):
                    archive.add(product, arcname="crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid/" + product.name)
            commands = []
            def fake_run(cwd, *args):
                commands.append(args)
                if args[:2] == ("git", "rev-parse"):
                    return SHA.encode()
                if args[:2] == ("git", "status"):
                    return b""
                if args[:2] == ("git", "archive"):
                    return blob.getvalue()
                if args[:2] == ("cargo", "vendor"):
                    dep = cwd / "vendor/fake-1.0"
                    dep.mkdir(parents=True)
                    (dep / ".cargo-checksum.json").write_text("{}")
                    (dep / "Cargo.toml").write_text("fixture")
                    return b'[source.crates-io]\nreplace-with = "vendored-sources"\n[source.vendored-sources]\ndirectory = "vendor"\n'
                return b"fixture"
            with patch.object(packager, "run", side_effect=fake_run):
                source = packager.prepare_source(root, SHA)
            packager.validate_source(source, SHA)
            for product in fixtures.iterdir():
                staged = source / "crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid" / product.name
                self.assertEqual(staged.read_bytes(), product.read_bytes())
            self.assertIn(("cargo", "vendor", "--locked", "--versioned-dirs", "vendor"), commands)
            self.assertIn(("cargo", "--config", ".cargo/vendor-config.toml", "metadata", "--offline", "--locked", "--format-version", "1"), commands)

    def test_vendor_config_resolves_inside_relocated_source(self):
        # Real Cargo resolution catches the two-level --config path rule that mocks missed.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve() / "relocated-source"
            (root / ".cargo").mkdir(parents=True)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("")
            (root / "Cargo.toml").write_text('[package]\nname="packaging-probe"\nversion="0.1.0"\nedition="2021"\n[dependencies]\npackaging-vendor-probe="=1.0.0"\n')
            dep = root / "vendor/packaging-vendor-probe-1.0.0"
            (dep / "src").mkdir(parents=True)
            (dep / "src/lib.rs").write_text("")
            (dep / "Cargo.toml").write_text('[package]\nname="packaging-vendor-probe"\nversion="1.0.0"\nedition="2021"\n')
            (dep / ".cargo-checksum.json").write_text('{"files":{},"package":null}')
            (root / ".cargo/vendor-config.toml").write_text('[source.crates-io]\nreplace-with="vendored-sources"\n[source.vendored-sources]\ndirectory="vendor"\n')
            result = subprocess.run(["cargo", "--config", ".cargo/vendor-config.toml", "metadata", "--offline", "--format-version", "1"], cwd=root, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            resolved = json.loads(result.stdout)
            vendor = next(p for p in resolved["packages"] if p["name"] == "packaging-vendor-probe")
            self.assertEqual(Path(vendor["manifest_path"]), dep / "Cargo.toml")


class HelperWorkflowTests(unittest.TestCase):
    """Guard the fast lane without treating it as full-workspace acceptance."""

    def test_player_docs_name_the_missing_navigation_helper_failure(self):
        root = Path(__file__).resolve().parents[1]
        builds = (root / "docs/BUILDS.md").read_text(encoding="utf-8")
        self.assertIn("optional stage reports `failed` with code `navigation.helper_missing`", builds)
        self.assertNotIn("optional stage reports unavailable", builds)

    def test_player_and_diagnostics_are_distinct_downloads_with_source_first(self):
        root = Path(__file__).resolve().parents[1]
        player = (root / ".github/workflows/build.yml").read_text().split("\n  terrain-helper-windows:\n")[0]
        source = player.index("id: source")
        retention = player.index("python3 scripts/verify-source-retention.py")
        for argument in ('--platform "$BUILD_PLATFORM" --commit "$GITHUB_SHA" --source-url "$SOURCE_URL"',
                         '--diagnostics-only --platform "$BUILD_PLATFORM" --commit "$GITHUB_SHA" --source-url "$SOURCE_URL"'):
            self.assertLess(source, retention)
            self.assertLess(retention, player.index("python3 scripts/package-player.py " + argument))
        self.assertIn("name: benilla-diagnostics-${{ matrix.platform }}-${{ github.sha }}", player)
        self.assertIn("dist/benilla-diagnostics-${{ matrix.platform }}-${{ github.sha }}.${{ matrix.archive }}", player)

    def test_independent_helper_uses_prepared_source_and_publishes_source_first(self):
        root = Path(__file__).resolve().parents[1]
        workflow = (root / ".github/workflows/build.yml").read_text()
        player, helper = workflow.split("\n  terrain-helper-windows:\n")
        self.assertNotIn("needs:", helper)
        self.assertNotIn("continue-on-error:", helper)
        self.assertNotIn("if:", helper)
        self.assertIn("runs-on: windows-2025", helper)
        self.assertIn("BUILD_TARGET: x86_64-pc-windows-msvc", helper)
        self.assertIn("BUILD_PLATFORM: windows-amd64", helper)
        for action in ("actions/checkout", "jdx/mr-boxington-action", "actions/upload-artifact"):
            pins = set(re.findall(re.escape(action) + r"@[0-9a-f]{40}", player))
            self.assertTrue(pins)
            self.assertEqual(set(re.findall(re.escape(action) + r"@[0-9a-f]{40}", helper)), pins)
        commands = [line.strip() for line in helper.splitlines() if line.strip().startswith("mbx ")]
        self.assertEqual(len(commands), 2)
        self.assertEqual(commands[0], 'mbx test --config .cargo/vendor-config.toml --offline --locked --no-default-features --no-fail-fast -p benilla-mpq -p benilla-native-terrain --target "$BUILD_TARGET"')
        self.assertEqual(commands[1], 'mbx build --config .cargo/vendor-config.toml --offline --locked --release -p benilla-native-terrain --bin benilla-native-terrain --no-default-features --target "$BUILD_TARGET"')
        self.assertEqual(helper.count("cd .player-source/current"), 2)
        self.assertIn("Set-Location .player-source/current", helper)
        for executable in ("powershell.exe", "pwsh.exe"):
            command = f"& {executable} -NoProfile -File scripts/Test-BenillaTerrainDiagnostics.ps1"
            self.assertIn(command, helper)
            self.assertLess(helper.index(command), helper.index(commands[0]))
        self.assertEqual(helper.count("if ($LASTEXITCODE -ne 0)"), 2)
        self.assertIn("scripts/Collect-BenillaTerrainDiagnostics.ps1", packager.DOCUMENTS)
        self.assertIn("scripts/Test-BenillaTerrainDiagnostics.ps1", packager.REQUIRED_SOURCE)
        ordered = (
            '--prepare-source --commit "$GITHUB_SHA"', commands[0], commands[1],
            '--source-only --platform "$BUILD_PLATFORM" --commit "$GITHUB_SHA"',
            "name: benilla-native-terrain-source-windows-amd64-${{ github.sha }}",
            "python3 scripts/verify-source-retention.py",
            '--helper-only --platform "$BUILD_PLATFORM" --commit "$GITHUB_SHA" --source-url "$SOURCE_URL"',
            "name: benilla-native-terrain-windows-amd64-${{ github.sha }}",
        )
        positions = [helper.index(text) for text in ordered]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("SOURCE_ID: ${{ steps.source.outputs.artifact-id }}", helper)
        self.assertIn("SOURCE_URL: ${{ steps.source.outputs.artifact-url }}", helper)
        self.assertEqual(helper.count("if-no-files-found: error"), 2)
        self.assertEqual(re.findall(r"retention-days: (\d+)", helper), ["90", "14"])
        for name in ("benilla-native-terrain-source-windows-amd64", "benilla-native-terrain-windows-amd64"):
            self.assertNotIn("name: " + name, player)
        self.assertIn("dist/benilla-windows-amd64-${{ github.sha }}-source.tar.gz.sha256", helper)
        self.assertIn("dist/benilla-native-terrain-windows-amd64-${{ github.sha }}.zip.sha256", helper)


def case_insensitive_path_collisions(names):
    seen = {}
    collisions = set()
    for name in names:
        parts = name.split("/")
        for end in range(1, len(parts) + 1):
            path = "/".join(parts[:end])
            previous = seen.setdefault(path.casefold(), path)
            if previous != path:
                collisions.add(tuple(sorted((previous, path))))
    return sorted(collisions)


class SourceCheckoutTests(unittest.TestCase):
    maxDiff = None

    def test_collision_probe_checks_files_and_ancestor_directories(self):
        self.assertEqual(case_insensitive_path_collisions([
            "models/Foo.m2/inputs.json", "models/Foo.M2/inputs.json",
            "other/Upper/one.json", "other/upper/two.json",
        ]), [
            ("models/Foo.M2", "models/Foo.m2"),
            ("models/Foo.M2/inputs.json", "models/Foo.m2/inputs.json"),
            ("other/Upper", "other/upper"),
        ])
        self.assertEqual(case_insensitive_path_collisions([
            "models/lower_m2/inputs.json", "models/upper_m2/inputs.json",
            "models/lower_m2/inputs.json",
        ]), [])

    def test_repository_source_paths_survive_case_insensitive_checkout(self):
        root = Path(__file__).resolve().parents[1]
        names = subprocess.check_output([
            "git", "ls-files", "--cached", "--others", "--exclude-standard", "-z",
        ], cwd=root).split(b"\0")
        # Include pending additions and exclude removals so an unstaged rename
        # can be checked too. Ancestor casing matters even for distinct files.
        names = [os.fsdecode(name) for name in names
                 if name and os.path.lexists(root / os.fsdecode(name))]
        self.assertEqual(case_insensitive_path_collisions(names), [],
                         "source paths must coexist on case-insensitive filesystems")


class OracleCheckoutTests(unittest.TestCase):
    """Keep byte-pinned inputs intact without disabling ordinary CRLF checkouts."""

    @classmethod
    def setUpClass(cls):
        cls.repository = Path(__file__).resolve().parents[1]
        cls.tools_path = Path("crates/benilla-sim/tools")
        for name in ("event_processor", "spell_event"):
            path = cls.repository / cls.tools_path / f"generate_{name}_fixture.py"
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            setattr(cls, name, module)
        manifest = json.loads((cls.spell_event.SOURCE / "source_manifest.json").read_bytes())
        cls.source_paths = [
            *(cls.tools_path / "event_processor_source" / name for name in cls.event_processor.HASHES),
            *(cls.tools_path / "spell_event_source" / name for name in manifest["files"]),
            cls.tools_path / "spell_event_source/source_manifest.json",
        ]
        cls.pinned_paths = [*cls.source_paths, *(
            Path("crates/benilla-sim/tests/fixtures") / name for name in (
                "relations_reputation_cmangos.sha256", "lifecycle_reputation_cmangos.sha256"))]

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.original = {path: (self.repository / path).read_bytes() for path in self.pinned_paths}
        self.original[Path(".gitattributes")] = (self.repository / ".gitattributes").read_bytes()
        self.control = Path("unprotected.txt")
        self.original[self.control] = b"ordinary text\nstill uses checkout line endings\n"
        for path, data in self.original.items():
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        # Per-command settings and an empty template keep the probe independent of
        # developer Git configuration, hooks, attributes, and any enclosing repo.
        self.git_env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        self.git_env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, GIT_ATTR_NOSYSTEM="1")
        self.git("init", "--quiet", "--template=")
        self.git("add", "--", ".")

    def git(self, *args, autocrlf="false"):
        result = subprocess.run([
            "git", "-c", f"core.autocrlf={autocrlf}", "-c", "core.safecrlf=false",
            "-c", f"core.attributesFile={os.devnull}", *args,
        ], cwd=self.root, env=self.git_env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def checkout(self, attributes=True):
        if not attributes:
            self.git("rm", "--cached", "--", ".gitattributes")
        for path in self.original:
            (self.root / path).unlink()
        self.git("checkout", "--", ".", autocrlf="true")
        self.assertEqual((self.root / self.control).read_bytes(),
                         self.original[self.control].replace(b"\n", b"\r\n"))

    def checked_spell_sources(self):
        with patch.object(self.spell_event, "ROOT", self.root / self.tools_path), \
             patch.object(self.spell_event, "SOURCE", self.root / self.tools_path / "spell_event_source"):
            self.spell_event.checked_sources(None)

    def test_pinned_bytes_and_source_guards_survive_crlf_checkout(self):
        self.checkout()
        for path in self.pinned_paths:
            with self.subTest(path=path):
                self.assertEqual((self.root / path).read_bytes(), self.original[path])
        for name, digest in self.event_processor.HASHES.items():
            source = self.root / self.tools_path / "event_processor_source" / name
            self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), digest)
        self.checked_spell_sources()

    def test_removing_attributes_reproduces_source_guard_failures(self):
        self.checkout(attributes=False)
        for path in self.pinned_paths:
            with self.subTest(path=path):
                self.assertEqual((self.root / path).read_bytes(), self.original[path].replace(b"\n", b"\r\n"))
                self.assertNotEqual((self.root / path).read_bytes(), self.original[path])
        with self.assertRaisesRegex(SystemExit, "pinned source manifest hash mismatch"):
            self.checked_spell_sources()
        manifest = self.tools_path / "spell_event_source/source_manifest.json"
        (self.root / manifest).write_bytes(self.original[manifest])
        with self.assertRaisesRegex(SystemExit, "pinned source hash mismatch:"):
            self.checked_spell_sources()
        for path in self.source_paths:
            if path.parent.name == "spell_event_source":
                (self.root / path).write_bytes(self.original[path])
        with self.assertRaisesRegex(SystemExit, "pinned queue source hash mismatch:"):
            self.checked_spell_sources()
        for name, digest in self.event_processor.HASHES.items():
            source = self.root / self.tools_path / "event_processor_source" / name
            self.assertNotEqual(hashlib.sha256(source.read_bytes()).hexdigest(), digest)

    def test_source_byte_mutations_still_fail_after_protected_checkout(self):
        self.checkout()
        self.checked_spell_sources()
        for path in self.source_paths:
            with self.subTest(path=path):
                data = self.original[path]
                (self.root / path).write_bytes(bytes([data[0] ^ 1]) + data[1:])
                try:
                    with self.assertRaisesRegex(SystemExit, "pinned .*hash mismatch"):
                        self.checked_spell_sources()
                finally:
                    (self.root / path).write_bytes(data)
        self.checked_spell_sources()


class SourceRetentionTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location("source_retention", Path(__file__).with_name("verify-source-retention.py"))
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.now = datetime(2026, 10, 13, tzinfo=timezone.utc)

    def response(self, status=0, **changes):
        artifact = {"id": 123, "expired": False, "expires_at": "2027-01-11T00:00:00Z"}
        artifact.update(changes)
        return subprocess.CompletedProcess([], int(status != 0), json.dumps(artifact),
                                           f"gh: service response (HTTP {status})" if status else "")

    def verify(self, run, sleep):
        return self.module.verify("bunkerlab-net/benilla", "123", run=run, sleep=sleep, now=self.now)

    def test_artifact_visibility_retry_keeps_exact_identity(self):
        run = Mock(side_effect=[self.response(404), self.response()])
        sleep = Mock()
        self.verify(run, sleep)
        self.assertEqual(run.call_count, 2)
        self.assertEqual(run.call_args.args[0], ["gh", "api", "repos/bunkerlab-net/benilla/actions/artifacts/123"])
        sleep.assert_called_once_with(1)

    def test_retries_are_bounded_and_never_accept_missing_source(self):
        run = Mock(return_value=self.response(404))
        sleep = Mock()
        with self.assertRaises(RuntimeError):
            self.verify(run, sleep)
        self.assertEqual(run.call_count, 6)
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [1, 2, 4, 8, 16])

    def test_permission_failure_is_not_retried(self):
        run, sleep = Mock(return_value=self.response(403)), Mock()
        with self.assertRaises(RuntimeError):
            self.verify(run, sleep)
        run.assert_called_once()
        sleep.assert_not_called()

    def test_timeout_and_service_failure_retry_then_verify(self):
        run = Mock(side_effect=[subprocess.TimeoutExpired("gh", 30), self.response(503), self.response()])
        sleep = Mock()
        self.verify(run, sleep)
        self.assertEqual(run.call_count, 3)
        self.assertEqual([call.args[0] for call in sleep.call_args_list], [1, 2])

    def test_invalid_artifact_and_expiry_never_pass_or_retry(self):
        for changes in [{"id": 124}, {"id": True}, {"expired": True},
                        {"expires_at": "2026-10-28T00:00:00Z"},
                        {"expires_at": "2027-01-11T00:00:00"}, {"expires_at": "invalid"}]:
            with self.subTest(changes=changes):
                run, sleep = Mock(return_value=self.response(**changes)), Mock()
                with self.assertRaises(ValueError):
                    self.verify(run, sleep)
                run.assert_called_once()
                sleep.assert_not_called()

    def test_invalid_request_never_invokes_cli(self):
        for repository, artifact in [("../benilla", "123"), ("bunkerlab-net/benilla", "123?x=1"),
                                     ("bunkerlab-net/benilla", "0")]:
            run = Mock()
            with self.assertRaises(ValueError):
                self.module.verify(repository, artifact, run=run)
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
