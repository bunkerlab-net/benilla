#!/usr/bin/env python3
"""Fast packaging tests: fake binaries/source, no compiler or game data."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("package_player", Path(__file__).with_name("package-player.py"))
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)
SHA = "a" * 40


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
                _, executable, source = self.fixture(root, platform)
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
        for name in ["crates/benilla-config", "crates/benilla-config/src/lib.rs", "config.example.toml"]:
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

    def test_prepare_snapshots_vendors_and_validates_offline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            blob = io.BytesIO()
            with tarfile.open(fileobj=blob, mode="w") as archive:
                for name in packager.REQUIRED_SOURCE:
                    if name in {"vendor-config.toml", "BUILD-INFO.txt"}:
                        continue
                    info = tarfile.TarInfo(name)
                    info.size = 7
                    archive.addfile(info, io.BytesIO(b"fixture"))
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
            self.assertIn(("cargo", "vendor", "--locked", "--versioned-dirs", "vendor"), commands)
            self.assertIn(("cargo", "--config", "vendor-config.toml", "metadata", "--offline", "--locked", "--format-version", "1"), commands)


if __name__ == "__main__":
    unittest.main()
