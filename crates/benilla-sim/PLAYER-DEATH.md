# Bounded solo-mage player death

SPDX-License-Identifier: GPL-3.0-or-later

`player_death` translates named CMaNGOS Classic death/control-flow mechanisms at
commit `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. The CMaNGOS Project contributors'
original source is GPL-2.0-or-later; this Rust translation is GPL-3.0-or-later.
There are no bundled Blizzard rows, database records, or game assets.

This is an engine-independent **ordered action plan and checked lifecycle**, not
a claim that a realm, ghost, graveyard, or client protocol has been implemented.
The caller must stage the plan's actions and commit the enclosing world state
atomically. Dropping the actions and committing only zero HP is not this port.

## Source boundaries

Pinned upstream paths and line ranges:

- [Unit.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Unit.cpp):
  `DealDamage` lethal branch 1009–1012; `Kill` 1019–1202;
  `SetCurrentCastedSpell` 4217–4300; `InterruptSpell` 4302–4330;
  `InterruptNonMeleeSpells` 4384–4397; `RemoveAllAurasOnDeath` 5457–5473;
  `AttackStop` 6339–6363; `CombatStop` 6365–6387;
  `ModifyAuraState` 6431–6480; `ClearInCombat` 8034–8042;
  `SetDeathState` 8534–8579; `DeleteThreatList` 8665–8669;
  `SetHealth` 9224–9245; `ClearAllReactives` 10310–10321
- [Player.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp):
  reclaim constants 136–139 and helper 1361–1375; `Update` 1523–1524,
  1571–1581; `SetDeathState` 1652–1699; `KillPlayer` 4418–4437;
  `GetCorpseReclaimDelay` and `UpdateCorpseReclaimDelay` 18990–19019
- [Spell.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.h):
  source spell states 206–216 and `CanBeInterrupted` 645
- [Spell.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/Spell.cpp):
  `cancel` 3101–3158, including GCD cancellation and dependent cleanup
- [SpellAuras.cpp](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellAuras.cpp):
  holder flags 5176–5177;
  [SpellMgr.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellMgr.h):
  `IsDeathPersistentSpell` 447–450;
  [SpellDefines.h](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Spells/SpellDefines.h):
  passive 0x40, no-durability-loss 0x20, allow-aura-while-dead 0x00100000

The generator checks the complete SHA-256 of every source input before extraction.
It does not accept another revision with coincidentally similar function names.

## Admission and input identity

The victim must be an alive solo mage (`class == 8`) with positive pre-hit HP and
lethal incoming damage. The caller supplies its stable player identity, ordinary
NPC killer identity, tick, and wall-clock seconds separately. A rejected
nonlethal request must use the normal damage path; a rejected dependency must
not silently become a zero-HP death.

`DeathSupport::checked` requires explicit known-absent evidence for every member
of `REQUIRED_ABSENCES`. Missing/unknown evidence and present unsupported behavior
are different errors. This covers battleground/duel/death prevention, killer
owner/responsible player, proc/AI/script hooks, controlled units, pet/guardian,
charm/totem/minipet, formation, instance death hooks, self-resurrection, group,
flying/falling movement, aura-removal hooks, player-AI combat hooks, active melee
spells, and spell-cancellation dependencies. The constructor verifies the
submitted facts, not the completeness or truth of a source capture. A compiler
must prove these facts rather than fill them from blanket defaults.

The last category includes used SpellMod charges, channel target-aura cleanup,
spell-created dynamic/game objects or creatures, and callback side effects not
implemented by the host's cancellation adapter. A callback cannot be called
complete by merely logging its name. Native integration may admit a stricter
subset, such as only already-supported generic mage projectiles.

The captured map `Instanceable` bit and world PvE reclaim-delay config are
explicit `DeathConfig` inputs. They must be included in the host authority/profile
binding. No default value is inferred from a map ID. The source-local u64 actor tokens are not truncated owner/character IDs: native
integration must bind them to the full owner/character and spawn/generation
identities in the enclosing authority state. The player snapshot restores
only against the same local identity and config; the host must additionally validate
its source profile, action-chain/checkpoint binding, zero dead-player HP, combat
cleanup and retained cast/aura state. Deserializing the DTO is not authority to
instantiate live state.

## Transaction order and integration obligations

`prepare_kill` returns a `DeathPlan` without modifying its input state. The ordered
`DeathAction` sequence preserves the admitted calls from `Kill`, player
`SetDeathState(JUST_DIED)`, and unit `SetDeathState(JUST_DIED)`:

1. Kill proc boundary (proved to have no handlers), HP zero, killer AI notification
   boundary, owner-pet notification boundary, PvP-death false
2. If the caller permits durability loss and the killing spell does not have
   `NO_DURABILITY_LOSS`, NPC 10% equipment loss and the durability-death message
3. Drunkenness, combo points and resurrection-request cleanup; pet/guardian calls
4. Death-removable aura holders, charm/minipet/totem calls, stop/clear/idle motion,
   stop evade, clear health-below-20 aura state, all six reactive timers and the
   mage defense aura state, diminishing returns
5. Combat stop, other units' combo-point references, non-melee interruption,
   state `JUST_DIED`, and the final `Kill` combat stop

`NpcDeathDurabilityLoss` is a callback boundary, not another implementation of
item loss. The runtime must call its existing
`EquipmentState::apply_npc_death_loss`, applying every reported `Broken` item
through its validated `StatRecipe::set_item_broken` callback **before each broken
item's durability commit**, in source equipment-slot/modifier order. Stage all
of that callback's resource/attribute/resistance/weapon/regen/crit changes and
journal them with the death. Its live health input is already zero. Mana is not
reset by death; item-break recomputation can change maxima and clamp live mana.
`mana_before_equipment_callback` is a provenance value, never an instruction to
overwrite the callback's resulting mana.

`CombatStop` denotes the pinned whole callback: `AttackStop(true,false,true)`,
remove attackers, delete outgoing threat and incoming hostile references, stop
combat timer and clear in-combat/pet-combat/PvP-combat flags. It does not clear a
player's selected target. The host must remove both sides of attacker/reference
relations and let ordinary surviving NPC AI continue its normal subsequent
update; it must not delete NPCs, pause the world, or invent an evade/respawn.
Callbacks for absent state are source call boundaries, not evidence that pets,
proc hooks, scripts or motion modes were supported.

The host may commit `JUST_DIED` durably at the lethal event. `prepare_update`
performs `KillPlayer` when the source player update runs: root movement, perform
`SetDeathState(CORPSE)` cleanup, clear dynamic flags, set the release-timer byte
according to `Instanceable`, initialize the six-minute timer, update reclaim
expiration when configured, update visibility, **then decrement the new timer by
the same update's `diff_ms`**. It must not initialize the timer after that update
or immediately at an unrelated lethal-damage stage. Pass surviving current spell
slots again because `SetDeathState(CORPSE)` repeats interruption. An out-of-world
update changes nothing. Duplicate update ticks and regressing clocks reject.

### Native fixed-step restore

`restore_native_fixed_step` adds the native first-life/no-resurrection policy to
generic snapshot validation. `NativeDeathClock` supplies clock origin, positive
fixed tick duration and current tick. Tick zero is genesis. Every later tick runs
player update before NPC melee, so a lethal NPC hit remains `JUST_DIED` for that
frame and `KillPlayer` executes exactly at `died_at_tick + 1`. This host ordering
is explicit policy; the generic CMaNGOS methods do not prescribe a map update
scheduler. The restore seam rejects a late phase, another corpse tick, inconsistent
last-update/wall clocks, or a timer that was merely range-valid.

It derives expiration from the origin-based wall second at the corpse tick and
source reclaim config. The snapshot retains expiration entering death; the native
first-life seam requires it to be zero. Outdoors, timer elapsed includes the
first corpse update: `(current_tick - corpse_at_tick + 1) * tick_ms`. The timer
and release-due latch must exactly match that derivation. Instances retain the
full timer. No offline catch-up is introduced.

This rejects a re-signed DTO with an arbitrary timer or delayed corpse transition,
but cannot independently prove that a claimed historical lethal hit happened.
The runtime's event/action binding and retained records still establish the death
tick and full identities; a plain checksum is not an authentication mechanism.

## Aura retention and spell lifetime

Aura input carries holder identity and the actual authoritative `SpellEntry`
`Attributes`, `AttributesEx3`, and `CasterAuraState`, not a guessed `passive` flag.
Holders use source multimap order (spell ID followed by stable insertion order
within that ID); duplicate identities and unsorted spell IDs reject.

`RemoveAllAurasOnDeath` removes a holder only if it is neither passive nor
death-persistent. Its stale comment mentions disabling passive modifiers; the
actual body does not do that. Surviving ordinary unconditional passive modifiers
remain active. Active death-persistent holders likewise survive this callback.
However `ModifyAuraState` can remove conditional holders later when the
health-below-20/defense flags clear. Nonzero `CasterAuraState` is therefore an
explicit rejection in this bounded port; it is not falsely preserved or silently
discarded. Applying aura-removal modifiers and their effects is the host's
ordered callback responsibility, with unsupported handlers rejected in advance.

Current slots are Generic, AutoRepeat, then Channeled. Source
`InterruptNonMeleeSpells(false)` ignores the triggered and
`ALLOW_CAST_WHILE_CASTING` traits:

- CREATED/TARGETING/CASTING/DELAYED current generic casts are interrupted even
  when triggered or allowed while casting
- CHANNELING is interruptible; every slot still respects `CanBeInterrupted`
- TRAVELING generic/auto slots are skipped. LANDING/FINISHED are not interruptible
- Channeled slots use `withDelayed=true`, but a TRAVELING state still fails
  `CanBeInterrupted`, so it is not canceled
- AutoRepeat sends its cancel notification when the interruption is attempted,
  before testing `CanBeInterrupted`, including LANDING/FINISHED
- Detached released flights are not members of the current slots and are retained

The host must invoke its source-derived `Spell::cancel` adapter for interrupted
casts, including preparing-cast GCD reset; removing an ID alone is insufficient.
The supplied traits document actual source metadata, not a survival exemption.
Nothing here refunds spent mana or cancels launched projectiles. The module does
not decide whether a surviving projectile later impacts a dead caster's target;
that remains the existing spell execution pipeline's responsibility.

## Timers, release, and deliberate stopping boundary

`PlayerDeathState` has private live fields and a versioned checked snapshot.
Alive, JustDied and Corpse are mutually validated phases. The six-minute source
death timer and the reclaim-expiration wall clock are separate counters.
`UpdateCorpseReclaimDelay` uses 300-second steps capped at three; reclaim delays
are 30/60/120 seconds from the source helper. At exactly the first-death update
second, the expiration delta is 300 and the helper returns 60; one second later
it returns 30. These exact boundary results are covered by the source oracle,
not smoothed to a guessed constant. A returned reclaim delay does not establish
that a corpse exists or can already be reclaimed.

`CORPSE` is the player body's state, **not a created corpse object**. Source
`KillPlayer` deliberately waits to create a corpse because the body might be
falling. `BuildPlayerRepop` includes ghost/wisp spells, corpse creation and
placement, health 1, unrooting and reclaim delay; `RepopAtGraveyard` needs captured
graveyard/terrain/teleport data. Those and resurrection/reclaim validation are
outside this port.

A corpse body's manual release request is reported as
`EligibleButUnavailable`, not accepted as a ghost transition. In outdoor maps,
reaching zero on the timer emits `ReleaseUnavailable { automatic: true }` and
latches `release_due`. This records the exact source boundary at which repop
would run; it does not claim to have executed it. Subsequent world updates
continue normally and do not repeat the unavailable event. Instance maps retain
the 360000-ms timer and have no automatic countdown. Neither case rolls back a
committed death or stops NPC/world time. Reaching the unimplemented boundary must
be visible to the native UI and persistence/diagnostics.

## Verification

`tools/generate_player_death_fixture.py <pinned-source-directory>` hash-verifies
eight upstream files, extracts unchanged `Kill`, both `SetDeathState` bodies,
aura removal, interruption, `KillPlayer`, reclaim-delay functions and the two
ordered player-update death blocks. It compiles the synthetic C++ harness with
warnings denied and UBSan. Unsupported branch adapters assert if reached.

`tests/fixtures/player_death_cmangos.tsv` contains 692 synthetic rows: 192 kill
orders, 192 corresponding corpse updates, and 308 timer/reclaim combinations.
They cover every source spell state, four triggered/cast-while-casting trait
combinations, durability and killing-spell exemptions, all passive/death-
persistent combinations, instance/config settings, repeated-death expiration
boundaries and zero/exact/overlong/u32-max deltas. Tests compare full callback
order and numeric state. The C++ harness stops at repop call boundaries; the Rust
expectation explicitly substitutes the pending-unavailable action there.

This is differential evidence for the named bodies and call boundaries, not a
full-server execution. Combat/reference mutation, equipment modifier application,
client packets/visuals, spell cleanup and native-world persistence require their
own host integration tests. The additional Rust tests cover strict evidence,
invalid metadata/identity/lifecycles, deferred corpse creation, timer expiry
without world freeze, passive preservation, launched-flight survival, and
preparing-cast cancellation.

A separate [corpse-reclaim call-plan component](CORPSE-RECLAIM.md) now models
source request guards and delegated callback order. It does not change this
body-only lifecycle, create a ghost/corpse, or enable native recovery.
