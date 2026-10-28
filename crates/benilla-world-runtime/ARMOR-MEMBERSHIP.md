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

Generic carried swap, split, autostore and destroy routes explicitly do not own
registered armor. They reject those consumers rather than silently losing or
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
verification. The required native 63 identity, quest reconstruction v7 and carried-items v9
host composition have no old-save migration. Carried-items v9 does not add a
generic equip reducer or relax ordinary carried-instance validation.
