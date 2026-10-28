# Explicit ordinary backpack stack splitting

Source: CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`ItemHandler.cpp::HandleSplitItemOpcode`, `Player.cpp::SplitItem` (10592),
`_CanStoreItem`, `_CanStoreItem_InSpecificSlot`, `_StoreItem`, and
`Item.cpp::CloneItem`. `inventory::split_backpack` is part of carried-items-v8,
not an expansion of private content admission.

## Supported semantics

- Actual packet count is uint8. The controller silently ignores same positions,
  then zero count, before its explicit backpack topology check. Other supported
  requests name bag255, slots23..38; unsupported bag/equipment/bank routes remain
  refused, not silently mapped to backpack positions.
- Missing source gives ITEM_NOT_FOUND23. Full source count gives
  COULDNT_SPLIT_ITEMS27; too much gives TRIED_TO_SPLIT_MORE_THAN_COUNT26.
  There is **no alive guard** in this ordinary source path (unlike SwapItem).
- After those guards, CloneItem consumes a real next-item identity. Source count
  is reduced before unique-count/storage checks. A storage refusal restores count
  but consumes/retire the allocated clone ID. The caller commits that allocator
  change even when no visible item counts changed.
- A full or incompatible explicit destination gives ITEM_CANT_STACK19. A partial
  compatible destination fills first, then ascending backpack stacks are considered.
  The reduced original source is eligible: the pointer passed to CanStoreItem is
  the new clone, not that original. Earlier matching slots can consume spill first.
- One clone allocation suffices in this bounded topology: an empty explicit slot
  fits the split; otherwise the reduced source always has room for remaining count.
  No forced fallback to a new empty slot or extra clone is invented.
- CloneItem creates template-default durability/charges and copies flags/duration/
  creator fields. Existing admitted instances already have fresh durability/charges,
  zero duration/random properties/enchantments and no creator extensions; these
  invariants remain strict. Bound state is copied, then normal StoreItem bonding
  applies. This is not arbitrary instance copying. Container-source splits remain
  unsupported; equipped bags, keyring, temporary/generated loot and scripts remain
  excluded by the existing catalog scope. Known absence is never guessed from missing data.

`SplitReceipt` retains before/after inventory, allocated clone ID, ordered
storage destinations and intermediate publication views. Successful source count
publication precedes ordered stores; a final merge retires the temporary clone.
Receipt validation recomputes everything. The runtime separately authenticates
its owner, input sequence and ancestor state and refreshes inventory receipts
on allocator-only refusals. No items are acquired and no quest credit is added.

## Connected host and tests

The existing stock inventory UI emits SplitItem. The actual native controller now
routes the bounded request through the normal sequenced runtime, projection and
checkpoint outbox. Projection uses existing ItemCreate/ObjectValues representations
for instance creation/updates; this is semantic session-event projection, not a
claim that its packet choices are byte-identical to upstream's explicit
SendCreateUpdateToPlayer. It preserves reduced-source-before-store ordering,
projects final temporary-clone destruction, and emits no ItemPushResult or quest
acquisition. Refusal uses source GUID and null second-item GUID.

Projection also checks the inventory allocator across grants, rewards, swaps and
splits against its preceding frame and final committed state. Checkpoint failure
withholds all successful events; retry must not split or allocate again.

Authored tests cover original/clone identities, empty and partial-merge targets,
spill into source/earlier stacks, unique limits, binding/template metadata,
allocator exhaustion, distinct count errors, post-clone refusal, receipt tampering,
actual Lua cursor request, actual command path, dead-player split, checkpoint
failure/retry/reopen and runtime rollback/replay. They are not real game-content,
graphical player verification or production native admission. Native55 rejects
older outer formats rather than migrating or reinterpreting their policy.
