# Bounded original armor membership

This integration is under validation. It does not grant native admission, import
private source data, implement arbitrary equipment swaps, or establish playable
world completeness.

Source: CMaNGOS `mangos-classic` commit
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, Player.cpp `SwapItem`,
`CanUnequipItem`, `CanEquipItem`, `RemoveItem`, `EquipItem`, `_ApplyItemMods`,
`HasItemFitToSpellReqirements`, and `RemoveItemDependentAurasAndCasts`.

## Closed supported operation

Only an original source-created pristine armor object can move between its own
original equipment slot below 15 and an empty ordinary backpack position. Its
registered handle and genesis-origin GUID remain the same. No clone is allocated,
no quest acquisition/removal hook is synthesized, and no weapon, replacement,
bag, damaged object, repair, or arbitrary same-template substitute is admitted.

The full raw spell-authority owner is required. Its raw EquippedItemClass = -1
constraint establishes the source early return for item-dependent casts; a reduced
ability description is insufficient. Source item rows close item sets, equip/use
spells, duration, random properties, script and block effects. Item::Create and
VisualizeItem supply retained original creation/binding facts, checked against the
catalog and all admitted mutable producers. Unknown metadata is not zero evidence.
BoE objects are excluded rather than relaxing the carried-instance validator.

Source ordering remains asymmetric: an alive player may unequip during a generic
cast, but re-equipping checks the generic spell slot; traveling projectiles do not
block. Armor changes in combat and changes while dead are refused. Unsupported
control/logout/form/aura paths are excluded by the existing source-bound owner,
not caller-provided permission booleans.

## Owners and replay

The immutable item recipe retains original rows. A required version-3 membership
list is reconstructed by replaying real modifier removal/addition, separately from
durability breakage. Every modifier step keeps source resource clamping; adding
maximum health/mana does not refill current resources. Historical spell and melee
contexts use their own validated journal prefix, not the final loadout.

A custody operation binds the registered original, exact source/destination,
before/after carried inventory and checksum receipts, unchanged allocator, source
context, and the real stat operation. Each original exists exactly once: worn in
its original slot or carried in its recorded backpack slot. Source replay occurs
before combat restore; incoming inactive flags alone are never authority.

Retained contexts prove source reachability and owner-bound prefix consistency,
not authenticated past chronology. TickRecord replay supplies execution from the
previous world state. Item receipt hashes are checksums, not creation authority.
Live execution reads actual lifecycle/combat/cast owners. Aggregate history work,
call counts and checkpoint byte limits still apply.

Generic split and destruction do not own registered armor. Carried swap and
AutoStore can move it only through their separate custody journals below. They
never silently lose a registered object. Indirect, unselected quest consumption
retains the conservative entry-wide hold when an original of that entry is
carried. A selected ordinary duplicate may be destroyed only after the exact
selected receipt proves the selected ID is not registered and every registered
carried slot/full instance, equipped aggregate and allocator is unchanged.
This is not permission to destroy the original or substitute a different stack.
The remaining hold is an unsupported host subset, not an upstream protected-item
rule or invented inventory error.

## Presentation and limitations

Projection retains the existing object identity while updating container and
player slot references. A removed item is not destroyed and recreated. Visible
entry words remain owned by the existing complete query-metadata gate; this is
semantic session projection, not a claim of byte-identical upstream packet order.
Headless parser/controller tests do not establish rendered appearance or user
verification. The required current native identity, quest reconstruction v11 and carried-items v12
host composition have no old-save migration. Carried-items v12 does not add a
generic equip reducer or relax ordinary carried-instance validation.

## Carried original relocation

After detachment, that same original may move to a different empty ordinary
backpack slot through the actual SwapInvItem/InventoryMove route. Pinned
Player::SwapItem applies its alive guard here, but does not call CanUnequipItem or
CanEquipItem: combat and generic casting do not prevent a backpack relocation.
Same-slot input is ignored and a dead player receives the existing source error.
Occupied destinations, swaps with another object and every other registered-item
consumer remain unsupported host subsets; these are not invented CMaNGOS errors.

The actual bounded inventory reducer supplies the receipt. A separate
ItemRelocation journal binds the original handle, complete before/after custody,
checksum receipts and reachable owner context while leaving the stat recipe,
physical equipment, derived caches, equipped counts and allocator unchanged.
There is no fabricated equip/unequip, quest acquisition/removal, cooldown or
modifier callback. Its journal prefix participates in historical reconstruction,
monotonic command/time checks and the existing aggregate byte/work budgets.

Projection reuses InventoryMoved's existing final-state semantic updates: old
backpack pointer clears, the destination points to the same object, and final
containment remains the same player. It does not model transient detach/re-attach
packet ordering. No visible equipment entry, item creation/destruction or item
push is synthesized. Headless Lua/controller checkpoint retry and replay tests
are not rendered gameplay or native admission evidence.

## Source AutoEquipItem handler

`EquipmentAutoEquip` is a distinct command, not an alias for SwapItem. Only a
registered original carried in the ordinary backpack, with a singleton source
inventory-type slot equal to its original empty slot, is supported. Finger and
trinket alternatives, occupied-slot swaps, arbitrary carried objects, bags and
already-equipped source positions are unsupported host subsets. Missing carried
sources are silent, before equipment guards.

Pinned ItemHandler.cpp `HandleAutoEquipItemOpcode` calls CanEquipItem(NULL_SLOT)
before CanUseItem's alive test. Inside the existing sealed unsupported-state
closure, the live priority is combat, then generic casting, then alive; SwapItem
keeps its original dead-first priority. Traveling spells do not block. The pure
guard-matrix test covers decision order, not claims that every authored boolean
combination is a reachable world state. The source initializer established
prototype eligibility and proficiency; the existing live skill owner preserves
those source-established skills and bounds, with no deletion or lower values.

The initial and current physical offhand must be absent. This proves the actual
AutoUnequipOffhandIfNeed immediate return; moving body armor alone would not
prove that callback inert. Successful moves use the existing exact custody and
ordered modifier operation, with a required Swap/AutoEquip discriminator in the
retained context. Auto replay validates singleton selection and the absent-offhand
source dependency. It does not invent an equip callback or relax recipe restore.

The controller accepts the existing stock AutoEquipItem wire command only for
ordinary backpack positions. Headless tests compose the real Lua cursor queue,
real wire mapping and extracted actual non-ammo sender with the actual controller,
checkpoint withholding/retry/reopen and same-object no-refill result. They do not
execute the complete Bevy Items/template/ammo dispatch branch or prove rendered
appearance. The tiny sender extraction leaves that existing dispatch unchanged.
The current native format, quest reconstruction v11 and carried-items v12 have no old-save migration
and grant no native admission.


## Carried original AutoStore

The distinct `ItemAutoStore` custody journal composes the existing real
`AutoStoreReceipt` for a registered original already in the ordinary backpack.
Pinned ItemHandler.cpp `HandleAutoStoreBagItemOpcode` has no alive/combat/casting
check for this source position. It is not replayed as SwapItem's alive-bound
relocation. Occupied positions are skipped; the first source-compatible empty
position includes the vacated source. The original's source-owned stack limit of
one excludes merges and allocation. Same-position results retain the existing
reason-zero, zero-GUID inventory response and do not append a custody operation.

A successful journal replays exact source slot, original instance and metadata,
ordered detach/store views, receipt ancestry, unchanged allocator and equipped
aggregate, and exact resulting placement. The existing owner-bound resource
boundary is retained without an alive requirement; recipe, stat caches and quest
objectives are untouched. Mixed membership/relocation/AutoStore histories retain
strict command sequencing, bounded replay work and aggregate byte limits. Other
registered-object consumers remain held. This changes no generic item validator,
source death semantics, ghost state or native admission.

The existing `InventoryAutoStored` semantic projection retains the same item GUID,
represents its detach/reattach, and introduces no new-item push, item creation or
visible-equipped-slot update. Tests cover actual casting/combat/death transactions,
replay, real Lua intent into the actual controller, checkpoint withholding/retry
and reopen. These are headless checks, not rendered-player or production-entry
verification. The current native format/quest-v11/carried-items-v12 reject older saves without
migration.
