"""Resolve the selected root once; reject links and reparse points below it.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from __future__ import annotations

import os
from pathlib import Path
import stat

from .core import check

DIR_FD = (os.name != "nt" and hasattr(os, "O_DIRECTORY") and hasattr(os, "O_NOFOLLOW")
          and os.open in os.supports_dir_fd)


def identity(meta):
    return meta.st_dev, meta.st_ino


def no_link(meta):
    check(not stat.S_ISLNK(meta.st_mode)
          and not getattr(meta, "st_file_attributes", 0) & 0x400, "catalog.path")


def component(name):
    name = os.fspath(name)
    check(name not in {"", ".", ".."} and not any(c in name for c in "/\\:\0"), "catalog.path")
    if os.name == "nt":
        stem = name.split(".", 1)[0].lower()
        check(not name.endswith((".", " ")) and stem not in {"con", "prn", "aux", "nul"}
              and stem not in {f"{kind}{n}" for kind in ("com", "lpt") for n in range(1, 10)}, "catalog.path")
    return name


class Directory:
    """A retained POSIX directory handle or a checked portable directory identity."""
    def __init__(self, path, parent=None):
        self.fd = None
        self.parent = parent
        try:
            if parent is None:
                self.path = Path(path).resolve(strict=True)
            else:
                parent.check()
                self.path = parent.path / component(path)
            before = self.path.lstat()
            no_link(before)
            check(stat.S_ISDIR(before.st_mode), "catalog.path")
            self.identity = identity(before)
            if DIR_FD:
                flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
                self.fd = os.open(self.path if parent is None else component(path), flags,
                                  **({} if parent is None else {"dir_fd": parent.fd}))
                check(identity(os.fstat(self.fd)) == self.identity, "catalog.changed")
            self.check()
        except OSError:
            self.close()
            check(False, "catalog.path")
        except BaseException:
            self.close()
            raise

    def check(self):
        if self.parent is not None:
            self.parent.check()
        try:
            current = self.path.lstat()
            no_link(current)
            check(stat.S_ISDIR(current.st_mode) and identity(current) == self.identity, "catalog.changed")
            if self.fd is not None:
                check(identity(os.fstat(self.fd)) == self.identity, "catalog.changed")
        except OSError:
            check(False, "catalog.path")

    def child(self, name):
        return Directory(name, self)

    def open_file(self, name, create=False):
        name = component(name)
        self.check()
        fd = None
        try:
            before = None if create else (self.path / name).lstat()
            if before is not None:
                no_link(before)
                check(stat.S_ISREG(before.st_mode), "catalog.file")
            flags = (os.O_WRONLY | os.O_CREAT | os.O_EXCL) if create else os.O_RDONLY
            flags |= getattr(os, "O_BINARY", 0) | getattr(os, "O_NONBLOCK", 0)
            if DIR_FD:
                fd = os.open(name, flags | os.O_NOFOLLOW, 0o600, dir_fd=self.fd)
            else:
                fd = os.open(self.path / name, flags, 0o600)
            opened = os.fstat(fd)
            no_link(opened)
            check(stat.S_ISREG(opened.st_mode), "catalog.file")
            after = (self.path / name).lstat()
            no_link(after)
            check(identity(opened) == identity(after)
                  and (before is None or identity(before) == identity(opened)), "catalog.changed")
            self.check()
            result, fd = fd, None
            return result
        except FileExistsError:
            raise
        except OSError:
            check(False, "catalog.path")
        finally:
            if fd is not None:
                os.close(fd)

    def sync(self):
        self.check()
        # Python cannot open/fsync Windows directories without a native API.
        if self.fd is not None:
            os.fsync(self.fd)

    def link(self, source, target):
        source, target = component(source), component(target)
        fd = self.open_file(source)
        try:
            if DIR_FD:
                os.link(source, target, src_dir_fd=self.fd, dst_dir_fd=self.fd, follow_symlinks=False)
            else:
                os.link(self.path / source, self.path / target)
            self.check()
            linked = (self.path / target).lstat()
            no_link(linked)
            check(identity(linked) == identity(os.fstat(fd)), "catalog.changed")
        finally:
            os.close(fd)

    def unlink(self, name):
        name = component(name)
        self.check()
        no_link((self.path / name).lstat())
        if DIR_FD:
            os.unlink(name, dir_fd=self.fd)
        else:
            os.unlink(self.path / name)
        self.check()

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None

    def __del__(self):
        self.close()

    def __truediv__(self, relative):
        return RootedPath(self, relative)

    def __fspath__(self):
        return os.fspath(self.path)


class RootedPath:
    def __init__(self, root, relative):
        self.root = root
        self.parts = Path(relative).parts
        check(self.parts and not Path(relative).is_absolute(), "catalog.path")
        for part in self.parts:
            component(part)

    def open(self):
        directories = []
        parent = self.root
        try:
            for part in self.parts[:-1]:
                parent = parent.child(part)
                directories.append(parent)
            return os.fdopen(parent.open_file(self.parts[-1]), "rb")
        finally:
            for directory in reversed(directories):
                directory.close()


def open_regular(path):
    if not isinstance(path, RootedPath):
        path = Path(path)
        path = Directory(path.parent) / path.name
    return path.open()


def read_at(fd, size, offset):
    pread = getattr(os, "pread", None)
    if pread is not None:
        return pread(fd, size, offset)
    # Catalog owns its cursor and is already a serial, mutable reader.
    os.lseek(fd, offset, os.SEEK_SET)
    return os.read(fd, size)
