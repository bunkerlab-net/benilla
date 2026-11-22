# Resident point absence and source owner filters

This opt-in path connects compatible genuine terrain/collision captures and generated
navigation to the existing retained owner query. It does not construct or widen
`DryLiquidSnapshot`, a complete-map dry proof, any movement Route, or normal world
startup. Its production acceptance remains bounded by the existing source capture,
resident geometry, query and owner-profile limits.

## Exact source branch

For one finite bounded XYZ, supported masks are ALL_LIQUIDS (15) and WATER|OCEAN
(10). VMap height calculation is enabled. The operation runs:

1. Actual source-ordered GetHeightStatic with the default 50-unit water search
2. VMap point/location selection over the complete validated index; any reached
   unloaded slot fails Missing. The existing complete-map location API is unchanged
3. Only genuine no-location or a selected group without liquid permits terrain
   fallback. A selected liquid-bearing group is Unsupported
4. The decoded terrain tile must be present and contain no liquid. Liquid-bearing
   terrain remains Unsupported, even if a rectangle or mask might exclude this point

The record retains initial static height and selected dry-group floor separately.
A false GetLiquidLevel may write that selected floor; no-location keeps the initial
height. The returned NO_WATER means caller liquid output remains unchanged. Missing
geometry is never treated as absence. This evidence applies only to that exact point,
not a tile, region, segment, target, cached player environment, or movement corridor.

The resident selector is distinct from the existing complete-map selector. It
preserves the source BIH traversal and strict highest-floor selection, including
conservative Missing when traversal visits an unloaded leaf. It does not infer that
an unloaded slot is irrelevant from missing model bytes.

## Owner and freshness lifetime

The provider requires matching consumed-source epochs between generated navigation
and compatible terrain/collision, then combines their immutable bindings with the
existing selected configuration. It retains the actual owner-constructor query.
Existing `CreatureCapabilityWitness::validate_context` and all route/session/body
checks remain authoritative for capability inputs; geometry never invents CanWalk,
CanSwim or an initialization witness.

Source createFilter uses the existing source evaluator. The supported NO_WATER path
queries masks 15 then 10 and does not call getNavTerrain. Source updateFilter obtains
two new observations and retains its prior flags without assigning new flags.

A private dependency binds provider allocation, exact live dispatch, owner pose and
witness, tickets and absolute expiry. Changed movement/context, another dispatch,
a deserialized receipt, a different provider or expiry cannot reuse that point
result. Expiry does not destroy retained filter provenance: an otherwise valid
continuation can renew it using fresh queries and a new deadline. Source changes
fail before publishing or reusing an observation. Historical geometry remains owned,
but this API checks its consumed source again for each operation. These are sequential
freshness observations; synchronous source reads/native work are not an atomic lease
or interruptible halfway through a read. A future live adapter still owns scheduling,
current world state and route admission.

## Evidence scope

Existing hash-guarded original-function filter evidence reproduces 87 rows normally
and under UBSan, including habitat 1 flags 1/0 and habitat 3 flags 13/0. Existing actual
original TerrainInfo/VMap/GridMap evidence reproduces 5,048 query rows; dry profile 25
includes 120 selected-floor writes and 28 no-location cases. These are complementary
oracles, not a claim of one integrated original resident-filter executable.

New authored MPQ tests exercise genuine capture/bundle construction, source phase
order, selected dry/no-location outcomes, wet refusal, unloaded/missing coverage,
identity/mask/coordinate guards, deadline/cancellation and source mutation. The owner
test uses actual NativeWorld constructor and movement-continuation dispatches with
genuine source geometry, and tests expiry/requery and stale receipt rejection.
Surrounding runtime metadata and motion replies are explicitly authored fixtures.

The connected resident test scope is initialized ground habitat 1 (flags 1/0).
Habitat3 still requires a genuine DryWater generation/liquid initialization witness;
the current initialization installer requires complete-dry products. No resident
point result is relabeled into that witness. Source corpus 13/0 coverage does not
claim the habitat 3 resident initialization path is connected. That integration,
player environment, remaining path stages and atomic route commit remain separate.
