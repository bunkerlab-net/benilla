# Audited mage ancillary fields

GPL-3.0-or-later source translation from CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`. Policy identity:
`cmangos-8ec338a1-static-mage-ancillary-softfloat-v1`.

`player_progression::derived` implements the mandatory ancillary stat hooks. It
is not a substitute for the progression host's learning, catalog, and live-state
audits. No client-install or database rows are bundled. The source's spell-crit
coefficients are a pinned implementation, not independently verified retail
Classic coefficients.

The companion `LiveTraining` adapter rejects every recursive skill setter,
including one that preserves value, maximum, and step. Native `SetSkill` still
marks a non-new skill changed and executes the defense callback for skill 95.
The initializer's learning state does not propagate those live status/callback
effects, so accepting a numerically unchanged setter would silently omit them.
Such learning and its replay fail atomically until those effects are supported.

## Domain and actual inputs

Only static, solo, unshapeshifted mages at levels 1–60 are admitted. The caller
supplies callback-time effective attributes, level, and complete skill fields
(including status and permanent/temporary bonuses) through `Snapshot`. It also
supplies the actual equipped weapon rows through `DerivedInputs`. The weapon
list is complete, excludes armor, and contains at most one mainhand. All five
native damage rows must be present. Exactly one nonzero physical damage line is
supported; missing rows, additional lines, elemental damage, offhand/ranged
weapons, nonfinite/subnormal values, and unproven audits fail closed.

Every ancillary audit field must explicitly be `Absent`. The AP/damage audit
includes weapon haste, attack-time and base-DPS multipliers. The other audits
exclude additional crit, avoidance, spell damage, hit, regeneration, enchantment,
ammo, disarm, parry, block, dual-wield, and shapeshift behavior. Static primary
attribute/resource/resistance modifiers already admitted by `StatRecipe` remain
supported. Neither zero nor a missing audit means that a scan was performed.

The source's empty hit-aura and spell-damage-aura lists produce real zero field
writes. These hooks are executed; they are not silently omitted. Offhand physical
damage and internal offhand crit retain their source-initialized zero in this
no-dual-wield domain. All three crit base-modifier groups are still written.
Negative mage base AP is rejected before the source's undefined negative
float-to-unsigned conversion. Missing/deleted current skills return zero because
that is the explicit native `GetSkill` behavior, not a guessed database default.

Arithmetic uses software IEEE-754 binary32 in source operation order, reusing
the existing mage regeneration/spell-crit and NPC mage-dodge kernels. Native
constants 1–2 default damage and 2000 ms attack time are taken from `Unit.h`;
weapon rows themselves are never defaulted.

## Callback and item-break order

`DerivedStats::apply_hook` changes only the requested native hook's fields and
is atomic on failure. `SkillDefenseBonuses` maps to `StatHook::DefenseBonuses`.
It is valid for a callback to see a new level and old effective agility before
`UpdateAllStats`; the host must pass that actual intermediate state.

`DerivedStats::from_snapshot` performs a complete `UpdateAllStats` refresh. It
must not be used to silently normalize every checkpoint, defense gain, or item
break. Cached crit display fields can intentionally reflect the previous weapon
skill until a native crit callback runs.

For a break, the host first stages the real prebreak recipe operation
`StatRecipe::set_item_broken(slot, true, live)`. The returned `EquipmentRecompute`
contains resource steps and `attribute_steps`, with a Stats index and effective
attributes after every nonzero item attribute modifier. Repeated attributes
remain separate and ordered. The host then calls `apply_item_break` with the
prebreak progression snapshot, old and proposed-new weapon inputs, changed slot,
and the recipe result. For each attribute callback the adapter executes:

- Agility: all crit percentages, then dodge only
- Intellect: all spell crit chances
- Every attribute: melee AP/damage, ranged AP/damage, spell damage bonus, mana regen

The non-ancillary armor/resource callbacks remain in the existing recipe reducer.
All attribute callbacks use the prebreak equipped weapon and its old damage
rows. After them, a broken weapon's applied damage rows are cleared and its
attack time resets to 2000 ms; only physical mainhand damage is refreshed. It
uses the existing AP field instead of invoking another AP callback. If the
actual item row's Delay is zero, native `_ApplyItemBonuses` skips that final
physical-damage refresh, so the cached range remains unchanged until another
appropriate hook. Durability is committed by the caller after these callbacks.

The adapter is deliberately break-only. Repair commits positive durability
before reapplying modifiers and has a different intermediate weapon state.
Equip, unequip, repair, disarm, external item effects, and a general native login
of an already-broken item are not supplied by this API. Broken damage state here
is obtained by replaying a break from a fresh authorized recipe; it is not
inferred to be the same as a new upstream Unit constructor loading a broken item.

## Persistence contract

`DerivedInputs` and `DerivedStats` have strict serde DTOs, but deserialization
alone grants no authority. A host must replay its ordered, bounded callback
history from fixed genesis, validate every callback-time catalog/skill/modifier
and equipment identity, and compare the resulting fields. In particular an
`EquipmentRecompute` stored as a break witness must be reproduced by the
corresponding authoritative prebreak recipe and compared in full. Merely checking
that supplied `attribute_steps` have plausible shapes cannot authorize them.
History overflow or a missing source witness must reject the operation rather
than dropping old callbacks or replacing replay with a full refresh.

The containing persistence envelope must bind policy/schema and source/profile
identity. `EquipmentRecompute` gains a required `attribute_steps` serde field;
old records do not silently deserialize with an empty trace. The stat recipe
snapshot is version 2 and contains its actual level. Rebase accepts explicit
caller-validated create attributes, health, and mana; it never looks up guessed
rows. Restore requires a fresh authorized recipe rebased to the target level and
rows before replaying broken slots. Current health/mana are separate live inputs
and are not cached inside `StatRecipe`.

## Independent source comparison

`tools/generate_player_derived_fixture.py` hash-checks the pinned source files
and inserts 28 whole, unchanged methods plus `PLAYER_BASE_DODGE` into the
synthetic C++ adapter. This includes all ancillary calculations, complete
`UpdateStats`/`UpdateAllStats`, and complete `_ApplyItemBonuses`. Empty aura lists,
fixed identity/capabilities, field storage, and flat modifier dispatch are
explicit adapters. Resource and armor results are outside this oracle's output
and have separate existing recipe tests.

The 1,620 vectors cover levels 1–60 and nine item cases, each before a break, after
source item callbacks with durability committed last, and after a subsequent
full refresh. Cases include a plain weapon, each primary attribute, armor-only
agility, repeated agility/intellect entries, and an authored zero-delay weapon.
The output compares 43 actual field cells bit-for-bit, including cached display
percentages. Compilation disables contraction/fast-math and enables undefined
behavior and float-cast-overflow sanitizers. `--check` regenerates without changing
the checked-in synthetic fixture.
