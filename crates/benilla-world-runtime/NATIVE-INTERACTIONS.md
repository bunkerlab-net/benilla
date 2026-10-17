# Native source interaction host

`native::InteractionCatalog` is the first semantic host in the executable native
runtime. It resolves `QuestStatus`, `QuestInteraction` and `LootInteraction` from
immutable source inputs and an internal borrow of the exact executing draft.
It does not construct `NativeCapability`, parse raw content, query a database,
open native startup, or establish playable Classic integration. Tests use
explicitly authored rows in `SyntheticComponentTest`; their interaction answers
are derived by this host, not authored success callbacks.

## Source contract

Pinned CMaNGOS Classic commit:
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`.

- `Quests/QuestHandler.cpp:38–88`: status queries use generic `CanInteract`
- `Quests/QuestHandler.cpp:90–112,718–736`: Hello requests mask NONE;
  accept/reward requests QUESTGIVER through `GetNPCIfCanInteractWith`
- `Entities/Relations.cpp:586–660`: generic status uses current NPC flags,
  UNINTERACTIBLE, ghost handling and both directional reactions; immediate
  interaction additionally checks taxi, charm, liveness, forms and current victim
- `Entities/Player.cpp:2414–2456`: current in-world/taxi/control state, flags,
  immediate interaction, alive invisibility and interaction distance
- `Loot/LootHandler.cpp:90–132`: player alive/in-world/standing/stunned and
  the existing source loot-distance formula
- `Entities/Creature.cpp:1865–1947`: death clears NPC flags; revival restores
  template NPC flags; spawn overrides therefore require separate implementation
- `Entities/Player.cpp:20101–20113`, `Entities/Unit.cpp:5211–5223` and
  `Loot/LootHandler.cpp:156–161`: interaction/loot can remove even passive
  numeric auras through AuraInterruptFlags
- `Entities/Unit.h:328–392,480–501,1598–1613,2283`: exact control masks, spirit
  service flags and the distinction between IsTaxiFlying and IsStunned

No LOS fact is requested for these calls. Status does not inherit an immediate
interaction range, alive, taxi, combat or returning-home gate. Returning home
without a current victim is not a standalone interaction denial. A hostile or
unfriendly directional rank is a gameplay rejection, not missing authority.

Source file SHA-256 identities:

| File | SHA-256 |
| --- | --- |
| Relations.cpp | `06e453c80b33cddf1606004055a5d1407a46007b20fef73c5dc917156256f77b` |
| Player.cpp | `d02d693f2575b0f47be405fe6e81e2a4e833fca8d149354d9cb8d3b458527c7e` |
| Unit.cpp | `03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0` |
| Unit.h | `2d9db8de53e4881a6684eeb2e31bc904019ee5e620aa067ecd68bd5d07c5d8a1` |
| Creature.cpp | `27ae75c9e90febd0827fd60f9c9fe27bfeeab34e476d10ac784ab1039d4b8302` |
| LootHandler.cpp | `1bd49276ce292e5991bf7a4f4fd3ac572d00b5230e229c3dd2bcb98c7c686232` |
| QuestHandler.cpp | `00844a75c11d7532475eb8ae2ed9118a249bf51aa07b06ce4ad3979e80c1b768` |

## Immutable inputs and current state

`Profile::initialize_interactions` accepts typed player/NPC initial control
fields, template NPC/unit/type flags, full faction-template comparator fields,
raw template flags and explicit Faction.dbc reputation-list metadata. It checks
each spawn’s faction against its source entry, NPC flags against the quest
catalog and unit/type flags against the existing
regeneration policy. Duplicate/missing/inconsistent rows never become neutral
reactions. A tracked faction requires the validated source reputation catalog;
its metadata must agree with that catalog. No renderer or client colour catalog
supplies a relation decision.

The immutable catalog carries explicit `Absent`, `Present` or `Unknown` evidence
for mutation hooks, controlled units, PvP/duel/sanctuary, forced reactions,
temporary factions, immunity overrides and NPC aura/script hooks. Unknown and
known-present evidence remain distinguishable errors. Nonzero initial control
fields and unimplemented legitimate flags are Unsupported rather than false
facts or gameplay refusals. These supplied inputs do not prove raw-source
completeness and cannot open production admission.

The private current view derives NPC presence, generation, health, victim,
combat flags and death-cleared NPC flags from runtime state, and player combat
and looting flags from current owner state. The active NativeWorld owns presence;
no logout or map-transfer operation exists in this domain. Player death stops at
the unreleased body, so it cannot introduce ghost state. Initial owner/charmer,
mount/form/aura/control fields must be explicitly ordinary and independent.
The only admitted executable spell effects are direct and periodic damage;
source initialization/training admits only the listed numeric stat/resistance/
skill passives. Neither those reducers nor ordinary NPC AI can introduce a
stun, charm, taxi, form, temporary faction or external override. Their absence
is a checked closed-domain invariant, not a new default-true authority callback.
The source initializer retains raw AuraInterruptFlags (Spell.dbc word 22).
INTERACTING (0x400) or LOOTING (0x800) on an initial passive explicitly prevents
installing this host until ordered aura/stat/resource removal is implemented;
those bits are not a permanent gameplay denial. Live training cannot introduce
a different passive-aura set.
New effects or mutators require extending this current-state view and its proof.

Directional reactions call the source-native relation primitive with the exact
current `NativeReputationState::view()` and immutable reputation catalog. A quest
reward earlier in the same input batch therefore affects a later interaction,
including AT_WAR asymmetry and the full Unfriendly/Neutral/Friendly rank range.

## Routing, witnesses and replay

Each semantic query contains a witness over the full private state checkpoint,
plus either `ReadOnly` or its frame reply/action ordinal. The host verifies the
world/profile/policy/geometry/tick/NPC binding, exact current positions, health,
entry and flags. Preview and execution witnesses are not interchangeable. Tick
zero read-only status never enters the spatial adapter's movement-only
`Session::body` or requires a collision body.

With a source catalog installed, the three semantic queries are resolved inside
`NativeWorld`; missing source context is never offered to external `Authority`
as a fallback. An absent catalog permits authored interaction replies only in
the explicit synthetic domain. Movement and the still-unimplemented semantic
queries continue through the existing external authority contract.

Derived replies are recorded among external replies in original request order.
`Authority::observe_derived` can observe or reject a generated reply, but returns
no replacement facts. Replay overrides this hook to compare and consume the
next complete recorded request and derived result. External replies still use
`resolve`. Stale/missing/extra/reordered records, changed source facts, wrong
draft/ordinal or preview reuse fail atomically. Old queries without witnesses
are rejected by decoding, even before profile-identity checks.

These three queries execute outside an NPC's temporary working-state removal.
The private host refuses an incomplete NPC scene. Broader spell/melee/NPC hosts
must explicitly carry the working NPC and post-movement state into their view;
this complete-scene view is not evidence that those callback timings are solved.

The current bounded implementation constructs a full checkpoint hash for both
query creation and verification. Status sweeps therefore scale with the full
state per query. Witness creation checks the serialized full-state size against
8 MiB before hashing, so an oversized draft stops before a second witness or
external-authority call; digest bytes remain identical to the full-state helper.
The first snapshot clone and JSON allocation still occur before this check.
A future private draft-revision cache or borrowed streaming hash
can reduce that work, but must invalidate after every relevant draft mutation.
The actual controller resolves one NPC per status request; its driver drains at
most 64 requests before pumping again (`world/driver.rs`, `MAX_ACTIONS`). Runtime
batches cap 64 inputs, and all authority replies cap 20,000 per frame. Profiles
cap 4,096 NPCs, so one full status sweep needs at most 4,096 requests across at
least 64 driver turns, totaling up to 8,192 full-state hashes. Committed checkpoint
and journal envelopes cap at 8 MiB; that is not a pre-allocation or transient
memory cap inside witness construction. Speculative drafts can grow before the
new serialized-state guard or the end-of-frame checkpoint rejects them. The max-NPC repeated-status regression
measures one 64-query drain without asserting a machine-dependent latency bound.
This cost is not a production-readiness or cheap-query claim.

## Coverage boundary

| Branch | Current handling |
| --- | --- |
| Tick-zero status, generic NPC flags and source directional ranks | Derived |
| Immediate Hello versus questgiver mask, current victim, spirit services | Derived |
| Quest and loot distance | Existing runtime source formula, no LOS addition |
| Current reputation after ordered quest reward | Derived from current source state |
| Loot in-world/stun subchecks, standing/lifecycle | Derived within closed control domain |
| Unknown evidence or missing source metadata | Unresolved/unsupported, never neutral |
| Interaction/loot-interrupted passive aura removal | Explicit unsupported callback path |
| Known charm/pet/taxi/stun/form/ghost/spawn override/PvP branch | Explicit unsupported domain |
| Gameobjects, pets, scripted dialog/loot, second player | Unsupported |
| Spell/melee/NPC semantic answers | Still external, not implemented by this host |
| Raw content completeness, production startup and whole Classic | Not admitted |

Observed Linux test workload (64 queries / 128 complete hashes, before the
serialized-size guard): the two-NPC / 8,764-byte checkpoint took 5 ms; the
4,096-NPC / 2,941,970-byte checkpoint took 1,662 ms. These are measurements of
one fresh-state fixture on one build, not worst-case history, memory or latency
bounds. The test reruns the workload without a machine-dependent time assertion.
