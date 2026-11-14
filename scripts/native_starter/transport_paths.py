"""Pinned transport keyframes, with map-only exclusion and no guessed spatial margin.
SPDX-License-Identifier: GPL-3.0-or-later
"""
from classic_export.core import digest


def coverage(source, template, dbcs):
    result = {"entry": template["entry"] if template else None,
              "taxi_path_id": template["data0"] if template else None,
              "selector_map": source["selection"]["map_id"], "status": "blocked"}
    def blocked(reason):
        return {**result, "reason": reason}
    if template is None:
        return blocked("template_unknown")
    if template["type"] != 15:
        return blocked("mo_transport_template_required")
    if not {"TaxiPath", "TaxiPathNode"} <= dbcs.keys():
        return blocked("reextract_v5_required")
    path_id = template["data0"]
    if path_id not in dbcs["TaxiPath"].by_id or not path_id:
        return blocked("taxi_path_unknown")
    # DBCStores.cpp:485-508 indexes by path/node index, not physical row order.
    nodes = sorted((r for r in dbcs["TaxiPathNode"].rows if r.words[1] == path_id),
                   key=lambda r: r.words[2])
    result["raw_node_count"] = len(nodes)
    result["nodes_sha256"] = digest([r.raw() for r in nodes])
    if len(nodes) < 4 or any(r.words[0] == 0 or r.words[2] != i for i, r in enumerate(nodes)):
        return blocked("path_node_order_or_coverage_invalid")
    if any(w & 0x7f800000 == 0x7f800000 for r in nodes for w in r.words[4:7]):
        return blocked("path_coordinate_nonfinite")
    if not template["data1"] or not template["data2"]:
        return blocked("path_speed_or_acceleration_invalid")
    if "Map" not in dbcs or any(r.words[3] not in dbcs["Map"].by_id for r in nodes):
        return blocked("path_map_unknown")
    # TransportMgr.cpp:100-124 drops endpoint/control nodes and the pair at each teleport.
    frames, teleports, changing = [], [], False
    for i in range(1, len(nodes) - 1):
        if changing:
            changing = False
        elif nodes[i].words[7] & 1 or nodes[i].words[3] != nodes[i + 1].words[3]:
            if not frames:
                return blocked("path_initial_teleport_invalid")
            teleports.append(frames[-1])
            changing = True
        else:
            frames.append(i)
    if len(frames) < 2 or path_id in (293, 303) and len(frames) <= 12:
        return blocked("path_keyframes_invalid")
    teleports.append(frames[-1])  # Last-to-first is always a teleport, line 139.
    # TransportMgr.cpp:160-189 builds each non-cyclic Catmull-Rom leg separately.
    legs, start = [], 0
    for i in range(1, len(frames)):
        teleport = frames[i - 1] in teleports
        if teleport or i + 1 == len(frames):
            leg = frames[start:i if teleport else i + 1]
            if len(leg) < 2 or len({nodes[j].words[3] for j in leg}) != 1:
                return blocked("path_spline_leg_invalid")
            legs.append(leg)
            start = i
    result.update(keyframe_node_indices=frames, teleport_after_node_indices=teleports,
                  spline_legs=legs, maps=sorted({nodes[i].words[3] for i in frames}))
    if result["selector_map"] not in result["maps"]:
        return {**result, "status": "excluded_other_maps", "reason": "map_disjoint",
                "margin": "not_applicable_map_identity"}
    # Transports.cpp:402-439 does not clamp t. No [0,1] hull or distance margin is proof.
    return blocked("interpolation_parameter_unproved")
