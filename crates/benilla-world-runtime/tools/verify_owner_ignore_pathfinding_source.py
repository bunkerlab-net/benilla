#!/usr/bin/env python3
"""Guard reviewed pinned source for the bounded owner bit; not a lifecycle oracle."""
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1]) / "src/game"
expected = {
    "Entities/Unit.h": "2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1",
    "Entities/Unit.cpp": "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "Entities/Creature.cpp": "27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302",
    "MotionGenerators/HomeMovementGenerator.cpp": "d225dd76bd02a33fcfdd17dc0d8a2a953f60f99349f59224e20ef678dbb144ac",
    "MotionGenerators/TargetedMovementGenerator.cpp": "eaa569e520a7b7a37d3011822e0381d41ab49b9f2dc6ffa1e43d6344bc14ff15",
    "MotionGenerators/PathFinder.cpp": "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef",
}
source = {}
for relative, digest in expected.items():
    data = (root / relative).read_bytes().replace(b"\r\n", b"\n")
    if hashlib.sha256(data).hexdigest() != digest:
        raise SystemExit(f"pinned LF-normalized source hash mismatch: {relative}")
    source[relative] = data.decode()

checks = {
    "Entities/Unit.h": [
        "UNIT_STAT_IGNORE_PATHFINDING    = 0x10000000",
        "bool hasUnitState(uint32 f) const { return (m_state & f) != 0; }",
        "UNIT_STAT_ALL_DYN_STATES  = UNIT_STAT_ALL_STATE & ~(UNIT_STAT_NO_COMBAT_MOVEMENT | UNIT_STAT_RUNNING | UNIT_STAT_WAYPOINT_PAUSED | UNIT_STAT_IGNORE_PATHFINDING)",
    ],
    "Entities/Unit.cpp": [
        "m_state = 0;", "addUnitState(UNIT_STAT_MELEE_ATTACKING);",
        "clearUnitState(UNIT_STAT_MELEE_ATTACKING);",
    ],
    "MotionGenerators/TargetedMovementGenerator.cpp": [
        "owner.addUnitState(UNIT_STAT_CHASE);",
        "u.addUnitState(UNIT_STAT_CHASE_MOVE);",
        "owner.clearUnitState(UNIT_STAT_CHASE | UNIT_STAT_CHASE_MOVE);",
    ],
    "MotionGenerators/HomeMovementGenerator.cpp": [
        "owner.clearUnitState(static_cast<uint32>(UNIT_STAT_ALL_DYN_STATES));",
    ],
    "MotionGenerators/PathFinder.cpp": [
        "m_sourceUnit->hasUnitState(UNIT_STAT_IGNORE_PATHFINDING)",
    ],
}
for relative, expressions in checks.items():
    for expression in expressions:
        if expression not in source[relative]:
            raise SystemExit(f"missing source contract in {relative}: {expression}")
print(f"verified {len(expected)} pinned source hashes and {sum(map(len, checks.values()))} owner-bit expressions")
