#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Generate authored probes from unchanged hash-pinned CMaNGOS bodies.
Usage: python3 tools/generate_spell_authority_fixture.py /path/to/mangos-classic
No database/game rows. Requires g++ and does not build the full server.
"""
import hashlib
import pathlib
import subprocess
import sys
import tempfile


def body(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end] + "\n"


def main():
    root = pathlib.Path(sys.argv[1])
    unit = root / "src/game/Entities/Unit.cpp"
    util = root / "src/shared/Util/Util.h"
    sources = []
    for path, expected in [(unit,"03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0"),(util,"1f53eafa77ab89935c905f331d9ee7e5e2fabf1fca7f812e2e913f19243c5878")]:
        data = path.read_bytes().replace(b"\r\n", b"\n")
        if hashlib.sha256(data).hexdigest() != expected:
            raise SystemExit("Wrong upstream revision: " + str(path))
        sources.append(data.decode())
    here = pathlib.Path(__file__).resolve().parent
    cpp = (here / "spell_authority_oracle.cpp").read_text()
    for marker, block in {"MISS":body(sources[0],"float Unit::CalculateSpellMissChance("),"CRIT":body(sources[0],"float Unit::CalculateSpellCritChance("),"CHANCE":body(sources[1],"inline uint32 chance_u(float chance)")}.items():
        cpp = cpp.replace("// PINNED_" + marker, block)
    with tempfile.TemporaryDirectory() as directory:
        work = pathlib.Path(directory)
        (work / "oracle.cpp").write_text(cpp)
        subprocess.run(["g++","-std=c++17","-O0","-Wall","-Wextra","-Werror","-fno-fast-math","-ffp-contract=off","-frounding-math","-fsanitize=undefined,float-cast-overflow",str(work/"oracle.cpp"),"-o",str(work/"oracle")],check=True)
        data = subprocess.check_output([str(work/"oracle")]).replace(b"\r\n",b"\n")
    output = here.parent / "src/native/fixtures/spell_authority_cmangos.tsv"
    output.write_bytes(data)
    print(len(data.splitlines()), hashlib.sha256(data).hexdigest())


if __name__ == "__main__":
    main()
