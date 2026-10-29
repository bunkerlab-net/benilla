# Carried original mainhand custody

Native format 75, quest reconstruction v15 and carried-item host v16 extend the
existing separate custody-only relocation and AutoStore receipts to a detached,
pristine, source-initialized original mainhand. This does not admit acquired or
imported weapons, occupied-destination swaps, bag/equipment routing, destruction,
indirect quest consumption, repair, or another weapon-modifier callback.

## Source branches and exact owner

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`Player::SwapItem` backpack-to-empty-backpack branch and
`WorldSession::HandleAutoStoreBagItemOpcode`, with `CanStoreItem`, `RemoveItem`
and `StoreItem`. Backpack SwapItem checks alive but has no equipment combat or
casting guard. AutoStore has no alive guard, excludes its source from merging,
then considers the vacated source in its empty-slot scan. Its same-position
outcome emits source error 0 before mutation, without binding or custody history.

The validated original object is count 1, non-container, pristine, and has the
existing complete item/prototype identity and absent duration/enchantment/script
hooks. The source-owned registry and historical placement prove it is currently
carried, not worn or retired. The same actual inventory reducer must recompute
one empty destination, identical item facts, unchanged allocator frontier and
equipped aggregate. Exact item receipts and the prior custody prefix remain
mandatory; changing a source slot, instance, destination or ancestry is not
accepted merely because a final inventory looks plausible.

Relocation retains the real alive operand/refusal. AutoStore uses its distinct
no-alive source operation rather than relabeling a Swap receipt. Both preserve the
inactive original mainhand recipe, cached weapon damage, live derived statistics
and weapon skill. Neither resets a melee timer: ordinary frame countdowns and
independent spell/combat events still advance normally. They add only their
existing custody journal variants, not ItemMembership or ItemBreak. A later
supported AutoEquip still uses the exact same original object and separately
replays the actual equipment callbacks.

## Projection and durability

Existing semantic inventory projection retains the same item GUID, clears the old
backpack pointer and sets the destination pointer. No equipment-transition-specific visible update, new object, item-acquisition
credit or equipment stat callback is invented. The ordinary consolidated player
descriptor still refreshes unchanged equipment/stat fields each frame.
The relocation projection represents final state rather than claiming an exact
transient RemoveItem packet sequence; AutoStore retains its existing ordered
semantic views. Checkpoint failure withholds publication, retry is transactional,
and reopening reconstructs custody and the inactive weapon recipe.

Production native capability and held raw compiler/consumer paths remain closed.
This is a conditional native gameplay route, not proof of rendered gameplay or a
complete populated world. No old-snapshot migration is provided.

The later [selected mainhand destruction](MAINHAND-DESTROY.md) contract is a
separate typed retirement operation. Custody movement itself does not authorize
consumption; generic indirect requirement removal remains held.
