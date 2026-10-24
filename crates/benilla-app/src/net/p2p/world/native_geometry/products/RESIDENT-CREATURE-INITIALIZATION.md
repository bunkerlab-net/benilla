# Resident creature initialization

The pinned CMaNGOS Classic source is
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`: Creature.cpp InitEntry and
GridMap.cpp IsSwimmable (1147–1160). The existing runtime selects model/gender,
resolves that model's collision height, asks the initialization authority, and
only then selects level. The resident adapter does not change this order or the
existing habitat-3/ExtraFlags/preexisting-swimming acceptance constraints.

`PreparedSourceHeight::initialization_binding` derives metadata from compatible
actual terrain/collision owners, their complete consumed-source epochs, selected
map and content, and a distinct fixed resident policy. Select it before building
the Profile. The snapshot DTO is source-query selection metadata, not complete
dry inventory, source authentication or a live lease. It does not use or fabricate
legacy MapHeightSnapshot products, missing VMap tiles, or navigation products.

The explicit VMap-height-enabled configuration is a local policy selection, not
evidence of an external server setting. Disabled and unknown configurations are
outside this adapter's supported domain: the original CanCheckLiquidLevel can
fall back to GetGrid with VMap disabled, so this refusal is not full source parity.
The effective configuration digest is derived locally; copied hashes cannot
turn a missing or disabled selection into the enabled policy.

`ResidentCreatureInitializationProvider` borrows the actual source view. For each
real runtime Query::CreatureInitialization, it validates the outer/request and
Profile binding and asks `point_no_water` at the exact position/ticket with mask
15. That primitive observes static height, VMap liquid/location, then GridMap.
Any present liquid, unsupported input or missing coverage refuses. NoWater
short-circuits IsSwimmable's depth test: selected model collision height remains
the later threshold, while the nested classification height is independently
fixed to binary32 `0x4002007e` (2.03128). Neither height nor radius is substituted.

Cancellation, the original absolute deadline and source freshness surround the
observation and are checked again before facts escape. These sequential checks
are not an atomic lease. The returned facts contain the exact request, NoWater,
and no liquid data. They contain no manufactured pointer provenance. The adapter
retains no opaque point result across calls; a newly bound equal-byte source may
freshly answer the same immutable historical request. NativeWorld alone proves
current generation, draw and clock history by consuming its exact request.

The authored shared source install tests exercise actual InitEntry and owned
capability/model-height witnesses, wet/missing refusal, configuration/transcript
mismatch, late cancellation/expiry/source mutation, historical re-query and live
runtime rollback/retry. Model and starter catalogs remain existing authored
fixtures. This point result proves no region, segment, route, corridor, movement
or production admission, and enables no source acquisition or importer.
