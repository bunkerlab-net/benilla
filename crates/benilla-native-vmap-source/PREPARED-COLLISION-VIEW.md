# Prepared collision queries

The app's private `PreparedCollision` view accepts a genuine assembled source map.
It offers historical static collision, line-of-sight and VMap height queries. It
requires no terrain or navigation placeholders and cannot become a full product
inventory or a gameplay admission token.

## Validation and identity

Construction accepts ordinary tiled maps. It checks tile coordinates, exact typed
model names, effective output collisions, the complete referenced model set, and
the existing serialized-product envelopes. The same decoder used by the full
inventory then validates models, transforms, hierarchy references and enclosing
bounds. Every primitive slot in the selected map tree must have a loaded spawn.
WMO liquids may be decoded for structural validation; this view exposes no liquid
or composite terrain-height API.

The binding includes the captured preprocessing identity and a hash of every exact
product identity and byte sequence. It uses a separate namespace from the full
inventory. Existing full-inventory requirements, revision formats and readers are
unchanged. No source file or generated product is written by construction.

## Historical snapshot and freshness

The view owns decoded geometry and borrows the source bundle. Its queries describe
that immutable version, even if files on disk later change. The source-version
binding is available to callers; a query does not promise that it describes the
current install.

Construction rechecks the captured inputs before and after decoding. Callers can
explicitly invoke `verify_unchanged` to compare the retained capture with current
inputs. This comparison is not a filesystem lock or live lease. An eventual
live-use adapter must perform an explicit freshness check for its new operation
and manage its own bounded validity interval. No live-use adapter is enabled here.

## Resource limits

The view inherits the existing decoder's 16 MiB aggregate serialized VMap budget,
model/group/spawn limits and query work/deadline checks. It allows at most 4,096
products. Input bytes are borrowed during construction rather than copied into a
fake full inventory; decoded geometry is owned. These are validation limits, not
a claim that every real world map fits.

Cancellation is checked during row enumeration, hashing in 64 KiB chunks, and
source freshness checks. The bounded complete geometry decode is synchronous;
cancellation is checked immediately before and after it. It does not promise a
fixed wall-clock cancellation latency. Failure exposes no partially decoded view.

## Remaining work

Real-install source acceptance and larger-map resource behavior still need
verification. Genuine terrain composition, navigation production, global WMO
maps, normal world startup and gameplay admission remain separate work. The
source crate is now a normal app dependency, but the authored-map test adapter
feature stays confined to development dependencies.
