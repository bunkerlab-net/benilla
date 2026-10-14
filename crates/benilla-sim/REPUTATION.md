# Source-native reputation primitives

`reputation` translates the reputation mechanics of CMaNGOS Classic commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. CMaNGOS Project contributors retain
upstream authorship, under GPL-2.0-or-later; the Rust translation and authored
harness are GPL-3.0-or-later. The notices and grants in `licenses/` apply. This
module contains no game-install assets or world-database rows.

## Input and replay contract

`Catalog` privately owns sorted faction rows, exact binary32 world rates, explicit
present/absent reward-rate and spillover results for every present faction, and
an `Audit`. Known absent faction IDs are listed separately. An unlisted faction
lookup returns `Unresolved`; missing data is never inferred to be absent. Every
nonzero spillover target must have a resolved lookup. `Faction` preserves all four
race masks, class masks, base values and flag values, plus reputation index and
team. Rewards preserve all five original slots, their flags, and signed quest level.

These are supplied typed, immutable inputs. Neither a caller's `Absent` assertion
nor a catalog hash proves a raw-content scan or grants admission to a native
content producer. The caller must supply the complete initialization set of
reputation-bearing faction rows and resolved reachable lookups. This module does
not acquire, inspect or certify raw databases. Catalog serialization rejects
unknown fields; `validate` must be called on decoded data before use. Every reducer
validates the catalog and state. Public read queries reject a mismatched catalog
hash. The canonical hash binds all supplied rows, explicit absence, rates and audit
fields to `POLICY_ID`. State binds that hash and its race/class masks, reputation
index map, stored deltas, flags, and send/save dirtiness. Caller-owned command and
quest inputs must also be bound into its replay/content identity.

The input domain is byte-sized nonzero player levels; target levels 0 through
`i32::MAX`; signed int32 rewards and bases; finite normal binary32 rates or signed
zero with absolute value at most 1,000,000; and reputation indices below 64
(negative indices mean no reputation). Reward and world rates must be nonnegative.
Spillover rates may be negative, as the pinned runtime multiplication allows.
NaNs, infinities, subnormals, arithmetic exceptions and signed conversion/operation
overflow fail instead of reproducing undefined C++. Intermediate binary32
operations use the existing pinned `rustc_apfloat` dependency, nearest-even rounding,
and final int32 conversions truncate toward zero. No host float math is used.

## Source mechanics preserved

- `FactionEntry::GetIndexFitTo` selects the first matching slot. A zero race or
  class mask is a wildcard, including the first slot. The client `FactionInfo`
  helper is deliberately not used
- `Initialize` walks ascending faction IDs, initializes stored Standing to zero,
  copies default flags without normalization, and marks send/save dirty. Later
  rows replace earlier rows sharing a reputation index, as the source map does
- Current reputation is base plus stored Standing; no-reputation and explicitly
  absent factions report zero and therefore Neutral. All eight rank boundaries,
  base rank, default flags and at-war queries follow the source
- `SetOneFactionReputation` performs incremental addition before the -42000/42999
  clamp, stores total minus base, marks dirty, makes visible, changes at-war flags
  on the two exact rank crossings, then invokes `ReputationChanged`
- The low-level at-war setter checks peace-forced eligibility against stored
  Standing, not total reputation. Downward hostile crossing uses this low-level
  setter; upward neutral crossing goes through the hidden/invisible guard.
  Clearing an already-clear at-war flag still marks dirty, as upstream does
- Four spillover slots execute sequentially before the primary update. Each rank
  condition observes all preceding changes. Duplicate and self targets remain
  separate operations. Spillover uses the original requested standing, truncates
  after binary32 multiplication, and does not recursively apply spillover
- `SetReputation` returns whether the primary rank increased, not whether anything
  changed. Its state-send event starts with the primary row and appends dirty
  other rows in reputation-index order, then clears their send flags
- Quest/kill/spell gain uses source-specific rates and source operation order.
  Kill and quest low-level thresholds are distinct. Target level zero skips the
  penalty. A present zero faction rate disables that reward; no row does not.
  Final multiplication is global rate, signed reward, percentage, then division
  by 100, with binary32 rounding after every operation
- Quest rewards skip zero faction or zero value slots, preserve duplicates and
  slot-specific no-spillover bits, and use current player level for nonpositive
  quest levels. A computed zero gain still performs the source reputation update

## Synchronous host integration

`reduce` and `reward_quest` are observation-only callback variants for cases where
`ReputationChanged` has no state-mutating quest callback. They emit ordered events
with full intermediate reputation snapshots. Their existence does not establish
that callbacks are absent in any imported content.

For active callbacks use `reduce_with_host` or `reward_quest_with_host`. `Host` owns
all mutable callback state and is cloned before execution. At each exact source
callback position the host receives the staged reputation state. It may run the
source quest callback reducer, including nested reputation transitions; nested
events returned by the host are inserted before subsequent spillover slots and the
primary update. A callback error discards both staged reputation and staged host.
Host implementations must be value-owned and perform no external I/O while staging;
cloning a shared mutable external handle is not a transaction. Validate and commit
reputation, host quest state, inventory, XP, event outputs and persistence together.

The hosted quest loop reads `Host::current_level` before each slot, preserving level
changes from nested callbacks. The outer `RewardQuest` ordering remains reward
items, reputation, clear quest-log slot, then XP/level-up and money. The generator
checks that ordering directly in the pinned source. This module does not implement
`Player::ReputationChanged` quest objectives, full quest eligibility, XP awarding,
script execution, creature-on-kill reward/team allocation, spell effects,
network packet serialization, database loading/saving, or a persistence protocol.
`need_send`/`need_save` and send-event payloads are explicit state observations.
Visible notifications assume a player who is not in the loading phase; a network
adapter must apply the source `PlayerLoading` suppression. `SendInitial` emits the
existing faction states; filling the missing entries in the 64-slot packet belongs
to the network adapter.

General reputation auras, faction-specific reputation auras, forced reactions and
unimplemented script-driven effects require explicit `Evidence::Absent` for this
version. `Present` and `Unknown` are errors, never silently treated as zero or no-op.
`no_aura_bonus` is retained as source command input; it is numerically inert only
because both aura dependencies have been explicitly excluded. Native aura and
forced-reaction support requires a new reviewed input/reducer policy, not fabricated
empty effects. These bounded exclusions are not a claim of full Classic gameplay.

## Independent verification

`tools/generate_reputation_fixture.py` hash-checks every source file before extracting
actual C++ declarations and method bodies into the authored harness. It compiles
with `-fno-fast-math -ffp-contract=off`, `FE_TONEAREST`, undefined-behavior and
float-conversion sanitizers. It never calls the Rust implementation.

- `reputation_gain.tsv`: 8,192 signed/zero, source, level and rate cases
- `reputation.tsv`: 2,048 initialization, absolute/incremental, flag, spillover and
  duplicate quest reward sequences, including every callback snapshot and send
- `reputation_host.tsv`: 256 synchronous reentrant callback sequences. An authored
  callback changes a spillover target's rank and player level; subsequent slots and
  rewards must observe both changes

Additional Rust tests cover wildcard ordering, unmatched slots, rank boundaries,
clamps, explicitly absent versus unresolved data, duplicate reputation indices,
serde/hash binding, fail-closed dependencies, checked overflow, pre-XP level use,
and atomic rollback of both host and reputation after callback failure. These are
original authored inputs and numeric outputs, not distributed game data.

Regenerate or verify against the pinned source checkout:

    python3 crates/benilla-sim/tools/generate_reputation_fixture.py /path/to/mangos-classic --check
    cargo test -p benilla-sim --test reputation --locked

### Reviewed source files and SHA-256

All paths below are relative to `src/game` at the pinned upstream commit.

- `Reputation/ReputationMgr.cpp`: `87094877ab19cfa6e3b297b6d68afeedf5e9f312efd84e09be0293879796f63b`
- `Reputation/ReputationMgr.h`: `76df8b8b91e791aff896c8a2fa5ca62395eb5764f0c40b056a987a5996f5f60f`
- `Server/DBCStructure.h`: `354013c7508d7891f19a1856cd7bbafda19f18388bc20671f5692efa57ebdf50`
- `Globals/SharedDefines.h`: `4832b8e0da45211143acae7c42173fea42732e2acdbf190692725e51b020bfe8`
- `Globals/ObjectMgr.h`: `27648698cb0ed86a6354cf737895ccd3752de1847f89ee70b49260954d903229`
- `Globals/ObjectMgr.cpp`: `dd27915ca363f81885d20783fc4f6c0392df20c4e3dfa031faa1a30df7a4b78b` (loader review only)
- `Entities/Player.cpp`: `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e`
- `Entities/Player.h`: `acd3ff726fafecd582673ec918157584d71fde6e3a1f59e26f0374b379cdffbb`
- `Tools/Formulas.h`: `ca8b55fd022b0c0191255bc457574ccafce5eeb2f99a447466ba75be4717652c`

The authored-input native quest runtime supplies the synchronous host, persistence
reconstruction and client-event adapter documented in
[the runtime integration contract](../benilla-world-runtime/src/native/REPUTATION.md).
This does not expand the primitive's excluded mechanics or grant content admission.
