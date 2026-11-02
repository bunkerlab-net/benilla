# Selected player mainhand authority

The separate `PlayerMeleePolicy` selects the CMaNGOS Classic
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f` player mainhand path. Relevant source is
`CombatHandler.cpp::HandleAttackSwingOpcode/HandleAttackStopOpcode`,
`Unit.cpp::Attack/AttackStop/UpdateMeleeAttackingState/CalculateMeleeDamage/DealMeleeDamage`,
`UnitAuraProcHandler.cpp::ProcSkillsAndReactives`, and
`Player.cpp::UpdateCombatSkills/UpdateWeaponSkill`.

Selection is not native admission. The synthetic host tests use authored rows;
no content compiler, assets, native capability, server connection or spatial
admission is supplied by this policy.

## Executed domain

The selected mage uses actual retained equipment, durability, skill, derived
mainhand damage/critical/hit fields, and source attack intervals. Ordinary target
level, armor, strength, flags, generation, movement, facing and relationship
operands come from retained initialization and current owned state. Caller-supplied
eligibility or damage values are not accepted. Fishing poles, offhands, attack
speed modifiers, unsupported control/proc closures and tracked-faction histories
are refused. The fishing restriction also excludes an initially broken fishing
pole; its source refresh-only skill callback is not modeled here.

Actual evade damage is refused before its RNG; earlier range/facing ready-check
failures retain their original precedence, including while the target returns home.

Ordinary source-owned generic player casts may coexist with selected player
melee. The active generic slot in Casting state suppresses swing attempts, not
attack countdown, victim ownership or the retained swing error. Traveling and
finished slots do not suppress attempts. Successful source-selected releases
reset BASE before the same Unit update decrements its timer; cancellation does
not invent a release reset. Stopping melee does not silently cancel a cast, and
casting does not silently stop melee.

Fireball periodic damage holders share the target's owned health with mainhand
damage. Spell target death stops the attacker lifecycle without a melee timer
reset or interruption of another active cast. NPC casts, channels/autorepeat and
player slow/control effects remain excluded from this selected overlap domain.
These component transitions do not open full playable native admission.

Queued start/stop commands use current target generations. Swings run in the
player update phase, not on a controller clock. Source 3D range and 120-degree
scheduler facing are distinct from the mutual front arcs used by the outcome
table. In the pinned source, NOT_IN_RANGE has value zero: it does not enter the
nonzero-error retry block. Other failed ready checks write both BASE and OFF to
100 ms even without an offhand. Client Stop has no forced CancelCombat event.
Lethal target cleanup stops the attacker before the subsequent BASE reset.

Actual fresh damage/outcome/optional-glance draws precede parry haste and damage.
Positive damage owns tap, threat, death rewards and loot; surviving positive
hits own durability callbacks. Attacker weapon skill callbacks run after damage,
including lethal and glancing outcomes. Victim defense reactives are refreshed
only by the source class/outcome/alive conditions and expire in NPC Unit update.

## Captured starting staff

`Profile::initialize_captured_player_melee(starter, weapon_skill_gain)` consumes the
existing `StarterInput` after the captured learning and outfit adapters populate it.
Run the ordinary player and quest-progression initializers first. The adapter binds
the complete starter content identity to retained combat genesis, requires Human
Mage identity, equipped item 35, staff subclass 10, two-hand inventory type 17,
staff skill 136 and unarmed fallback skill 162. Both SkillLine rows and initialized
skills must exist. All five damage rows remain checked. Only one physical line
and a positive delay of at most 1,000,000 milliseconds are supported.

The return value is weapon-only mean DPS, calculated with software binary32 as
`((minimum + maximum) * 0.5 * 1000) / delay_ms`. DPS is derived, not an additional
`item_template` column or an independent authority operand. The adapter never
stores a level-one damage range. Existing progression callbacks derive live Mage
AP from strength minus 10 and calculate each damage bound as weapon damage plus
`AP / 14 * attack_seconds`. The existing executor rolls damage before outcome,
then consumes a glancing draw only when required. Targets at levels one and two
cannot receive glancing blows at this pin.

FactionTemplate 25 remains neutral, not automatically hostile. Its captured
FactionTemplate and Faction metadata must already resolve through the interaction
and creature-generation catalogs. Existing source predicates deny friendly targets
and refuse unknown relations or unsupported tracked-faction history. A permitted
neutral target can retaliate through the existing melee callback, including a
non-immune miss. The adapter supplies no forced hostility, retaliation flag,
source-closure assertion, or new combat executor.

This adapter constructs the unchanged `PlayerMeleePolicy`. Offhand, ranged,
unsupported weapon types, missing skills, changed starter inputs, incomplete
source dependencies and unsupported modifier/hook domains still refuse atomically.
No native format, replay policy, production entry, or `NativeCapability` changes.
The compound `creature.ai_selection_and_native_melee_required` preparation
obligation remains for independent AI, creature melee and source context.

Authored tests pass captured learning/outfit projections through the real
initializer and existing runtime. They cover level-one and level-two damage
against authored vermin armor, deterministic outcome boundaries, the low-level
glancing exclusion, faction-25 retaliation and friendly denial. The controller
test saves and reopens mid-countdown through `NativeStore` and compares the later
event transcript, RNG cursor and state with uninterrupted execution. These
authored rows use semantic keys, not copied Classic DB or DBC content.

### Pinned source correspondence

Upstream CMaNGOS Project contributors, GPL-2.0-or-later, revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The Rust translation and authored
adapter/tests are GPL-3.0-or-later. The adapter reuses these existing ports:

- [Player::UpdateAttackPowerAndDamage, CalculateMinMaxDamage and UpdateDamagePhysical, StatSystem.cpp:203-386](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/StatSystem.cpp#L203-L386).
- [Player::_ApplyItemBonuses weapon damage and delay, Player.cpp:7118-7173](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp#L7118-L7173).
- [Unit::UpdateMeleeAttackingState, Unit.cpp:650-729](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L650-L729), [resetAttackTimer:782-785](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L782-L785) and [GetAPMultiplier:10525-10528](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L10525-L10528).
- [Unit::CalculateMeleeDamage, Unit.cpp:1870-2156](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L1870-L2156), [RollMeleeOutcomeAgainst:2709-2759](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L2709-L2759), [CanGlanceInCombat:3133-3136](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L3133-L3136) and [CalcArmorReducedDamage:2337-2361](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L2337-L2361).
- [Unit::CanAttack and CanAttackNow, Relations.cpp:351-447](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Relations.cpp#L351-L447) and [CanAttackInCombat:1172-1196](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Relations.cpp#L1172-L1196).
- [Unit::DealMeleeDamage retaliation, Unit.cpp:2212-2224](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp#L2212-L2224) and [UnitAI::AttackedBy, UnitAI.cpp:137-140](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/AI/BaseAI/UnitAI.cpp#L137-L140).

## Persistence contract

Native format `NATIVE_FORMAT_VERSION` rejects older envelopes; no migration is provided. Existing
unrelated callback hash domains retain their prior versions.

The bounded player proof retains the latest writers of lifecycle, BASE, OFF and
last swing error, with a shared action frontier and exact surviving paired
writes. Captured source contexts are revalidated against retained historical
progression/generation prefixes. The proof validates reachable contexts, not the
authenticity of discarded action history or historical movement trajectories.
Retained generic-slot witnesses likewise prove a source-reachable cast context,
not an authenticated historical slot-selection trajectory.
Actual RNG purposes, ranges, contiguous counters and source callback transitions
are checked. Initial state, command sequencing, source spell resets, lethal stop
and reset order remain distinct causes.

Player death clears lifecycle immediately. A private nonserialized frame debt
waits only for a temporarily executing NPC owner to return before replaying the
proof. Repeated source no-op CombatStop calls do not replace that debt. No
checkpoint or accepted frame may retain an unfinished debt.

NPC parry haste uses the existing source-bound bounded AI timer restore contract;
the player proof does not authenticate that timer's historical last writer.
NPC aura-state wire field125 remains omitted as in the existing client projection.
A correct full field also needs the source low-health bit sampled before creature
regeneration; the defense bit alone is not projected as a fabricated full mask.
Attack, damage, critical-wound emote and stop events are separate actual events.

NPC defense reactive causes are cross-bound to the player action frontier and
any still-retained matching swing. Older pruned origins retain the explicit
reachable-context limitation.

Bounded proof storage does not make all progression history unbounded. Existing
derived callback journal and aggregate replay-work/byte limits remain fail-closed
and transactional. Unsupported operations or exhausted limits are errors, never
successful no-op swings or fabricated defaults.

Native format `NATIVE_FORMAT_VERSION` retains exact selected ordinary-instance destruction even when
a registered original with the same entry remains carried. The selected receipt
must preserve every registered instance; unselected quest consumption and
registered-item destruction remain unsupported. No old-save migration or native
admission is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) requires the optional shared
spline-ID owner. Its bounded source-thread domain includes actual NPC allocation
callbacks and the first-life player forced stop; profiles outside that domain
retain the older partial motion lane. Compact endpoints prove reachable
interleaving, while TickRecord replay establishes callback order. No old-save
migration, ghost/recovery capability, or native admission is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) retains an optional immutable
selected-character appearance owner from exact loaded source-row choices.
`PLAYER_BYTES` projects only its four owned cosmetic bytes; facial hair remains
retained without inventing the bank/rest siblings of `PLAYER_BYTES_2`. No old-save
migration, native admission, corpse creation, or ghost release is provided.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) retains source hide-helm/hide-cloak
XOR commands under the combined initial-control, spell and fresh-rest owner
contract. The full player-flags descriptor is not inferred for other profiles.
No old-save migration, native admission, guild state, or ghost release is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional exact loaded Map.dbc classification
to the native profile map identity. Classification is not geometry, admission,
or evidence that a corpse was created. No old-save migration or ghost release
is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) supports selected detached-original armor destruction
with retained tombstone custody; registered identities remain reserved. Generic
and indirect consumption remain unsupported. No old-save migration, native
admission, or ghost release is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded original mainhand membership
to source-ordered equipment observations and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded original mainhand AutoEquip for singleton types 17/21
to existing source-ordered membership and retained weapon fields. This does
not admit generic weapons, in-combat switching, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded carried-original-mainhand relocation and AutoStore
to existing exact custody receipts without equipment or stat callbacks. This does
not admit generic consumers, mainhand destruction, or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds selected destruction of detached pristine original mainhand
to exact quest callbacks and permanent identity tombstones without stat callbacks.
This does not admit generic consumption or held private consumers.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional explicit host-selected health regeneration
to validated standing-mage owners and the shared source regeneration cadence.
No rate is inferred when unset; no external server configuration is claimed.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded player sheath controls
to qualified fresh-player and no-enchantment owners, with field 164 composition.
Unsupported profiles do not gain source-backed sheath state.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds bounded occupied-backpack exchange
to exact both-endpoint original custody and existing source swap receipts.
No equipment replacement or generic consumption admission is added.
No old-save migration or native admission is added.

Native format `NATIVE_FORMAT_VERSION` (`NATIVE_POLICY`) binds optional source-qualified
static creature identity and committed-pose cell observations. These are not source
CurrentCell, loaded-grid, registration, lookup or selection authority.
No old-save migration or native admission is added.
