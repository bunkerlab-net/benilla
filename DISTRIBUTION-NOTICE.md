# Benilla fork: combined player distribution

This fork links `benilla-session` and `benilla-sim` into the player. The combined
program is distributed under GNU GPL version 3 or (at your option) any later
version. See `LICENSE-GPL-3.0`. There is no warranty, to the extent permitted by
applicable law.

Original Benilla files retain their MIT OR Apache-2.0 grants and copyright
notices; `LICENSE-MIT` and `LICENSE-APACHE` remain applicable to those files.
This notice does not replace or relicense individual third-party files. Dependency
licenses and notices accompany their source in the matching source archive
(see `SOURCE-DOWNLOAD.txt`, or the embedded `corresponding-source.tar.gz`).
The patched Kira and Lua source and notices are included there too.

The simulation contains a partial Rust translation of CMaNGOS Classic XP helpers,
modified by Benilla fork contributors in 2026, from commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. CMaNGOS granted GPL version 2 or later;
this combination uses GPL version 3 or later. Consult
`crates/benilla-sim/PROVENANCE.md` and the unchanged upstream authorship, copyright,
and license documents in `crates/benilla-sim/licenses/`. The synthetic encounter
harness is not a port of full Classic combat or a claim of full gameplay parity.

The matching build source, locked dependency sources, license texts, build scripts,
and build information are provided through the exact download and SHA-256 in
`SOURCE-DOWNLOAD.txt`, or through an embedded `corresponding-source.tar.gz` in a
self-contained package. Extract that archive and follow `docs/BUILDS.md`. When
redistributing, keep the source and notices with the binary or provide equivalent
source access alongside its download and update the directions. A source repository
URL alone is not this delivery mechanism. Do not offer a binary after its matching
source download expires or is removed.

No Blizzard game-install files, extracted assets, original FrameXML, account
credentials, or populated local configuration are part of this distribution.
Players supply their own legally obtained game data separately. Retained upstream
copyright commentary is attribution, not a legal determination or permission to
redistribute third-party game content. This packaging process is not legal advice
or a guarantee of compliance with every applicable right or dependency obligation.
