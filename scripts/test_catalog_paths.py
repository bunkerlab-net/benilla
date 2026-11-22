"""Filesystem-only catalog path regressions; no database or game content.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import os
from pathlib import Path
import stat
import tempfile
import types
import unittest
from unittest import mock

from classic_export import catalog_paths as paths
from classic_export.catalog_spawns import _open, _regular
from classic_export.core import ExportError


class CatalogPathTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.root = self.base / "root"
        self.root.mkdir()
        (self.root / "nested").mkdir()
        (self.root / "nested" / "rows").write_bytes(b"recorded")
        self.directory = paths.Directory(self.root)
        self.addCleanup(self.directory.close)

    def test_portable_occurrence_helpers_keep_nested_paths_rooted(self):
        with mock.patch.object(paths, "DIR_FD", False):
            self.assertEqual(_regular(self.directory / "nested/rows", 32), b"recorded")
            with _open(self.directory / "nested/rows") as handle:
                self.assertEqual(handle.read(), b"recorded")
            with self.assertRaisesRegex(ExportError, "catalog.path"):
                _open(self.directory / "../root/nested/rows")

    def test_explicit_root_and_ancestor_links_resolve_once(self):
        alias = self.base / "alias"
        alias.symlink_to(self.root, target_is_directory=True)
        for portable in (False, True):
            with self.subTest(portable=portable), mock.patch.object(paths, "DIR_FD", paths.DIR_FD and not portable):
                root = paths.Directory(alias)
                try:
                    self.assertEqual(_regular(root / "nested/rows", 32), b"recorded")
                    self.assertEqual(_regular(alias / "nested/rows", 32), b"recorded")
                finally:
                    root.close()

    def test_links_below_the_root_refuse_in_both_backends(self):
        (self.root / "linked-directory").symlink_to(self.root / "nested", target_is_directory=True)
        (self.root / "nested" / "linked-file").symlink_to(self.root / "nested" / "rows")
        for portable in (False, True):
            with self.subTest(portable=portable), mock.patch.object(paths, "DIR_FD", paths.DIR_FD and not portable):
                for relative in ("linked-directory/rows", "nested/linked-file"):
                    with self.assertRaisesRegex(ExportError, "catalog.path"):
                        _regular(self.directory / relative, 32)

    def test_windows_reparse_attribute_refuses_even_on_a_regular_file(self):
        for mode in (stat.S_IFREG, stat.S_IFDIR):
            with self.assertRaisesRegex(ExportError, "catalog.path"):
                paths.no_link(types.SimpleNamespace(st_mode=mode, st_file_attributes=0x400))

    def test_portable_file_replacement_between_stat_and_open_refuses(self):
        path = self.root / "rows"
        replacement = self.root / "replacement"
        path.write_bytes(b"before")
        replacement.write_bytes(b"after!")
        original_open = os.open
        opened = []
        def replace_before_open(name, *args, **kwargs):
            os.replace(replacement, path)
            fd = original_open(name, *args, **kwargs)
            opened.append(fd)
            return fd
        with mock.patch.object(paths, "DIR_FD", False), mock.patch.object(os, "open", side_effect=replace_before_open):
            with self.assertRaisesRegex(ExportError, "catalog.changed"):
                self.directory.open_file("rows")
        self.assertEqual(len(opened), 1)
        with self.assertRaises(OSError):
            os.fstat(opened[0])

    def test_portable_retained_directory_replacement_refuses(self):
        child = self.directory.child("nested")
        try:
            (self.root / "nested").rename(self.root / "old")
            (self.root / "nested").mkdir()
            with mock.patch.object(paths, "DIR_FD", False):
                with self.assertRaisesRegex(ExportError, "catalog.changed"):
                    child.open_file("rows")
        finally:
            child.close()
