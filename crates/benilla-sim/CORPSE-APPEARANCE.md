# Ordinary corpse appearance snapshot — unconnected

`corpse_appearance::capture_ordinary` ports only the appearance assignments in
CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`
[`Player::CreateCorpse`](https://github.com/cmangos/mangos-classic/blob/8ec338a1704e7dcb1c0213eb7ed58f9231ade40f/src/game/Entities/Player.cpp).
It is not a port of the enclosing lifecycle. The world-client adapter passes those
words into the existing protocol `ObjectFields` representation. No production
caller publishes the result or makes a corpse from it.

## Exact bounded contract

The caller supplies complete observations **at the appearance assignment stage**:
race, gender, skin, face, hair style/color, facial hair, **native** display, guild,
hide-helm/cloak flags and the 19 actual equipped item prototypes. Current display
(after transformation or a ghost/wisp effect) is not substituted for native display.
`None` in equipment means a known empty slot, not missing metadata. There are no
fallback/default constructors for missing observations. Inputs and snapshots are
plain values, not authenticated capture records or runtime capabilities.

Packing preserves upstream operations:

- BYTES_1 has unused low byte zero, then race, gender, skin.
- BYTES_2 has face, hair style, hair color, facial hair.
- Each occupied slot stores `DisplayInfoID | (InventoryType << 24)` using raw
  uint32 operations. No additional display mask or inventory-type range restriction
  is invented; the existing client decoder separately interprets the resulting word.
- All 19 slots remain, including hidden or broken items. Hide flags do not erase
  equipment. Empty slots have explicit zero words in this complete appearance view.
- Ordinary non-battleground flags are UNK2(4), optionally HIDE_HELM(8) and
  HIDE_CLOAK(16). No bones, battleground lootability or dynamic loot state is created.
- The corpse descriptor uses display12, items13..31, bytes32/33, guild34, flags35.

The adapter does not call `into_created`; owner GUID, identity, pose, scale,
map membership, timestamps and dynamic flags are absent, rather than fabricated.
It emits no `SessionEvent`, packet or `ObjectCreate` and does not change native
profile admission, persistence or version identity.

## Tests and scope of evidence

Authored tests encode the adapter's appearance words into a test-only VALUES
packet and use the existing `parse_server_with_tail(SMSG_UPDATE_OBJECT, ...)`
opcode/mask decoder, then actual corpse appearance/item/flag accessors. They check
all 19 slots, four hide-flag combinations, known empty slots, snapshot independence,
all byte bits and high-bit item OR/truncation behavior. These are not game data,
rendering, native gameplay or player-verified corpse-release tests.

## Missing lifecycle composition

Upstream first retires the old corpse, selects its type and clears PvP-death before
`Corpse::Create`; creation failure does not roll back those earlier operations.
Success copies the owner pose/map and validates placement before appearance packing,
then saves (outside BG) before owner-registry insertion. World insertion occurs
later in `BuildPlayerRepop`. Ghost spells happen before that snapshot; final ghost
reset time occurs later than reclaim-delay notification. This component performs
none of those callbacks and does not claim transactional rollback semantics.

Real release still requires source ghost-spell/aura behavior, validated owner
observations and placement, generation-owned unique corpse registry/retirement,
world visibility, graveyard authority, interaction flags and repeated-life
persistence. Existing native release refusal and first-life restore checks remain.
