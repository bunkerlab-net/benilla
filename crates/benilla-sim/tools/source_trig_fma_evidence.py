"""Create new trig oracle evidence directories without following symlinks."""
# SPDX-License-Identifier: GPL-3.0-or-later
from pathlib import Path

ASSIGNED = Path(__file__).resolve().parents[3] / 'benilla-config/full-server/f6/trig-fma'


def create_evidence(child, assigned):
    if child.is_absolute() or not child.parts or '..' in child.parts:
        raise SystemExit('Output must be a relative child of the assigned trig-fma directory')
    path = assigned / child
    for component in (path, *path.parents):
        if component.is_symlink():
            raise SystemExit('Evidence path must not contain a symlink')
    root = assigned.resolve()
    resolved = path.resolve()
    if resolved == root or not resolved.is_relative_to(root):
        raise SystemExit('Output must resolve to a relative child of the assigned trig-fma directory')
    try:
        resolved.mkdir(parents=True, exist_ok=False)
    except FileExistsError as error:
        raise SystemExit('Evidence directory already exists; refusing to overwrite receipts') from error
    return resolved
