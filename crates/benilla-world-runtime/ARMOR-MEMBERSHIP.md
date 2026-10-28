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

Generic split, autostore and destroy routes explicitly do not own
registered armor. The separate custody-only relocation route described below is
the only supported carried swap exception. They reject those consumers rather than silently losing or
relocating a registered object. Indirect quest consumption is likewise closed. The removal host is deliberately
conservative by entry: while an original registered armor object is carried, it
rejects consumption of that entry even if a selected ordinary duplicate could
otherwise be removed safely. This is an unsupported bounded-host case with atomic
rollback, not a CMaNGOS rule or protected-item error. Narrowing it requires a
separately validated selected-receipt path.

## Presentation and limitations

Projection retains the existing object identity while updating container and
player slot references. A removed item is not destroyed and recreated. Visible
entry words remain owned by the existing complete query-metadata gate; this is
semantic session projection, not a claim of byte-identical upstream packet order.
Headless parser/controller tests do not establish rendered appearance or user
verification. The required native 64 identity, quest reconstruction v8 and carried-items v10
host composition have no old-save migration. Carried-items v10 does not add a
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
