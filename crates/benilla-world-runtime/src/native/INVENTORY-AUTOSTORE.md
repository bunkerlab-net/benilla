# Ordinary backpack auto-store

Source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`src/game/Handlers/ItemHandler.cpp:809–858` and
`src/game/Entities/Player.cpp` (`_CanStoreItem_InInventorySlots`, `_CanStoreItem`,
`RemoveItem`, `StoreItem`, `_StoreItem`, `SendEquipError`). Classic DB item rows
already retained by the authenticated carried-item catalog remain the item inputs;
no new database, private capture, compiler, native admission or inferred metadata.

## Connected route and supported boundary

The existing stock Lua `PickupContainerItem` → `PutItemInBackpack` queue produces
`ClientCommand::AutoStoreBagItem`. The real native controller now accepts exactly
source bag255, source slots23..38, destination bag255, with the existing inventory
owner. It resolves the selected position in authenticated command sequence order.
Equipment, equipped bags, bank, keyring, container items and other topologies stay
unsupported. This is an explicit host subset, not a claim upstream rejects them.
Existing catalog proofs exclude temporary loot, foreign ownership, duration,
enchantment and item-script dependencies; unsupported evidence fails closed.

There is no invented alive, combat, NPC-giver or quest guard. The primitive checks
its actual inventory/catalog owners. An empty supported source produces no UI
event. For an occupied source, `CanStoreItem` ignores that source pointer when
searching: other compatible stacks first in ascending backpack slot order, then
empty positions including the vacated source position. Binding does not prevent
merging; source `_StoreItem` binds the actual destination when applicable.

If the only destination is the original slot, source sends `EQUIP_ERR_NONE` to
clear cursor state and does not remove/store/bind anything. `SendEquipError(0)`
serializes only its reason byte; the semantic event therefore has zero GUIDs,
no required level and zero bag slot, despite the C++ caller passing the source.

Otherwise `RemoveItem` detaches the original instance, then destinations store in
order. Since a valid ordinary source holds at most one maximum stack and its old
slot is available, any remaining empty destination is unique and final. Source
`StoreItem` therefore never clones in this domain: the allocator stays unchanged.
A final empty position retains the original instance and metadata; a final merge
retires the original only after updating the destination. Existing catalog absence
proofs cover duration/script callbacks; these are not silently executed.

## Transactions and semantic projection

The retained receipt includes complete before/after inventory, selected source,
ordered destinations, and detach/store views. Recomputing it proves source
reachability; authenticated tick history supplies input ancestry. Runtime refreshes
inventory anchors after mutations without item-added/item-removed quest hooks.
Aggregate carried counts and quest objective state remain unchanged.

Projection validates preceding carried slots and allocator plus the receipt
before publication. It clears the source player slot and contained GUID, retains
owner and the live item object, emits ordered destination updates, then destroys
only a fully merged source. Moving to an empty position does not fabricate an
`ItemCreate` or allocate a new GUID. No acquisition push or quest credit is emitted.
These are source-ordered semantic session events, not a claim of byte-identical
upstream update batching/dirty masks. Durable checkpoint failure withholds the
whole outbox; retry does not execute the move twice.

## Verification and limits

Authored primitive tests cover ordered partial/full merges, original-slot reuse,
full backpack capacity, same-position no-op/binding, identity, unchanged allocator,
unsupported dependency evidence and receipt tampering. Runtime tests use actual
grant/split operations, accepted deliver quests, replay/reopen, atomic later-input
failure, and genuine lethal NPC damage before dead-player auto-store. Actual Lua
and controller tests cover save withholding/retry/reopen, descriptor parser state,
no premature source destruction, final-merge order and zero-reason semantics.

Native format`NATIVE_FORMAT_VERSION` and inventory policy v8 reject incompatible prior context; no
migration is claimed. This remains headless source/controller integration under
the existing synthetic test entry. It does not open production native gameplay,
claim a user-tested populated world, or widen equipment/bag/P2P support.
