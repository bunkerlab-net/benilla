#!/usr/bin/env python3
"""Packaging and resolved feature-graph tests: no app build or game data."""
import hashlib
import importlib.util
import io
import json
import os
import subprocess
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from datetime import datetime, timezone
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location("package_player", Path(__file__).with_name("package-player.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)
SHA = "a" * 40


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
        for package in ["benilla-app", "benilla-assets", "benilla-formats", "benilla-world"]:
            with self.subTest(package=package):
                self.assertIn(package, features, "the resolved graph must contain the dev seam")
                self.assertEqual("dev" in features[package], enabled,
                                 f"{package}: selected features {sorted(features[package])}")

    def test_player_launcher_does_not_enable_dev_through_dependencies(self):
        self.assert_dev_features(["benilla"], False)

    def test_player_test_roots_do_not_enable_dev_through_dependencies(self):
        self.assert_dev_features(["benilla-formats", "benilla-app"], False,
                                 test_dependencies=True)

    def test_explicit_dev_launcher_forwards_the_dev_seam(self):
        self.assert_dev_features(["benilla"], True)


class PackagingTests(unittest.TestCase):
    def fixture(self, root, platform="linux-amd64"):
        executable = "benilla.exe" if platform == "windows-amd64" else "benilla"
        binary = root / "target" / packager.TARGETS[platform] / "release" / executable
        source = packager.source_dir(root, SHA)
        for path in [binary, *(source / p for p in packager.REQUIRED_SOURCE),
                     source / "vendor/fake-1.0/src/lib.rs", source / "vendor/fake-1.0/LICENSE",
                     source / "vendor/fake-1.0/.cargo-checksum.json"]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture", encoding="utf-8")
        binary.chmod(0o755)
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
                binary, executable, source = self.fixture(root, platform)
                binary.chmod(0o644)  # Windows cannot set POSIX executable bits.
                archive = packager.package(root, platform, SHA)
                prefix = f"benilla-{platform}-{SHA}"
                expected = {f"{prefix}/{p}" for p in [executable, *packager.DOCUMENTS, "corresponding-source.tar.gz"]}
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        self.assertEqual(set(output.namelist()), expected)
                        data = output.read(f"{prefix}/corresponding-source.tar.gz")
                else:
                    with tarfile.open(archive) as output:
                        self.assertEqual(set(output.getnames()), expected)
                        self.assertEqual(output.getmember(f"{prefix}/{executable}").mode, 0o755)
                        data = output.extractfile(f"{prefix}/corresponding-source.tar.gz").read()
                with tarfile.open(fileobj=io.BytesIO(data)) as output:
                    wanted = {f"benilla-source-{SHA}/{p.relative_to(source).as_posix()}" for p in source.rglob("*") if p.is_file()}
                    self.assertEqual(set(output.getnames()), wanted)
                self.assertEqual(archive.with_name(archive.name + ".sha256").read_text(),
                                 f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")

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

    def test_game_asset_extensions_still_fail_even_in_fixture_directory(self):
        fixture = "crates/benilla-native-geometry-tests/tools/fixtures/terrain-liquid"
        for directory in ["data", fixture, "vendor/example-1.0"]:
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
                _, executable, _ = self.fixture(root, platform)
                source_archive = packager.package_source(root, platform, SHA)
                before = source_archive.read_bytes()
                archive = packager.package(root, platform, SHA, url)
                prefix = f"benilla-{platform}-{SHA}"
                expected = {f"{prefix}/{p}" for p in [executable, *packager.DOCUMENTS, "SOURCE-DOWNLOAD.txt"]}
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        self.assertEqual(set(output.namelist()), expected)
                        notice = output.read(f"{prefix}/SOURCE-DOWNLOAD.txt").decode()
                else:
                    with tarfile.open(archive) as output:
                        self.assertEqual(set(output.getnames()), expected)
                        notice = output.extractfile(f"{prefix}/SOURCE-DOWNLOAD.txt").read().decode()
                self.assertIn(url, notice)
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
        for missing in ["binary", packager.MANIFEST, *packager.REQUIRED_SOURCE,
                        "vendor/fake-1.0/src/lib.rs", "vendor/fake-1.0/.cargo-checksum.json"]:
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary, _, source = self.fixture(root)
                (binary if missing == "binary" else source / missing).unlink()
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

    def test_binary_symlink_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, _, _ = self.fixture(root)
            binary.unlink()
            try:
                binary.symlink_to(root / "config.toml")
            except OSError as error:
                self.skipTest(str(error))
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)

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
