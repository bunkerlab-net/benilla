# Conditional resident Chase water dependency

This continues the model-owned numeric/LOS prefix only when that prefix is clear.
Source owner.IsInWater uses ALL_LIQUIDS=15 and default collision height 2.03128f
(bits 0x4002007e). It requires a fresh exact owner-point observation after LOS;
the earlier constructor mask15/mask10 observations cannot substitute. Only the
existing absence-of-liquid branch is supported. The recorded collision-height
bits annotate the source operand; point_no_water does not consume that height,
because it proves absence before any height-dependent liquid classification.
This cannot justify a wet-status implementation without the source operand.
Missing required geometry and selected wet products remain errors, not dry answers.

The next source call is Player::IsInWater on the runtime-owned target cache.
require_target_water validates the actual owner, target, time/phase, current pose,
height source and environment history. An intentionally older cached query
position is valid during the source countdown and must not be resampled here.
Numeric/blocked LOS prefixes skip both water calls, including missing target
cache dependencies that the source never reaches.

## Metadata-only source compatibility

An already-provided complete LiquidCatalog must hash to the exact full WDBC
SHA256 in the genuine terrain capture's selected LiquidType.dbc receipt. Its
is_complete flag is checked independently: the legacy reader can hash identical
bytes without selecting the complete profile. No raw bytes are returned, no new
catalog is loaded, and no extractor/importer is added.

A dedicated resident policy derives the environment source epoch from genuine
terrain/collision source identity and the inventory hash from the actual selected
composition revision, map and catalog. Content remains explicit world
configuration, not server-database authenticity. Every Profile environment
snapshot field must equal this locally derived binding; caller inventory/policy
hashes are never inherited. Legacy full-inventory bindings cannot be relabeled.

This establishes compatibility of consumed source versions, not whole-install
equality, an atomic filesystem lease or ownership of the original catalog
acquisition. Sequential freshness checks preserve the immutable historical
snapshot contract. Source verification and bounded native calls have documented
synchronous sections. Existing geometry residency and point-only limits remain.

## Conditional evidence, not movement admission

Private dependencies retain the exact prefix/provider allocation, live request,
model witnesses, compatible environment binding, actual target-cache transcript
and capped expiry. Diagnostic access checks those again. Failures do not publish
a Route, Facts, corridor/cache or movement commit. Fresh retry needs fresh valid
evidence; expired observations are not revived.

Authored tests use matching captured catalog bytes and actual NativeWorld
movement/countdown reducers. Their surrounding environment authority replies and
model/starter initialization remain authored test inputs. A production resident
player-environment responder, real-install model/starter acquisition, complete
NearPoint/support and atomic Chase admission remain separate work. This consumer
alone does not prove that historical environment replies came from a connected
production responder or that a populated game session is playable.

The new composed fixture covers clear and blocked LOS, dry-owner sampling,
authored dry/wet runtime target caches, before/after countdown ownership, older
cached positions, binding mismatches and stale evidence. Owner wet/Missing
short-circuit and initial sequence-zero cache behavior retain complementary
existing coverage; they are not newly demonstrated end to end here.

The separate [resident dry environment provider](RESIDENT-PLAYER-ENVIRONMENT.md)
now offers a genuine point-absence reply path into the existing cache reducer.
It remains a separate component from this consumer and does not widen wet query
support or normal-session installation.
