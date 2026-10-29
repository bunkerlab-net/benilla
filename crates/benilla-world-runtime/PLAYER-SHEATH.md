# Bounded player weapon sheath presentation

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`:
`CombatHandler::HandleSetSheathed`, `Player::SetSheath`/`SetVirtualItemSlot`,
`Unit::SetSheath`, and `Player::Create`. Upstream source grant GPL-2.0-or-later;
this native translation remains in GPL-3.0-or-later crates.

The real stock `SetSheathed` command preserves the raw uint32. Values 0 (stowed),
1 (melee) and 2 (ranged) are accepted without an alive, combat, posture, weapon
presence, or cast guard. Values >=3 are silently ignored **before narrowing** and
before requiring a supported owner. Ranged state with no ranged weapon is valid;
this does not invent a ranged item, ammunition or attack capability.

## Source callbacks and current ownership

Player's override of `SetVirtualItemSlot` does not publish the creature virtual
weapon fields. For main/offhand it can consume a selected item's temporary
enchantment charges. The admitted fresh initializer creates unenchanted objects;
existing source initialization, raw interaction/spell, quest/inventory and exact
equipment membership owners have no enchanting, imported item, charm or pet
mutation path. The qualification gate requires these owners and actual source
prototype/lifecycle validation. A future enchant/control producer must extend
this closure before using this route, not assume the callback is inert.

Therefore each valid command evaluates an inert selected-item callback under
that closed domain, then the source byte setter. Repeated values do not change
state or grow the changed-write ledger; this is **not** a claim that upstream
Player::SetSheath skips its callbacks. Broken, detached and retired originals do
not acquire new identities, durability, charges or modifiers from this command.
Equipment custody, allocator, spell state and stat recipes are untouched.

## Packed descriptor, persistence and limits

`UNIT_FIELD_BYTES_2` is field **164** (`0x9e + OBJECT_END`), not 139. Its low byte
is the sheath state. Source fresh `Player::Create` sets miscellaneous byte1 to
SUPPORTABLE|UNK5 = 0x28. Initial field zeroing plus the closed fresh/no-pet/no-charm/
no-import domain owns bytes2/3 as zero. Qualified player descriptors therefore
publish exactly `0x2800 | state`. Unsupported profiles omit this word: unknown
is not an invented zero. No creature virtual-item words are added.

The actual protocol accessor `unit_sheath_state` and equipment placement code
consume field 164. Projection uses the existing consolidated semantic descriptor;
it does not claim byte-identical dirty-mask/no-op packet timing or rendered pixels.
The existing UI sender is shared with headless controller tests; those tests do
not execute the full Bevy animation driver or certify animation/sound output.

The required checkpoint ledger starts at source state 0 and retains at most 2048
changed writes with strictly increasing native input sequence (sequence 0 is
valid). This is an explicit host resource bound, not a CMaNGOS restriction.
Same-state and invalid raw requests do not spend the budget. Restore validates
0..2, ordering, final value and owner eligibility; source TickRecord replay from
an earlier checkpoint establishes command chronology. The ledger itself proves
reachable state, not authenticated external sender history.

Native 78 rejects old snapshots; there is no migration. Production native entry
remains uninhabited, and no held private compiler/raw consumer or admission path
is changed. This is weapon presentation only, not stand/sit, enchantments,
spell interruption, ghost/release, or new equipment admission.
