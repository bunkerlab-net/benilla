#!/usr/bin/env python3
"""Read-only ELF evidence inventory. Never executes an inspected executable."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

OBLIGATIONS = {
    1: ("archive_member", ["MPQArchive::MPQArchive", "MPQFile::MPQFile", "libmpq__file_number"]),
    2: ("listfile", ["ExtractWmo", "ExtractDBCFiles", "GetFileListTo"]),
    3: ("first_wins", ["FileExists", "ExtractSingleModel", "ExtractSingleWmo", "Model::open", "ConvertToVMAPModel"]),
    4: ("physical_traversal", ["ParsMapFiles", "WDTFile::init", "GetMap(", "ADTFile::init", "ExtractMaps"]),
    5: ("mclq_history", ["ConvertADT", "liquid_height"]),
    6: ("ids_doodads", ["GenerateUniqueObjectId", "Doodad::ExtractSet"]),
    7: ("assembly_bih", ["TileAssembler::readMapSpawns", "TileAssembler::convertWorld2", "TileAssembler::convertRawFile", "TileAssembler::exportGameobjectModels", "BIH::build", "BIH::subdivide"]),
    8: ("navigation", ["MapBuilder::discoverTiles", "MapBuilder::buildMap(", "MapBuilder::buildTile(", "MapBuilder::buildMoveMapTile", "MapBuilder::buildGameObject", "MapBuilder::buildTransports", "getDirContents"]),
    9: ("files_numerics", ["main"]),
}
EXPECTED = {
    "ad": "c43fb4c71b8fb4706a433a5bc561c482ca473ec31c53b787e7c66e1b3d9d9d35",
    "vmap_extractor": "5dd307ca6d6ae0f51585caa0b05cb0bcac7e2507f0eda0b492e0f239cf8762af",
    "vmap_assembler": "c1883c0c4b1d4e0245fae5ec222c276e883dfc7e62ecec578363da32ab3a5ff7",
    "MoveMapGen": "3f93e21e0d4362ab5dc8c02db907e7b7ec37b9c84e94b69e9f22158f0dbe2d95",
}
REASONS = {
    1: "No exhaustive hash-probe/deque/block-selection machine control-flow cover or per-inline-copy memory-location proof.",
    2: "Allocation bounds, tokenizer cursor, libc wide-load access extents and every continuation/exception path are not pinned.",
    3: "Every inline existence probe, zero-hull branch, deletion path and output lifetime is not pinned.",
    4: "Every physical row/chunk/name/placement loop and rejection edge is not pinned.",
    5: "No exhaustive writer/read dependency cover including hidden packing and other liquid stores; ad has no DWARF.",
    6: "Every inline map/unordered mutation, rehash, traversal and shortened-name store is not pinned.",
    7: "Every inline spawn insertion/duplicate, BIH partition and exception edge is not pinned.",
    8: "Every directory/filter/queue/async and no-output edge is not pinned.",
    9: "Runtime modules, buffered libc calls, resolver/IFUNC/FMA paths and concurrent mapped-file accesses cannot be established by static executable anchors alone.",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def command(*args):
    return subprocess.check_output(args)


def inspect(path, out):
    data = path.read_bytes()
    sha = digest(data)
    if sha != EXPECTED[path.name]:
        raise RuntimeError(f"identity mismatch: {path}")
    if data[:6] != b"\x7fELF\x02\x01":
        raise RuntimeError("unsupported ELF")
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phsize, phnum = struct.unpack_from("<HH", data, 54)
    segments = []
    for i in range(phnum):
        typ, flags, off, va, _, filesz, memsz, align = struct.unpack_from("<IIQQQQQQ", data, phoff + i * phsize)
        if typ == 1:
            segments.append(dict(offset=off, virtual_address=va, file_size=filesz, memory_size=memsz, flags=flags, alignment=align, initial_bytes_sha256=digest(data[off:off+filesz])))
    out.mkdir()
    raw = {}
    for name, args in {
        "elf.txt": ("readelf", "-W", "-h", "-l", "-S", "-n", str(path)),
        "symbols.txt": ("nm", "-C", "-S", "--defined-only", str(path)),
        "disassembly.txt": ("objdump", "-w", "-d", "-C", str(path)),
        "dwarf-info.txt": ("readelf", "--debug-dump=info", "--wide", str(path)),
        "dwarf-locations.txt": ("readelf", "--debug-dump=loc", "--wide", str(path)),
        "dwarf-ranges.txt": ("readelf", "--debug-dump=Ranges", "--wide", str(path)),
        "dwarf-lines.txt": ("readelf", "--debug-dump=decodedline", "--wide", str(path)),
    }.items():
        raw[name] = command(*args)
        (out / name).write_bytes(raw[name])
    build_id = re.search(rb"Build ID: ([0-9a-f]+)", raw["elf.txt"]).group(1).decode()
    symbols = []
    for line in raw["symbols.txt"].decode().splitlines():
        match = re.match(r"([0-9a-f]+) ([0-9a-f]+) ([A-Za-z]) (.*)", line)
        if match:
            start, size, typ, name = match.groups()
            symbols.append((int(start, 16), int(size, 16), typ, name))
    sites = []
    for number, (name, needles) in OBLIGATIONS.items():
        anchors = []
        for start, size, typ, symbol in symbols:
            if not any(n in symbol for n in needles):
                continue
            segment = next((s for s in segments if s["virtual_address"] <= start < s["virtual_address"] + s["file_size"]), None)
            if not segment:
                continue
            offset = segment["offset"] + start - segment["virtual_address"]
            chain = command("addr2line", "-a", "-f", "-C", "-i", "-e", str(path), hex(start)).decode().splitlines()
            anchors.append(dict(symbol=symbol, symbol_type=typ, pc_range=[start, start+size], file_offset=offset, original_first_bytes=data[offset:offset+min(size,16)].hex(), inline_chain_raw=chain, source_status="[INFERENCE] disassembly only; no DWARF" if path.name == "ad" else "DWARF anchor only; full inline ownership unproven", argument_locations="[INFERENCE] SysV AMD64 function entry only: rdi,rsi,rdx,rcx,r8,r9; return rax. Not valid for clones/inline sites without location proof.", memory_locations="unresolved; do not dereference guessed C++ layouts"))
        sites.append(dict(obligation=number, source_site_id=name, status="refused_unpinned", anchors=anchors, reason=REASONS[number], capture_rule="REFUSE before launch. These symbol anchors are not an executable semantic manifest; no obligation is admitted.", conservative_pc_range=[s for s in segments if s["flags"] & 1]))
    return dict(tool=path.name, path=str(path), executable_sha256=sha, size=len(data), build_id=build_id, load_segments=segments, sites=sites, pinned_obligations=0, unpinned_obligations=9)


def classify(root):
    manifest = json.loads((root / "manifest.json").read_text())
    counts = {}
    for tool in manifest["tools"]:
        totals = {"a": 0, "b": 0, "c": 0}
        for site in tool["sites"]:
            dwarf = [anchor for anchor in site["anchors"] if any(re.search(r":\d+(?:\s|$)", line) for line in anchor["inline_chain_raw"]) and tool["tool"] != "ad"]
            category = "a" if dwarf else "b" if site["anchors"] else "c"
            totals[category] += 1
            site["anchor_class"] = category
            site["anchor_class_meaning"] = {"a": "DWARF line and inline-chain anchor", "b": "symbol/disassembly anchor only", "c": "no selected anchor"}[category]
            site["missing_for_exhaustive_coverage"] = site["reason"]
            if category == "c":
                site["missing_anchor"] = "None of the inventoried obligation owner symbols occur as a mapped defined symbol. Find inlined/cold code in the retained disassembly and DWARF; absence of a symbol is not proof of unreachable behavior."
        counts[tool["tool"]] = totals
    manifest["anchor_class_counts"] = counts
    path = root / "anchor-classification.json"
    with path.open("x") as output:
        output.write(json.dumps(manifest, indent=2) + "\n")
    with (root / "anchor-classification.sha256.json").open("x") as output:
        output.write(json.dumps({"path": path.name, "length": path.stat().st_size, "sha256": digest(path.read_bytes())}, indent=2) + "\n")
    print(json.dumps(counts))


def bind_sources(root):
    manifest = json.loads((root / "anchor-classification.json").read_text())
    tool = next(tool for tool in manifest["tools"] if tool["tool"] == "vmap_assembler")
    reconstruction = next(parent for parent in Path(tool["path"]).parents if (parent / "source-inventory.json").is_file())
    source = reconstruction / "source"
    result = {"schema_version": 1, "admission": "refused", "source_root": str(source), "source_inventory_sha256": digest((reconstruction / "source-inventory.json").read_bytes()), "source_identity_receipt_sha256": digest((reconstruction / "source-identity.json").read_bytes()), "bindings": []}
    for tool in manifest["tools"]:
        for site in tool["sites"]:
            for anchor in site["anchors"]:
                locations = []
                for line in anchor["inline_chain_raw"]:
                    match = re.match(r"(.+?):([1-9]\d*)(?:\s|$)", line)
                    if not match:
                        continue
                    name, number = match.groups()
                    local = source / name.removeprefix("/reference/source/") if name.startswith("/reference/source/") else None
                    locations.append({"dwarf_path": name, "line": int(number), "local_source": str(local) if local else None, "sha256": digest(local.read_bytes()) if local and local.is_file() else None, "unresolved_reason": None if local and local.is_file() else "External/generated header bytes need immutable builder-module evidence."})
                result["bindings"].append({"tool": tool["tool"], "site": site["source_site_id"], "symbol": anchor["symbol"], "pc_range": anchor["pc_range"], "source_locations": locations, "source_status": "DWARF interpretation using reconstructed original source; no exhaustive ownership proof" if locations else "[INFERENCE] symbol/disassembly only, source location cannot be pinned"})
    path = root / "source-bindings.json"
    with path.open("x") as output:
        output.write(json.dumps(result, indent=2) + "\n")
    with (root / "source-bindings.sha256.json").open("x") as output:
        output.write(json.dumps({"path": path.name, "length": path.stat().st_size, "sha256": digest(path.read_bytes())}, indent=2) + "\n")
    print(json.dumps({"source_bindings": len(result["bindings"]), "sha256": digest(path.read_bytes())}))


def library_imports(root, retained=False):
    manifest = json.loads((root / "manifest.json").read_text())
    result = {"schema_version": 1, "target_executed": False, "runtime_modules_and_ifunc_targets": "unobserved; DT_NEEDED is not a runtime mapping receipt", "tools": []}
    for tool in manifest["tools"]:
        path = Path(tool["path"])
        if digest(path.read_bytes()) != tool["executable_sha256"]:
            raise RuntimeError(f"changed executable: {path}")
        raw_path = root / tool["tool"] / "dynamic-imports.txt"
        if retained:
            raw = raw_path.read_bytes()
        else:
            raw = command("readelf", "-W", "-h", "-d", "--dyn-syms", str(path))
            with raw_path.open("xb") as output:
                output.write(raw)
        text = raw.decode()
        needed = re.findall(r"\(NEEDED\).*Shared library: \[(.*?)\]", text)
        undefined = sorted(set(match.group(1).split("@")[0] for match in re.finditer(r"\bUND[ \t]+(\S+)", text)))
        numeric = [name for name in undefined if re.match(r"^(acos|asin|atan|cos|sin|tan|sqrt|pow|ceil|floor|fmod|fabs|exp|log|round|trunc|lrint|llrint|rint|nearbyint|remainder|modf|copysign|hypot|feget|feset|feclear|feraise|fetest)", name)]
        interpreter = re.search(r"Requesting program interpreter: ([^\]]+)", (root / tool["tool"] / "elf.txt").read_text())
        result["tools"].append({"tool":tool["tool"],"sha256":tool["executable_sha256"],"elf_type":re.search(r"Type:\s+(.+)",text).group(1),"pie_flag_present":bool(re.search(r"FLAGS_1.*PIE",text)),"requested_interpreter":interpreter.group(1) if interpreter else None,"dt_needed":needed,"undefined_dynamic_symbols":undefined,"numeric_import_candidates":numeric,"actual_loaded_libraries":"unobserved, including transitive modules and lazy resolver targets","raw_evidence":{"path":str(raw_path.relative_to(root)),"length":len(raw),"sha256":digest(raw)}})
    name = "dynamic-imports-v2" if retained else "dynamic-imports"
    path = root / f"{name}.json"
    with path.open("x") as output:
        output.write(json.dumps(result, indent=2) + "\n")
    with (root / f"{name}.sha256.json").open("x") as output:
        output.write(json.dumps({"path":path.name,"length":path.stat().st_size,"sha256":digest(path.read_bytes())},indent=2)+"\n")
    print(json.dumps({tool["tool"]: {key: tool[key] for key in ("elf_type","requested_interpreter","dt_needed","numeric_import_candidates")} for tool in result["tools"]},indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--recovered", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--classify-existing", action="store_true")
    parser.add_argument("--bind-existing-sources", action="store_true")
    parser.add_argument("--library-imports-existing", action="store_true")
    parser.add_argument("--library-imports-from-retained", action="store_true")
    args = parser.parse_args()
    if args.library_imports_from_retained:
        library_imports(args.output, retained=True)
        return
    if args.library_imports_existing:
        library_imports(args.output)
        return
    if args.bind_existing_sources:
        bind_sources(args.output)
        return
    if args.classify_existing:
        classify(args.output)
        return
    if not args.reference or not args.recovered:
        parser.error("--reference and --recovered are required for acquisition")
    args.output.mkdir(mode=0o700, parents=True, exist_ok=False)
    tools = [args.reference / name for name in ("ad", "vmap_extractor")] + [args.recovered / name for name in ("vmap_assembler", "MoveMapGen")]
    manifest = dict(schema_version=1, status="refused_incomplete_machine_cover", target_executed=False, tools=[inspect(path, args.output / path.name) for path in tools])
    (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    receipts = []
    for path in sorted(args.output.rglob("*")):
        if path.is_file():
            data = path.read_bytes()
            receipts.append(dict(path=str(path.relative_to(args.output)), length=len(data), sha256=digest(data)))
    (args.output / "sha256-receipts.json").write_text(json.dumps(receipts, indent=2) + "\n")
    print(json.dumps({"manifest_sha256": digest((args.output / "manifest.json").read_bytes()), "files": len(receipts), "pinned": 0, "refused": 36}))


if __name__ == "__main__":
    main()
