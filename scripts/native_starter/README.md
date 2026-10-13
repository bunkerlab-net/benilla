# Native starter preparation compiler

`native-starter.py` consumes a private validated
`benilla-classic-source-slice-v2` and the user's extracted, final-patch-chain
WDBC files. Its versioned capability is
`cmangos-8ec338a1-native-starter-bindings-v1`. It compiles native *inputs*, not an
executable world: the output is `benilla-native-starter-preparation-v1`, always
`status: "blocked"` with `runtime_export: null`. It has no activation switch,
trust flag, `WorldData` conversion, native mechanics token or inferred defaults.
The existing `Rules::validate` rejection is unchanged.

## Smallest concrete target

A selected Human Mage (race 1, class 8), level one is a candidate, not bundled
content. The source must actually select that combination and include its native
start, initial SQL spells, nearby spawns and quest relations. The compiler never
invents Fireball, Frost Armor, outfit IDs, a convenient heal, quest objective
counts or rewards. Other selected race/classes can be inspected, but none is
currently enabled for gameplay.

First extract the fixed DBC inputs from your normal WoW Data directory. The new
Rust helper reuses the application’s existing benilla-formats patch chain, checks
MPQ member sizes before decompression, and reads the installation without writing
to it. It has no separate locale/patch resolver. It refuses missing, deleted,
oversize or malformed winners instead of falling back to older archive copies.
It writes into a new private directory and publishes extraction.json last.
A crash may leave an incomplete directory without that manifest; retry to a new
output path, never assume those partial files constitute a completed extraction.
The CLI requires that manifest and verifies the exact 18-file allowlist, hashes,
lengths and layouts before preparing anything. Manual/external unmanifested
directories are not accepted by the CLI. The diagnostic Python library can still
inspect synthetic/incomplete tables, recording no extraction claim.

Then prepare from your private source and those exact extracted bytes:

```sh
cargo run -p benilla-native-dbc -- \
  --data "/path/to/your/WoW/Data" \
  --output benilla-config/local-dbc

python3 scripts/native-starter.py prepare \
  --source benilla-config/classic-source-v2.json \
  --dbc-dir benilla-config/local-dbc \
  --race 1 --class 8 --gender 0 \
  --output benilla-config/native-starter-preparation.json

python3 scripts/native-starter.py verify \
  --source benilla-config/classic-source-v2.json \
  --dbc-dir benilla-config/local-dbc \
  benilla-config/native-starter-preparation.json
```

The benilla-config parent must already exist. The extraction destination must
not exist; the preparation destination must be a new file. Never write into the
game installation. The separate helper checks WDBC envelopes; the Python
compiler additionally checks records, strings, references and source semantics. `prepare` exits **3** when
it publishes a valid blocked preparation; **2** means invalid inputs or a failed
operation. `verify` exits **0** for an exactly reproducible preparation, which
still does not establish gameplay support. Error messages omit imported text and
local path contents. No SQL, network, automatic uploads or asset downloads occur.
Outputs contain private game data: keep them under gitignored local state, never
commit or publish them. Publication uses a private 0600 temporary file, fsync and
atomic no-replace hard link; it cannot overwrite an earlier preparation or input.
The tool rejects output inside the DBC input directory.

## Compiled facts and exact references

- Explicit race/class/gender, native starting map/zone/decimal pose, selected
  display/faction IDs, DBC power type and class spell family. CharBaseInfo confirms
  the local race/class pair. Instance starts and unavailable races are blocked
- Each captured level's five base attributes, create health/mana and exact XP
  row. A missing cap-level XP row is `null`, never an invented threshold. These
  are inputs, not final player stats; equipment, passive and aura modifiers remain
  unresolved
- CharStartOutfit's actual packed selector and twelve ordered signed item slots.
  Negative/zero slots are skipped exactly as `Player::Create` does. Missing
  positive IDs become explicit `item_template` requests. Known item quantities
  use the bounded native BuyCount/food/drink branch; SQL custom grants follow
  outfit grants in order. Equipment placement and item-instance creation are not
  implemented. Ambiguous outfit matches fail instead of picking an arbitrary row
- All three effects and the entire selected server spell rows, matched to local
  Spell.dbc fields. Server/client differences are reported by field. Signed cast
  times and exact binary32 range words retain precision. Reagents, totems,
  family masks, max-target fields and damage-multiplier words omitted by the SQL
  capture are retained from the actual DBC. Duration/radius/focus references are
  looked up when present. `EffectItemType` is not assumed to be an item ID: it can
  be a modifier-aura family mask, so unresolved interpretation is blocked
- Actual source creature/spawn/model/class-level rows, without inventing final
  combat stats, faction hostility, AI, collision or pathing. New source captures
  record complete EventAI selector coverage for current entries and selected
  spawn GUIDs. Any matching row blocks; no flags/chance/phase filter can turn it
  into a false absence. Old captures without that evidence remain unproven.
  Empty coverage is narrow: substitutions, groups/addons and other hooks remain
  unresolved, and no AI profile is constructed
- Skill candidates, their race/class rows, tier references and linked spell
  candidates. These are conservative dependencies, not asserted learned skills
- Candidate quest paths using only captured quest IDs, giver/finisher/target
  spawn GUIDs and initial SQL spells with an actual damage/heal effect. They are
  explicitly `eligibility: "not_evaluated"`; a structural candidate does not
  prove quest eligibility, learned-active spell availability or target hostility

`required_additional_rows` is a sorted, exact-ID request set for discovered
missing DBC/item/spell inputs. It is not a complete playable-content closure:
`unsupported` also names unported resolution/learning/overlay/AI/lifecycle paths
which may discover further dependencies. Never fabricate those rows or modify a
v2 bundle to satisfy a request; a later bounded capture extension must obtain
additional SQL rows and preserve its provenance.

## Manifest and field-disposition contract

The canonical source hash binds the fully revalidated v2 bundle. Each successfully
parsed DBC has its whole-file SHA-256, size, layout and row count. A malformed but
fully readable DBC is also hash-bound in its error observation. Oversize files
fail before publication because a prefix hash is not a whole-file hash. Missing
or unreadable files are explicit observations; no bytes are claimed for them.
Each retained DBC row records its raw words, validated strings and record hash.
Map ID zero is valid; absent IDs never become zero/default rows.

WDBC contains **no build marker**. The format records `client_build_claim: 5875`
and `client_build_authenticity_verified: false`. Exact reviewed layouts reject
mismatches, but layouts/hashes prove neither build authenticity nor provenance,
legal rights, database completeness or gameplay correctness.

Every captured SQL row/field has exactly one preparation disposition:
`consumed_by_preparation`, `inert_source_annotations`, or a stable
`blocked_for_gameplay` reason. Consumed means the preparation uses/retains the
input; it does not assert runtime implementation. Every stored word/byte of each
selected DBC row similarly has exactly one disposition. Packed CharStartOutfit is
38 stored words (41 logical fields, 152 bytes); CharBaseInfo stores two bytes.
No unknown field, enum, target selector, extra effect slot or set flag bit is
silently declared harmless. Runtime blockers remain even for zero values.
Coarse source-capture missing-DBC findings are retained with a replacement
annotation; exact missing references and remaining native-rule requirements
replace their stale wording.

Unsupported initial active abilities are classified as visible but unavailable,
with exact IDs, dependency IDs and a mandatory cast rejection. They need not
individually veto future supported gameplay. Passive, DO_NOT_DISPLAY,
CAST_WHEN_LEARNED, learn-spell/skill-step effects, item/quest spell dependencies,
and unresolved learning roles remain entry-critical, including trigger closure.
Client/server disagreements in role-defining fields prevent active-only
classification. Effective learning is still unproven and no ability is enabled.

`preparation_sha256` covers every output field except itself. Verification
recompiles the entire output against the current source, DBC and exact completed
extraction metadata bytes; changing
a hash after deleting a blocker does not make it pass. This is deterministic
integrity checking, not a signature or an authenticity authority.

Bounds: 64 MiB per input DBC; 192 MiB read aggregate; 100,000 records/table;
16 KiB/string; 16 MiB distinct decoded text/table; 262,144 distinct offsets/table;
250,000 SQL fields; 32 MiB selected DBC bindings; 128 MiB final preparation.
Repeated string offsets share decoded strings. Only regular files are accepted;
POSIX NONBLOCK/NOFOLLOW plus opened-inode checks prevent pipe/device blocking and
symlink races. Strings remain inert data. No imported path is opened.
The helper's opt-in bounded MPQ path additionally caps encoded indices at 64 MiB
cumulatively across archives, encoded/decoded sectors at 64 KiB, header scanning
at 1 MiB/archive, mounted archives at 64, and directory entries at 4096. Checks
precede allocation/read. Only stored/exact-zlib sectors are supported; oversized
encoded sectors or excess inflation reject. Bounded archive opens reject
symlinks and nonregular files, including member reopens. Legacy archive/chain
callers retain their existing behavior and the helper uses identical precedence.

## Remaining native path and app boundary

The existing app controller must reject this output as unsupported. A future
`NativeWorldProfile` must construct opaque validated world/rules/catalog objects
only when every entry-critical dependency and every enabled ability has
executable, validated semantics. Unsupported active abilities may remain visible
but unavailable only when both admission and UI reject their use. This
preparation exposes no token that could satisfy that gate. The future profile
validator must run conditionally on the user's local inputs; lack of cloud-held
game data is a verification limitation, not a permanent admission rule.

The next ports depend on the actual report:

1. Player creation: effective skills/spells, passive and equipment resolution,
   final stat calculation, start/server settings and initial reputation
2. Spells: all actual effect slots, attribute words, target selectors, projectile
   travel, aura/periodic state, bonus/scaling, power, cooldown and interruption
   rules. Effect 2/6/10 labels indicate damage/aura/heal work, never support
3. Creature `SelectLevel` and exact native stat inputs, model/level RNG, selected
   AI branch, movement, melee rolls, retaliation, threat, loot and attribution
4. Quest full eligibility/credit/reward/XP lifecycle over actual native rewards
5. Exact-ID stock-UI binding, durable headless flow, then local graphical evidence

The independent Rust sim formula/quest/target primitives are not reimplemented or
called as if they establish these whole lifecycles. No source/DBC/game assets were
available for live testing here; synthetic tests establish parser/compiler
contracts only.

## Tests

```sh
cargo test -p benilla-native-dbc
cargo clippy -p benilla-native-dbc --all-targets -- -D warnings
python3 scripts/test-native-starter.py
python3 scripts/test-classic-export.py
python3 scripts/check-native-outfit-oracle.py # required Linux source regeneration check
```

Tests generate original data in memory, including packed layouts, native-zero
map keys, every field's disposition, deterministic hashes, metadata mismatch,
missing exact references, all effect slots, modifier-mask non-item semantics,
malformed files, memory caps, no-overwrite publication and unchanged inputs. A
committed 378-case original numeric fixture verifies native outfit quantities
on every platform without an optional compiler skip. The separate Linux source
check recompiles the standalone C++ oracle and requires an exact fixture match;
an unavailable compiler is an error. See [provenance](PROVENANCE.md).

Extraction tests invoke the actual Rust CLI against generated original MPQ
archives: patch-2 overrides patch/base, patch-10 is not mounted, tombstones and
invalid winners never fall back, and oversize headers fail before member reads.
Input archives stay byte-identical. The helper is tested on Linux; Windows/macOS
privacy and durability behavior has not been executed here. On non-Unix systems
permissions inherit from the selected parent, so choose a user-private parent.
