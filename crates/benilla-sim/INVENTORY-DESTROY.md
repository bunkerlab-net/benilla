# Selected ordinary backpack destruction

Source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`ItemHandler.cpp::HandleDestroyItemOpcode` (239), `Player.cpp::DestroyItem`
(10335), selected-item `DestroyItemCount` (10571), and
`ItemRemovedQuestCheck` (13488). This is carried-items-v7 with native outer55;
older outer identities are rejected, not migrated. No private content compiler,
native capability, or data admission is enabled.

## Source semantics and bounded domain

The real stock command names bag255, ordinary backpack slots23..38, and a uint8
count. Count0 destroys the whole selected stack. Positive counts are clamped to
that stack; excess does not traverse another same-entry stack. A partial removal
keeps the selected instance identity. Neither operation allocates an item ID.
There is no alive guard on this ordinary source path.

Missing items return ITEM_NOT_FOUND23. The prototype NO_USER_DESTROY flag0x20
returns CANT_DROP_SOULBOUND24 with **both error GUIDs null**, despite an item being
present. Ordinary instance soulbinding alone does not forbid destruction. The
catalog now recognizes this one explicit flag in addition to its previous0x800
policy; all other unknown flags remain unsupported. Existing starter/corpse-source
flag constraints remain unchanged; the protected-item controller fixture acquires
its separate authored row through the existing authorized admin grant route. Container contents,
equipment/unequip, bank/keyring, wrapped items, temporary loot, item scripts and
active durations remain outside the admitted domain. Existing fresh-instance
and dependency checks are not weakened to manufacture their absence.

Whole removal's source duration/script callbacks are excluded by those existing
explicit dependencies. Both whole and partial removal call ItemRemovedQuestCheck
**before** physically changing inventory. Existing quest-lifecycle code retains
source quest-log/objective order, pre-removal aggregate handling and unsigned
arithmetic; this integration does not replace it with post-removal recounting.

## Connected transaction and projection

`DestroyReceipt` is a recomputable plan containing the exact selected slot,
requested and clamped counts, before/after inventories and source outcome. Planning
does not mutate inventory. The native quest host supplies exactly one selected
removal step to the existing RemoveItems reducer. It verifies and consumes that
receipt exactly once at the original DestroyCarriedStack host callback. A selector
mismatch never falls back to first-stack-by-entry removal. Ordinary quest-reward
removals retain their existing first-stack traversal.

The source quest callbacks precede the selected receipt and its generic host event.
Projection validates its preceding carried slots and allocator, stages the
receipt, and requires the very next event to be that exact item/count host callback.
Wrong callbacks, interleaving and unconsumed end-of-frame selectors fail closed.
Physical projection removes or updates only the selected instance. No ItemPushResult
or acquisition credit is emitted. Existing reputation/objective replay still sees
the original generic quest host event, including ordinary nonquest trash removals.

The actual native controller routes the stock UI command through authenticated
sequencing, runtime reduction and the durable checkpoint outbox. Checkpoint failure
withholds publication; retry must not remove the item again. Runtime replay binds
the source receipt to its actual ancestor. Session events use the existing semantic
ObjectValues/ObjectDestroyed representation, not a claim of byte-identical upstream
packet encoding.

Tests use original authored fixtures and the actual Lua cursor command/controller,
not private game content. They exercise selected duplicate stacks, partial/whole/
excess counts, protected and bound items, pre-removal quest transitions, allocator
preservation, malformed receipts/callbacks, rollback, checkpoint retry/reopen and
real simulated player death. These do not establish graphical user verification,
production native entry, or a complete populated Classic world.
