# Selected detached original mainhand destruction

Native format 81, quest reconstruction v16 and carried-item host v17 permit the
existing selected-destruction owner to retire a pristine original mainhand that
is already carried in an ordinary backpack slot. The source registry identity,
full prototype and exact instance must match; worn, acquired, imported, modified
or unproven objects do not acquire permission from this route.

## Source callback boundary

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`WorldSession::HandleDestroyItemOpcode` and `Player::DestroyItem`/`DestroyItemCount`.
The handler selects the actual slot first. Count zero removes the selected whole
stack; a positive count is clamped to that stack. The original source object has
count one, so every successful count retires exactly that instance. The actual
inventory reducer still checks protected-item flags. There is no alive, combat
or casting guard, and no search for a substitute same-entry object.

The source backpack branch bypasses equipped-weapon modifier, dependency, visible
slot and weapon-hit callbacks. General item duration/enchantment/script callbacks
are absent only through the existing validated original-item/prototype and host
closure, not merely because the item is detached. ItemRemovedQuestCheck retains
its source-before-deletion order. The existing one-use selected receipt/quest
permit is consumed at that exact callback, after proving the live inventory and
historical custody; ordinary indirect consumption stays blocked.

## Permanent retirement, not a reset

The exact registry handle becomes Retired and remains reserved forever. The
selected carried slot and item receipt disappear, other objects remain unchanged,
and allocator/equipped aggregate are preserved. No generic equipped item or stat
callback is synthesized. The original source metadata stays in the reconstructed
recipe with its inactive state: cleared cached weapon damage, source unarmed
attack interval, and retained unarmed skill do not revert to a never-equipped
character's initial weapon cache. Ordinary later cast/melee callbacks still use
their validated historical source prefixes. A tombstone cannot be replaced by a
same-template object, re-equipped, or reallocated.

The existing client projection clears the backpack slot and destroys the same
original GUID. Checkpoint failures withhold publication; successful retry and
reopen retain the retirement and exact owner history. No item-acquisition credit,
new identity, health/mana refill or timer reset is invented.

This remains conditional native integration, not production admission, ghost
recovery, a rendered build claim or a complete world. Held raw consumers and
private compiler paths are unchanged. Earlier snapshots are rejected without a
migration.
