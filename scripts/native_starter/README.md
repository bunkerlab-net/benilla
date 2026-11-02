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
The Python CLI requires that manifest and verifies the completed v3 or v4
allowlist, hashes, lengths, and layouts before preparing anything. The production
Rust verifier requires v4 and all 21 tables, including Emotes. Manual or
unmanifested directories are not accepted by the CLI. The diagnostic Python
library can still inspect synthetic or incomplete tables, recording no
extraction claim.

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

The runtime's `native::diagnose_production_inputs` independently reconstructs this
complete report from source-v2, v4 DBC files, an explicit selected identity, and
explicit `NativeWorldRules`. It compares canonical reports, not just
`preparation_sha256`. Its diagnostic identity also binds the exact bytes and
length of `extraction.json`. It never constructs `NativeCapability` or a runtime
profile. See the [runtime verifier contract](../../crates/benilla-world-runtime/src/native/README.md).

The shared `production_verifier_cases.json`, `production_verifier_mutations.json`,
and `production_verifier_malformed.json` contain authored data only. Rust and
Python consume the same cases for complete preparation equality, stale bindings,
rehashed inconsistent artifacts, strict JSON, packed WDBC layouts, and numeric
limits. Preparation uses sorted presentation field names so JSON object key
order cannot change its identity. JSON decoding has a 128-level nesting bound
and a 4,000,000-value bound in addition to each input's byte limit.

The combined corpus retains both initial spell execution fixtures and creature
combat evidence fixtures. Every preparation is rebuilt from its authored source,
DBC bytes, extraction metadata, request, and rules after either plan changes.
Fixture JSON is compact to keep the combined corpus below the 8 MiB reader limit.

The corpus includes same-snapshot quest completion scripts, source items and
gossip lookup closure, exact SQL skill-training probes and profession repair,
and selected spawn-context exclusions. Complete-empty exclusions retain cold
loader-order and assistance obligations. Inbound linking rows remain blockers.

The verifier also reproduces the captured item plan and its carried-storage
dispositions. Supported item shapes replace only the per-item inventory-rule
finding. Starter eligibility, inventory audits, and newly looted equipment or use
remain separate obligations. The shared corpus covers signed spell sentinels,
class and race masks, bags, weapon stats and damage, precise item refusals, NULL
request-items text, outfit-only discovery roots, and cast-time selector zero.
A zero cast-time selector ignores even an existing DBC row with ID zero.
Rehashed report mutations cannot change any of these decisions.

## Creature combat evidence

Each creature has a `combat_evidence_plan`. The plan derives the pinned ordinary
AI selector branch, reaction flags, neutrality, melee timer inputs, and assistance
radius source. It does not construct an effective AI or replace current runtime
state with template values. The four combat, aggro, assistance, and cold-order
obligations retain their precise missing-context lists.

At the pinned revision, `AIRegistry::Initialize` does not register `AggressorAI`
or `ReactorAI`. Ordinary creatures with an empty `AIName` select `EventAI` through
its reactive or proactive permit score. `NullAI` has a lower permit score.
An explicit unknown name falls back to `NullCreatureAI`, not `EventAI`.
Script callbacks precede this ordinary branch. Pet, charm, totem, and guard
selection require their own source context. `NO_AGGRO_ON_SIGHT` makes the supported
ordinary AI Defensive. It does not make neutral creatures hostile or disable
retaliation and assistance traversal.

The existing `creature_ai::attack_distance` reducer owns the level calculation.
It clamps player level minus creature level to at least -25, subtracts that
difference from detection range, applies a five-yard minimum, and multiplies by
the explicit aggro rate. Zero detection or zero rate returns zero. The supported
ordinary player branch excludes both detection-range aura modifiers and pet
scaling. Faction-template reactions remain directional. Current reputation and
attackability checks remain separate from the radius.

The existing generation and mainhand owners consume `MeleeBaseAttackTime`, damage
school, generated damage, and per-life timers. A captured 2000 ms template timer
does not prove current attack readiness, target selection, facing, or combat
callbacks. `resolved_combat_stats` remains null until runtime composition.

`CallAssistance` and pull assistance use positive template `CallForHelp`, otherwise
the configured family radius. The current runtime admits only the positive
template branch. `CallForHelp` instead uses its argument, or template `CallForHelp`
when that argument is nonpositive. It visits grid objects synchronously. It does
not apply the family-radius fallback. Visible-peer `CheckForHelp` uses another
configured radius. The existing world-rules v1 schema supplies none of these
combat settings. The progression preset cannot supply them implicitly.

SQL can capture templates, positions, exclusions, and row order. SQL cannot
capture the loaded map's grid activation sequence, linked-list membership order,
current generations, pending events, or shared RNG interleaving. A canonical GUID
sort is not that evidence. `CreatureColdEntrySchedule` supports only a synthetic
player-then-authored-spawns schedule, so even an observed realm trace cannot yet
construct a production schedule.

Cold admission requires source and configuration identity, map and grid activation,
persistent spawn state, ordered grid/world membership, cold-entry and consecutive
update order, and the shared draw schedule. Assistance additionally requires each
request's exact current-generation inventory, complete hooks, reactions, range,
and LOS evidence. Empty linking/group SQL results do not prove an empty receiver
traversal. Nonempty delayed dispatch and direct `CallForHelp` remain unsupported.
See the [assistance contract](../../crates/benilla-world-runtime/COMBAT-ASSISTANCE.md)
and [pinned source citations](PROVENANCE.md#creature-combat-selection-and-cold-loading).

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
  outfit grants in order. A bounded `placement_plan` reports equip candidates,
  source guards, backpack storage, and the second inventory pass. It names
  `starter_init::CapturedStarterItems::compile` as its Rust consumer. The plan
  does not claim resolved equipment or effective skill/spell learning. The
  placement blocker remains if any selected row is missing or unsupported.
  Ambiguous outfit matches fail instead of picking an arbitrary row
- `captured_item_plan` binds every selected item to exact inventory and starter
  inputs. Supported carried rows discharge only their own coarse inventory/item
  finding. The output preserves five spell slots and their sentinels and reports
  separate starter-placement support. Class/race masks, level, proficiency, and
  other equip requirements never become pickup restrictions. Newly looted gear
  equipping and all item use remain unavailable. Independent audits, client
  metadata, and actual initializer/custody composition remain required. See the
  [captured item contract](../../crates/benilla-sim/INVENTORY.md#captured-item-admission).
- `player.initial_reputation` reports source race/class-selected initial bases,
  flags, reputation indices, and zero stored deltas when the complete Faction
  table and selected template/quest/item references resolve. Missing or invalid
  inputs retain the initialization blocker and precise reasons. The bounded
  [Rust adapter](../../crates/benilla-sim/REPUTATION.md#dbc-initialization-adapter)
  feeds the existing explicit catalog. Diplomacy affects awards, not initial
  standing. Reward policies and effective passive closure remain independent
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
  New source captures also bind [native spawn dependency coverage](../classic_export/NATIVE-SPAWN-CONTRACT.md)
  for default/explicit spawn templates, events, pools and GUID/template addons.
  Preparation preserves raw overrides and GUID-addon precedence. Complete-empty
  evidence is usable only for those named selectors; legacy missing evidence is
  null. Substitutions, groups and other hooks remain unresolved, and no AI profile
  is constructed
  Each creature also has a `generation_plan` for the bounded captured-template
  adapter. It names the inclusive level range, whether a level draw is required,
  current display probabilities, class-level rows, exact faction-template words,
  and numeric binary32 inputs. Only a complete supported plan replaces that
  entry's coarse stat/faction and variable-level findings. Script names,
  unsupported AI names, unknown flags/ranks, missing levels, and incomplete
  model/display/faction closure retain those findings. The remaining template
  lifecycle fields stay blocked. The plan does not choose a level or invent
  resolved stats, rates, or faction hostility. See the
  [adapter formulas and refusals](../../crates/benilla-sim/CREATURE-INIT.md#captured-template-generation-adapter).
- A bounded fixed point over default skills, all matching SkillLineAbility
  rows, their skill/spell/forward references, all three SkillStep/Skill/LearnSpell
  effects, items/reagents/totems, and captured SQL learning/rank links. These are
  conservative dependencies, not asserted learned skills
- Candidate quest paths using only captured quest IDs, giver/finisher/target
  spawn GUIDs and initial SQL spells with an actual damage/heal effect. They are
  explicitly `eligibility: "not_evaluated"`; a structural candidate does not
  prove quest eligibility, learned-active spell availability or target hostility

`required_additional_rows` is a sorted, exact-ID request set for discovered
missing DBC/item/spell inputs. It is not a complete playable-content closure:
`unsupported` also names unported resolution/learning/overlay/AI/lifecycle paths
which may discover further dependencies. Never fabricate those rows or modify a
v2 bundle to satisfy a request. Use the bounded recapture flow below to obtain
additional SQL rows in a new snapshot and preserve its provenance.

## Local dependency recapture

The compiler also emits `dependency_request` and
`player.skills.dependency_closure`. The latter retains every candidate row in
native DBC row-ID order and explicit dependency edges with all three effect
slots. Client and server role disagreements expand both dependency sets and
remain blocked. An effect's misc value is interpreted as a skill ID only for
SkillStep (44) and Skill (118); other raw effects and modifier masks retain their
original roles. SQL learning links and previous/first/required rank references
are followed in the same bounded graph; reverse forward-spell DBC links also
recover predecessor candidates. Rank-dependent passives remain entry-critical,
even when the higher-rank active cast is visible but unavailable. Learn-spell and previous-rank cycles
terminate discovery and become entry blockers.

A finite `fixed_point_reached` result does not prove that missing SQL rows contain
no further edges. `captured_inputs_complete` is limited to the named discovery
scope. It can become true with valid local inputs, independently of whether any
cloud environment possesses those inputs. It never grants an ability, proves
initial stats or produces a runtime export. Talent membership and external
initial hooks remain explicitly unproven; no missing relationship becomes an
invented false/empty value. SQL captured-empty relationships have explicit
selector coverage and empty arrays; legacy missing coverage has null arrays.
The separate one-row global SQL probe determines `derived_learn_spell_scan`:
`enabled_by_nonempty_table`, `skipped_for_empty_table`, or `not_captured`. This
reflects the native loader's early return when the whole table is empty. It is
not an already resolved learned-spell map; talent and native-loader conditions
still must be applied before initialization.

To obtain a standalone request, recompute it against the original local files:

```sh
python3 scripts/native-starter.py dependencies \
  --source benilla-config/classic-source-v2.json \
  --dbc-dir benilla-config/local-dbc --race 1 --class 8 --gender 0 \
  --output benilla-config/native-dependencies.json

python3 scripts/classic-export.py capture \
  --database YOUR_WORLD_DB --user YOUR_EXISTING_READER \
  --selection benilla-config/selection.json \
  --dependency-request benilla-config/native-dependencies.json \
  --output benilla-config/classic-source-v2-expanded.json
```

Use the existing SELECT-only terminal credential flow and, for MyISAM, the
explicit quiescent-snapshot option described in the
[capture instructions](../classic_export/README.md). Reprepare against the new
capture and the same extracted DBC directory. Repeat only if newly captured
rows reveal further exact dependencies; all requests retain earlier item/spell IDs, so
later captures cannot silently discard prior discovery. Every output must be a
new file. The request is a bounded ID list, never executable SQL or a gameplay
trust token. Its source/DBC hashes identify the discovery inputs; they are
lineage metadata, not proof that a later SQL snapshot has identical content.
The later preparation revalidates that snapshot and recomputes all dependencies.

Limits are 512 spell nodes, 512 skill nodes, 2,048 item nodes, 4,096 selected
ability rows, 16,384 distinct edges and 16,384 exact DBC references. Each node/ability row is visited once;
all DBC files retain their existing record/byte caps. SQL spell discovery also
terminates by its visited set within 512 nodes, rather than truncating a valid
chain after sixteen links. Overflow rejects the operation, never a partial
success. All existing entry-critical/passive/initialization blockers remain.

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

The [Rust consumer contract](CONSUMER-CONTRACT.md) specifies exact candidate
initializer/NPC/spell bindings, remaining admission gaps and diagnostic-only
fields for the next conditionally admitted native profile.

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
   final stat calculation, start/server settings, and reputation reward policies
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

Extraction manifest v3 requires 20 complete tables, including Talent.dbc and
CreatureDisplayInfo.dbc. Earlier v1/v2 extractions must be repeated with the
current helper into a new private output directory. The verifier reports
`extraction.legacy_manifest_reextract_v3_required`; editing the old manifest or
adding a coverage boolean is not migration. Re-run preparation against the same
verified source and newly extracted bytes. Old source-v2 files remain accepted
for diagnostics, but missing spell-registry evidence and missing gender-model
closure require a new bounded local capture for those facts.

Selected creature displays retain all 12 raw words, record and whole-file hashes,
model/extended-display IDs, scale bits and signed SizeClass. Only words 6–8 are
strings; word 9 accepts -1. All template alternatives, applicable spawn override
and recursive other-gender rows are bound. Missing table and known absent row
remain distinct; neither gets scale 1.0. Alternative-model semantics and renderer
relations are separate requirements. See the [consumer contract](CONSUMER-CONTRACT.md).

Selected spell/aura registry rows and empty results carry their exact source
[coverage proof](../classic_export/SPELL-REGISTRY-CONTRACT.md). Present unknown or
empty names stay explicit rows, never a no-hook default. New local world rules
can be explicit consumer policy; a new P2P world does not require an existing
server configuration. Importing legacy effective configuration is a distinct,
unimplemented evidence path.
