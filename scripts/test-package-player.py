#!/usr/bin/env python3
"""Check package contents and checksums without a Rust build or game data."""

import hashlib
import importlib.util
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location(
    "package_player", Path(__file__).with_name("package-player.py")
)
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)
SHA = "a" * 40


class PackagingTests(unittest.TestCase):
    def fixture(self, root, platform):
        executable = "benilla.exe" if platform == "windows-amd64" else "benilla"
        binary = root / "target" / packager.TARGETS[platform] / "release" / executable
        for path in [binary, *(root / p for p in packager.DOCUMENTS)]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture", encoding="utf-8")
        for path in ["WoW/Data/game.MPQ", "benilla-config/config.toml", ".probe-identity"]:
            source = root / path
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text("must not ship", encoding="utf-8")
        return binary, executable

    def test_all_platforms(self):
        for platform in packager.TARGETS:
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                _, executable = self.fixture(root, platform)
                archive = packager.package(root, platform, SHA)
                prefix = f"benilla-{platform}-{SHA}"
                expected = {f"{prefix}/{p}" for p in [executable, *packager.DOCUMENTS]}
                if platform == "windows-amd64":
                    with zipfile.ZipFile(archive) as output:
                        self.assertEqual(set(output.namelist()), expected)
                else:
                    with tarfile.open(archive) as output:
                        self.assertEqual(set(output.getnames()), expected)
                        self.assertEqual(output.getmember(f"{prefix}/{executable}").mode, 0o755)
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                self.assertEqual(
                    archive.with_name(archive.name + ".sha256").read_text(),
                    f"{digest}  {archive.name}\n",
                )

    def test_missing_binary_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, _ = self.fixture(root, "linux-amd64")
            binary.unlink()
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)
            self.assertFalse((root / "dist").exists())

    def test_invalid_commit_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                packager.package(Path(directory), "linux-amd64", "../bad")

    def test_symlink_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary, _ = self.fixture(root, "linux-amd64")
            binary.unlink()
            binary.symlink_to(root / "WoW/Data/game.MPQ")
            with self.assertRaises(ValueError):
                packager.package(root, "linux-amd64", SHA)


if __name__ == "__main__":
    unittest.main()
