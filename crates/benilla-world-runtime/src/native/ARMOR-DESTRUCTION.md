# Selected original armor destruction

The native armor owner admits destruction of an original pristine nonweapon armor
instance **after** it was moved into a supported backpack slot. This is a separate
source-owned selected handler, not generic inventory consumption or arbitrary
item deletion. Native format `NATIVE_FORMAT_VERSION`, quest reconstruction 12 and carried-items host
policy 13 reject old identities; there is no migration or native admission change.

## Source operation

Pinned CMaNGOS Classic `8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`,
`ItemHandler.cpp::HandleDestroyItemOpcode` and `Player.cpp::DestroyItem`.
The existing real selected DestroyItem reducer retains count-zero whole-stack,
positive selected-count clamp and NO_USER_DESTROY refusal semantics. A registered
original has count one, so every successful removal is whole-instance. There is
no alive, combat or casting guard for the supported carried slot. Refusals and
missing items do not create retirement history.

The source quest `ItemRemovedQuestCheck` ordering is preserved by the existing
quest reducer: pre-removal counts and quest events precede the exact selected
host removal. The source duration/enchant/script hooks remain excluded by the
original item's validated template and carried-instance closure. Backpack removal
has no equipment modifier or item-dependent aura/cast callback. This route does
not fake an unequip, recompute stats, refund resources, acquire an item or allocate
a clone.

## Custody and replay

Registry membership is permanent. A separate validated `Retired` placement has
no worn or carried instance and never releases its reserved handle. Exact
`Slot`/`Retired` maps are reconstructed from source-owned history. The inactive
original stat slot remains inactive after retirement; another same-template
ordinary item cannot impersonate the original.

A typed one-use destruction receipt binds the actual pre-removal inventory,
selected ID, source slot, original template, before-placement prefix, item
receipts and real reducer result. Only the matching quest host callback consumes
that permission. The entire transaction is cloned; any callback, receipt, source
context, capacity or checkpoint failure leaves the live state unpublished.
The after inventory must differ by exactly this one instance, with unchanged
allocator and equipped aggregate. Other registered objects remain unchanged.

Movement receipts now retain their exact before-placement map. Standalone
projection validation establishes reachable custody, not history authentication;
full journal replay compares that map with the reconstructed actual prefix.
Destruction history has the same bounded derived-call, aggregate-byte and replay
work limits as other custody transitions. Checkpoints revalidate final custody
and stat membership. Missing history, rebound IDs, duplicate occupancy and stale
placement prefixes fail closed.

Ordinary selected duplicate deletion still preserves all registered instances.
Unselected quest/reward consumption and other unsupported registered consumers
retain their existing holds. The source catalog already excludes quests that
require consuming initially equipped starter templates; this change does not
relax that content restriction. UI/controller projection uses the existing exact
selected-destroy receipt and original GUID mapping: slot removal and one object
destroy, withheld until durable checkpoint success. This is semantic session-event
projection, not a claim of byte-identical source packet scheduling.
