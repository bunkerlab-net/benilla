# Source and content provenance

This crate's Rust implementation, JSON fixture and tests are original code for
this fork, licensed GPL-3.0-or-later. No CMaNGOS implementation function, database
row, Blizzard client file, quest text or model is copied into the crate. The root
`LICENSE-GPL-3.0` supplies the license text when integrated into this repository.

The reviewed upstream commit IDs identify the target for a future adapter, not a
claim that one exists. Schema/table facts were inspected from the official
CMaNGOS repositories; the adjacent import design links those exact references.
The JSON model intentionally differs from native SQL and is defined only by this
crate's versioned contract. Fixed damage, reward and direct ability primitives are
not assertions of upstream game rules.

`tests/fixtures/original-camp.json` is authored test content. Identifiers are
arbitrary fixture keys, display/faction IDs zero are placeholders, and all prose
and numeric values are synthetic. Never replace this fixture with local game
content or commit local exports for convenience.

Input provenance is not authenticated. A supplied `local_cmangos_classic` label,
commit ID or SHA-256 does not prove licensing, correct conversion or a compatible
installation. Classic-DB itself distinguishes GPL-covered material from Blizzard
copyrighted material. Preserve that distinction in any future data workflow.
