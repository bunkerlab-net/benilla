#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify the canonical defense curve against this platform's unchanged CMaNGOS.

--write establishes a new canonical table and fixtures; --check (the default)
keeps that versioned table and verifies final f32 chances and strict draw edges.
A native libm log-bit difference is reported, never silently rewritten. Any final
chance or semantic RNG divergence fails. No game data or Cargo invocation.
"""
import argparse
import hashlib
import os
import pathlib
import shutil
import re
import shlex
import subprocess
import tempfile

PIN = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
SOURCES = {
    "src/game/Entities/Player.cpp": "d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e",
    "src/shared/Util/Util.h": "1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878",
}
SNIPPETS = {
    "Player.cpp.inc": "6967d4772a3557eb162c68b987be29fdf90bfa36dc20c7d246d14fe4853a0495",
    "Util.h.inc": "d38f94fa7c2c023490cd331afb902008e1b217fa464b2b1367bb6e76f0d2c1a2",
}
LOG_SHA256 = "88a6d0c0b95514fb14dbc7fd369d857f701f6cd13161d73e0853d13823ff426e"
ROOT = pathlib.Path(__file__).resolve().parents[1]
TABLE = ROOT / "src/npc_melee/skill_log.rs"
DIGESTS = ROOT / "tests/fixtures/defense_skill_cmangos.sha256"
EDGES = ROOT / "tests/fixtures/defense_skill_edges_cmangos.tsv"


def body(text, signature):
    start = text.index(signature)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"


def table_text(logs):
    header = """// SPDX-License-Identifier: GPL-3.0-or-later
// Canonical v1: CMaNGOS 8ec338a1, Player::UpdateCombatSkills.
// std::log((room + 5.0) / 12.0), room 7..300, binary64 nearest-even.
// Generated on x86_64 Linux, glibc 2.41, GCC 14.2; not universal libm identity.
// Regenerate/verify with tools/generate_defense_skill_fixture.py; see NPC-MELEE.md.
pub(super) const DEFENSE_GAP_LOG_BITS: [u64; 294] = [
"""
    return header + "".join(f"    0x{bits:016x}, // room {room}\n" for room, bits in logs) + "];\n"


def canonical_text(path):
    raw = path.read_bytes().replace(b"\r\n", b"\n")
    if b"\r" in raw:
        raise SystemExit("Bare carriage return in source/fixture: " + str(path))
    return raw


def checked_text(path, expected):
    raw = canonical_text(path)
    if hashlib.sha256(raw).hexdigest() != expected:
        raise SystemExit("Wrong upstream revision or altered source: " + str(path))
    return raw.decode()


def native_compiler(requested, temp):
    env = dict(os.environ)
    # Hosted Windows does not start in a VS developer shell. Ask the supported
    # installer catalog for VC tools, then import vcvars64's environment.
    if os.name == "nt":
        catalog = pathlib.Path(env.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
        if catalog.is_file():
            install = subprocess.check_output([
                str(catalog), "-latest", "-products", "*", "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"
            ], text=True).strip()
            if install:
                setup = pathlib.Path(install) / "VC/Auxiliary/Build/vcvars64.bat"
                if not setup.is_file():
                    raise SystemExit("VS installation has no vcvars64.bat: " + str(setup))
                batch = temp / "compiler-env.bat"
                batch.write_text(f'@echo off\ncall "{setup}" >nul\nif errorlevel 1 exit /b 1\nset\n')
                output = subprocess.check_output([env.get("COMSPEC", "cmd.exe"), "/d", "/c", str(batch)])
                for line in output.decode("mbcs").splitlines():
                    key, separator, value = line.partition("=")
                    if separator and key:
                        # Environment variable names are case-insensitive on Windows.
                        for old in list(env):
                            if old.upper() == key.upper():
                                del env[old]
                        env[key] = value
    path = next((value for key, value in env.items() if key.upper() == "PATH"), "")
    if requested:
        compiler = shlex.split(requested, posix=os.name != "nt")
        if os.name == "nt":
            compiler = [word[1:-1] if word.startswith('"') and word.endswith('"') else word for word in compiler]
    else:
        names = ("cl", "clang++", "g++") if os.name == "nt" else ("c++", "clang++", "g++")
        found = next((value for name in names if (value := shutil.which(name, path=path))), None)
        if found is None:
            raise SystemExit("No native C++ compiler found; install GCC/Clang or supported MSVC VC tools. Oracle was not run.")
        compiler = [found]
    msvc = pathlib.Path(compiler[0]).stem.lower() in ("cl", "clang-cl")
    if msvc:
        # The unchanged source intentionally narrows the final double to float.
        flags = ["/nologo", "/std:c++17", "/Od", "/W4", "/WX", "/wd4100", "/wd4244", "/fp:strict", "/EHsc"]
    else:
        flags = ["-std=c++17", "-O0", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
                 "-fno-fast-math", "-ffp-contract=off", "-frounding-math"]
        if os.name != "nt":
            flags += ["-fsanitize=undefined,float-cast-overflow", "-fno-sanitize-recover=all"]
    return compiler, flags, env, msvc


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source_root", type=pathlib.Path, nargs="?", help="Optional complete pinned checkout; otherwise use hash-locked unchanged excerpts")
    parser.add_argument("--cxx", help="Native compiler command; detected by default")
    parser.add_argument("--write", action="store_true", help="Reproduce canonical v1 table/fixtures; refuses a different native table")
    parser.add_argument("--check", action="store_true", help="Check only (default)")
    args = parser.parse_args()
    if args.write and args.check:
        parser.error("--write and --check are mutually exclusive")
    excerpts = {name: checked_text(ROOT / "tools/defense_skill_source" / name, digest)
                for name, digest in SNIPPETS.items()}
    if args.source_root:
        source = {name: checked_text(args.source_root / name, digest) for name, digest in SOURCES.items()}
        for name, path, signatures in (
            ("Player.cpp.inc", "src/game/Entities/Player.cpp", (
                "void Player::UpdateCombatSkills(", "void Player::UpdateDefense(", "void Player::UpdateSkill(")),
            ("Util.h.inc", "src/shared/Util/Util.h", ("inline bool roll_chance_f(",)),
        ):
            original = source[path]
            extracted = original[:original.index("*/") + 2] + "\n\n" + "".join(body(original, signature) for signature in signatures)
            if extracted != excerpts[name]:
                raise SystemExit("Bundled method excerpts differ from complete pinned source: " + name)
    harness = pathlib.Path(__file__).with_name("defense_skill_oracle.cpp").read_text()
    for marker, value in {"PINNED_ROLL": excerpts["Util.h.inc"], "PINNED_METHODS": excerpts["Player.cpp.inc"]}.items():
        assert harness.count("// " + marker + "\n") == 1
        harness = harness.replace("// " + marker + "\n", value)
    with tempfile.TemporaryDirectory(prefix="defense-source-oracle-") as temp:
        root = pathlib.Path(temp)
        cpp, exe = root / "oracle.cpp", root / ("oracle.exe" if os.name == "nt" else "oracle")
        compiler, flags, env, msvc = native_compiler(args.cxx, root)

        def compile_with(logs):
            constants = "const uint64_t canonical_log_bits[294] = {" + ",".join(f"{v}ULL" for v in logs) + "};\n"
            cpp.write_text(harness.replace("// CANONICAL_LOG_BITS\n", constants))
            outputs = [str(cpp), "/Fe" + str(exe), "/Fo" + str(root / "oracle.obj")] if msvc else [str(cpp), "-o", str(exe)]
            subprocess.run(compiler + flags + outputs, env=env, cwd=root, check=True)

        def run(mode):
            return subprocess.check_output([str(exe), mode], env=env)

        compile_with([0] * 294)
        native_logs = [tuple(map(int, line.split())) for line in run("logs").decode().splitlines()]
        assert [room for room, _ in native_logs] == list(range(7, 301))
        if args.write:
            canonical_logs = [value for _, value in native_logs]
        else:
            canonical_logs = [int(value, 16) for value in re.findall(r"0x([0-9a-f]{16}),", TABLE.read_text())]
        assert len(canonical_logs) == 294
        if hashlib.sha256(b"".join(value.to_bytes(8, "little") for value in canonical_logs)).hexdigest() != LOG_SHA256:
            raise SystemExit("Canonical v1 log table changed; do not replace it from a different platform or loosen tolerances")
        differences = sum(a != b for a, (_, b) in zip(canonical_logs, native_logs))
        print(f"Native std::log binary64 differences from canonical v1: {differences}/294", flush=True)
        compile_with(canonical_logs)
        data = run("chances")
        expected_cases = 60 * sum(range(1, 301))
        assert len(data) == expected_cases * 5
        per_level = sum(range(1, 301)) * 5
        digests = f"# {PIN}; ascending level, max 1..300, value 0..max-1\n"
        digests += "# SHA-256 of LE-u32 final f32 bits + u8 below/equal/above source-roll mask (2=invalid)\n"
        for level in range(1, 61):
            data_level = data[(level - 1) * per_level:level * per_level]
            digests += f"{level} 45150 {hashlib.sha256(data_level).hexdigest()}\n"
        edges = run("edges")
    if args.write:
        TABLE.write_text(table_text(native_logs))
        DIGESTS.write_text(digests)
        EDGES.write_bytes(edges)
    else:
        for path, result in ((DIGESTS, digests.encode()), (EDGES, edges)):
            if canonical_text(path) != result:
                raise SystemExit(f"Source fixture divergence: {path.name}; do not loosen tolerances")
    print(f"Verified {expected_cases:,} final-f32 cases and three adjacent RNG boundaries each; "
          f"{len(edges.splitlines()) - 1:,} state/roll edge cases")
    print("Compiler:", " ".join(compiler), "; sanitizers:", "UB/float-cast" if os.name != "nt" else "not enabled")


if __name__ == "__main__":
    main()
