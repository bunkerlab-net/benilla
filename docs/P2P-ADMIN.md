# In-game Admin menu

The Escape menu includes an **Admin** button using the existing Vanilla-style
FrameXML templates. The panel covers item/bag search, gold, mounts, movement
speed, city teleportation, one-level increments, XP gain, faction reputation,
Classic honor contribution, time of day and current-location weather.

**No live admin backend is attached yet, so production operations are disabled
with an explanation. The component backend implements durable level/XP controls
and positive carried-item grants.**
Opening the panel does not grant privileges or mutate character/world state.
The group passphrase does not confer admin permission. There is no realm chat
command fallback.

## Access and catalog browsing

Open **Escape → Admin**. Opening it does not make the character an administrator.
There is no missing password or configuration toggle in production exploration:
a host integration must attach an admitted native world backend and verify its
explicit world-admin policy. The group passphrase alone is not admin authority.

Items/bags, mounts, city teleport points, factions and current-location weather
use the same catalog browser:

- Leave search empty to browse five visible entries per page, with Previous/Next
  controls, page counts and matching-result counts
- Type a name or item/catalog ID for live suggestions. Exact IDs rank first,
  followed by name/ID prefixes and name substrings; ties use case-folded name and
  numeric ID order
- Click a suggestion to select it. From search, Tab enters the result list;
  Up/Down moves through rows, Left/Right changes pages, and Enter selects. Tab
  returns to search. Enter in search selects its first suggestion
- Selection prepares an action; only the separately labeled action button
  submits it. Enter, Tab, browsing and suggestion clicks never grant anything
- Hover a row for its complete backend label, including any item-specific
  unavailability reason. Searchability and operation-level permission do not
  establish that an individual item's gameplay behavior is implemented

Changing the query, category, page, world/session, catalog contents or revision,
backend attachment or location clears the selection. Weather choices come only
from the current backend location snapshot, including its explicit Automatic
option. Empty or unavailable catalogs show an explanation; they are never filled
with example cities, item IDs or guessed weather choices.

## Saved local-world setup

**Escape → Admin → Local World Setup** opens a separate profile-owned setup
form. It does not attach a native gameplay backend or activate any Admin grant.
The current profile owner ID is read-only. **New World** accepts a 1–40 character
ASCII name and up to 64 distinct, nonzero, 32-hex-character additional admin owner
IDs, separated by commas or spaces. The owner is already included and must not
be repeated in the additional list.

The review names the owner, counts additional admins and explains that the saved
setup is immutable. Only **Save & Select New World** queues the signed declaration
and its selection for atomic persistence; pressing Enter or leaving the form does
not save it. **Saved Worlds**
shows up to 16 saved declarations in five-row pages. Selecting a row merely
prepares **Select Saved World**, which saves the selection without starting it.
The already selected world cannot be submitted again from this panel.

The setup form carries an exact profile-context and revision baseline. Changes
clear unsaved fields and row selection; stale submissions are rejected by the
separate typed setup interface. Closing or returning to Admin does not cancel a
pending save, and no success is displayed before the host reports its durable
result. The panel explicitly says that gameplay and grants remain unavailable
until the selected world can start. A group passphrase is still not permission.

## Search boundary

`BenillaAdminSearch(action, query, page)` returns at most five rows plus total,
page and generation information. Rust caches the case-folded catalog and query
matches; periodic refreshes and paging do not rescan an unchanged query. The view
binding keeps only a five-entry compatibility preview per category, rather than
copying the full catalog into Lua. The backend should reuse immutable catalog
`Rc`s until contents change; a same-revision content replacement still invalidates
the cache and UI selection.

Search accepts at most 80 Unicode characters. Each catalog is limited to 100,000
rows and each label to 512 UTF-8 bytes; duplicate IDs, invalid IDs, blank or
control-character labels, and oversized catalogs fail browsing with a reason.
These are UI resource bounds, not claims about Classic data limits. A new query
scans at most that bounded catalog once; only the visible five rows cross the
search binding. No filesystem or network work belongs in backend snapshot calls.

The actual menu includes its view generation when submitting, so a refreshed
world/catalog cannot turn a stale selection into a new request. This supplements,
and never replaces, backend authorization, typed validation and durable receipts.

## Backend contract

`benilla-ui::script::admin::AdminBackend` supplies a world/session-bound snapshot,
accepts typed requests and returns results. Catalogs and location-valid choices
come from that backend, not hard-coded example item IDs or city coordinates.
The backend must independently verify the authenticated actor, explicit world
admin policy, current character, catalog revision and location. UI checks are
convenience checks, not authorization.

Requests remain pending until a matching authoritative result arrives. A success
result must follow durable, idempotent world-state publication. Switching world
or session invalidates old result presentation. UI numeric ceilings are bounded
per-request limits, not claims about Classic ruleset maxima.

The requested item scope is the complete Classic catalog, including bags. The
component backend preserves catalog visibility separately from per-item grant
readiness, and uses actual selected-catalog labels. It does not substitute a
plain-item whitelist or fabricate missing native rows. Complete raw native catalog
capture and production admission remain separate work.

Positive `GrantItem` uses the named `CmangosAddItemV1` source policy: partial
capacity/unique success, a durable requested/granted/ungranted receipt, pre-store
quest credit and source self-target unbinding, including existing merged stacks.
The selected character is the only current destination. Zero counts, unsupported
creation dependencies, stale authority/catalog context and conflicting request
IDs fail closed. There is no deletion, overflow mail or unique-limit bypass.

Bag grants create real, empty carried container instances and preserve their
capacity through save/reopen and protocol projection. They occupy backpack slots;
equipping bags and filling their contents are not implemented. Complete five-slot
item spell records (including raw PPM bits), item sets, StartQuest and ScriptId are retained without
pretending item use or equip effects work. Random properties, timed items, scripts
and other unclosed dependencies remain visible with explicit grant blockers.

Native format 22 rejects earlier experimental saves instead of migrating item schemas, externally supplied spell-range receipts or missing creature capability and route-owner ownership.
The existing controller/store path withholds acquisition packets and UI receipts
until the authoritative checkpoint is durably published. Exact retries and reopen
reconciliation do not repeat allocation, quest credit, self-unbinding or item-push
notifications. Item acquisition is independent of corpse `ItemRemoved` events. The receipt ledger
has both a count bound and a 1 MiB serialized-byte budget, reserving 32 KiB before
each new operation so item witnesses cannot exhaust the checkpoint budget.

Item-query spell presentation is separately resolved through explicit
`sSpellTemplate` lookup evidence. The source uses spell-table cooldown/category
fallbacks only when both template cooldowns are negative; mixed-sign overrides
remain raw. Missing query spells emit an empty block while created instances
retain their raw template charges. Unknown lookup evidence blocks projection.

Classic honor contribution is distinct from honorable kills, PvP rank and modern
spendable honor currency. Future time/weather commands must use a shared world
policy and current-location validation; changing daylight must not rewind
simulation timers. Automatic time/weather reset is represented explicitly.

## Implemented native progression path

The [native admin backend](../crates/benilla-world-runtime/src/native/ADMIN.md)
now implements one-level increments and the XP toggle through the actual UI
adapter, controller, simulation and durable store. Its authored component tests
cover explicit world/owner permission, exact retries, save failures and reopening.
The production exploration session does not install this backend. A valid admitted
native world is still required before these buttons can be enabled.

The level action uses the supplied, checked progression data and reports its
supported bound separately from the configured Classic maximum. XP lock preserves
existing XP and suppresses all currently admitted XP award paths while retaining
other quest, kill and loot effects. Positive item grants are described above; the
remaining Admin operations are not implemented.

## Verification

Engine-only tests cover typed requests, malformed and oversized catalogs, exact-ID
and type-ahead search, deterministic pages, cached large-catalog work, stale
contexts/selections, keyboard and mouse selection, pending/duplicate handling,
receipts and menu lifecycle. These tests do not prove asset-backed rendering,
native database coverage or a playable populated world. Component runtime,
projection, controller and store tests cover partial item grants, real empty
containers, failed saves and exactly-once retry/reopen behavior. Native gameplay
admission remains a separate prerequisite for live admin operations.
