# Benilla fork: combined player distribution

This fork links `benilla-session` and `benilla-sim` into the player. The combined
program is distributed under GNU GPL version 3 or (at your option) any later
version. See `LICENSE-GPL-3.0`. There is no warranty, to the extent permitted by
applicable law.

Original Benilla files retain their MIT OR Apache-2.0 grants and copyright
notices; `LICENSE-MIT` and `LICENSE-APACHE` remain applicable to those files.
This notice does not replace or relicense individual third-party files. Dependency
licenses and notices accompany their source in the matching source archive
(see the player `README.md`, or `SOURCE-DOWNLOAD.txt` in a diagnostic download).
The patched Kira and Lua source and notices are included there too.

The simulation contains a partial Rust translation of CMaNGOS Classic XP, selected stat/armor, quest-reward and targeting helpers,
modified by Benilla fork contributors in 2026, from commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. CMaNGOS granted GPL version 2 or later;
this combination uses GPL version 3 or later. Consult
`crates/benilla-sim/PROVENANCE.md` and the unchanged upstream authorship, copyright,
and license documents in `crates/benilla-sim/licenses/`. The synthetic encounter
harness is not a port of full Classic combat or a claim of full gameplay parity.

The app includes eight factual racial starting poses checked against Classic-DB
`ec4f596146be6467ea93c57397858e329e2db852`. It does not include SQL records or a
world-content pack. The upstream authorship, GPLv3 license and separate copyright
qualifications are retained under
`crates/benilla-app/src/net/p2p/creation_start/licenses/`; see
`docs/CLASSIC-EXPLORATION-STARTS.md` for exact scope and source evidence.

The app's bounded dry static VMap ray port derives from the same CMaNGOS revision and
G3D math. Its source scope, GPL and original G3D Modified BSD notices are linked in
`crates/benilla-app/src/net/p2p/world/native_geometry/products/vmap/PROVENANCE.md`.
This is not a full VMap/MMap/PathFinder port or native-content admission.

The simulation's selected current-pose facing profile also translates GNU libc
2.41's CORE-MATH-derived `e_atan2f.c`, under its original MIT grant, Copyright
(c) 2022–2024 Alexei Sibidanov and Paul Zimmermann. Preserve its complete notice
in `crates/benilla-sim/src/facing/MIT-NOTICE.txt`; exact sources and numeric scope
are documented in `crates/benilla-sim/FACING.md`. The combined program remains
GPL-3.0-or-later.

The shared bounded sinf/cosf core used by VMap Euler transforms and initial
and separately bounded fallback near-point arithmetic derives from GNU libc 2.41, Copyright (C) 2018–2025 Free
Software Foundation, Inc., under LGPL-2.1-or-later. Preserve the complete notice
and license in `crates/benilla-sim/src/source_trig/GLIBC-NOTICE.txt` and
`crates/benilla-sim/src/source_trig/GLIBC-LICENSE.txt`. Exact sources and bounded
native/software profiles are documented in `crates/benilla-sim/SOURCE-TRIG.md`.
The original VMap copies of the notice and license remain available too.

The selected binary64 atan core and coefficient table derive from GNU libc 2.41's
IBM Accurate Mathematical Library, Copyright (C) 2001–2025 Free Software
Foundation, Inc., under LGPL-2.1-or-later. Preserve the notice and license in
`crates/benilla-sim/src/source_atan/GLIBC-NOTICE.txt` and
`crates/benilla-sim/src/source_atan/GLIBC-LICENSE.txt`. Its bounded software profile
and source identities are documented in `crates/benilla-sim/SOURCE-ATAN.md`.
The CMaNGOS empty-list near-point selector uses that numeric dependency; this
arithmetic prerequisite does not establish full fallback or pathfinding integration.

The matching build source, locked dependency sources, license texts, build scripts,
and build information are provided through the exact download and SHA-256 in
the player `README.md`, or `SOURCE-DOWNLOAD.txt` in a diagnostic download.
A local player package has its source archive alongside it, not inside it;
local diagnostic packages may embed `corresponding-source.tar.gz`. Extract that archive and follow `docs/BUILDS.md`. When
redistributing, keep the source and notices with the binary or provide equivalent
source access alongside its download and update the directions. A source repository
URL alone is not this delivery mechanism. Do not offer a binary after its matching
source download expires or is removed.

No Blizzard game-install files, extracted assets, original FrameXML, account
credentials, or populated local configuration are part of this distribution.
Players supply their own legally obtained game data separately. The player README
consolidates distribution license and attribution notices without removing the
original copies from the corresponding source. Retained upstream
copyright commentary is attribution, not a legal determination or permission to
redistribute third-party game content. This packaging process is not legal advice
or a guarantee of compliance with every applicable right or dependency obligation.

The terrain-only local producer is a bounded Rust adaptation of the pinned CMaNGOS
`ad -f 0` terrain/MCLQ writer. Its source scope and unchanged upstream notice are in
`crates/benilla-native-terrain/README.md` and `CMANGOS-NOTICE.txt`. It distributes
original authored fixtures, not client assets or generated real terrain.

The separate raw M2 and bounded WMO collision writers are bounded Rust adaptations of the pinned
CMaNGOS VMap extractor. Their scope and upstream notice are retained in
`crates/benilla-native-vmap/PROVENANCE.md` and `CMANGOS-NOTICE.txt`.
Their extractor/assembler regression inputs are authored numeric fixtures.

The bounded VMap acquisition and placement-discovery library adapts the pinned
CMaNGOS extractor's ordering and name handling. Its source scope, explicitly
versioned listfile termination compatibility rule, and upstream notice are in
`crates/benilla-native-vmap-source/PROVENANCE.md` and `CMANGOS-NOTICE.txt`.
Its test archives are authored fixtures, not client-install data.

The source-acquisition library also contains bounded G3D doodad projection under
the explicit portable `libm 0.2.16` numerical compatibility policy documented in
`crates/benilla-native-vmap-source/DOODAD-PROJECTION.md`. The unchanged G3D Modified
BSD grant is retained in `G3D-NOTICE.cpp.txt`; shared sine/cosine source and its
LGPL notices remain in `benilla-sim::source_trig`. No native-libm universal
bit-parity claim is made.

The test-only WMO reference-order profile derives list operations from GNU
libstdc++ 14's `bits/hashtable.h` (Free Software Foundation, GPL-3.0-or-later
with GCC Runtime Library Exception 3.1). Its unchanged notice and exception
are retained in `crates/benilla-native-vmap-source/GCC-RUNTIME-NOTICE.txt`.
The authored reference fixtures contain numeric keys and iteration orders,
with compiler/header/library hash provenance; they contain no installation data.
