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

## Persistence contract

Native format 66 rejects older envelopes; no migration is provided. Existing
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
