# SPDX-License-Identifier: GPL-3.0-or-later
"""Authored accessibility inputs; results come only from the pinned source body."""
import itertools


def cases():
    rows = []

    def add(name, flags, swim, walk, fly):
        rows.append(dict(name=name, input=dict(environment_flags=flags,
            can_swim=swim, can_walk=walk, can_fly=fly)))

    for water, swim, walk, fly in itertools.product([False, True], repeat=4):
        add(f"water_{int(water)}_swim_{int(swim)}_walk_{int(walk)}_fly_{int(fly)}",
            int(water), swim, walk, fly)
    for flags in [2, 4, 8, 16, 32, 64, 126, 0xffffffff]:
        add(f"environment_mask_{flags}_walker", flags, False, True, False)
        add(f"environment_mask_{flags}_swimmer", flags, True, False, False)
    return rows


def command_stream(rows):
    return ("\n".join(" ".join(map(str, [row["name"],
        row["input"]["environment_flags"], int(row["input"]["can_swim"]),
        int(row["input"]["can_walk"]), int(row["input"]["can_fly"])]))
        for row in rows) + "\n").encode()
