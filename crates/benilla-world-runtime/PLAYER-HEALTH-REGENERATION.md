# Explicit standing-mage health regeneration

This is a conditional native component, not production native entry, a server
configuration capture, or a death/release implementation. The production native
capability remains uninhabited. No private compiler or raw spell consumer is
expanded.

## Explicit control, no inferred rate

The application configuration accepts an optional block:

```toml
[native_regeneration]
health_rate = 1.375 # illustrative explicit choice, not a default
```

An absent block remains `None`; a present block requires `health_rate`. Parsing
rejects unknown keys, nonfinite/negative values, values above 1,000,000 and values
that narrow to a binary32 subnormal or underflowed zero. Explicit positive and
negative zero are retained. This is a generic local host setting, not evidence of
an external CMaNGOS server's `Rate.Health`. Existing XP/drop Ruleset fingerprints
and signed world declarations are unchanged.

The conditional component entry binds the actual parsed value using
`Profile::initialize_player_health_regeneration`. The opaque immutable option is
included in the native profile digest and therefore checkpoint/record binding.
Changing a selected value is not an in-place world update; reopening against a
different profile must fail. An unset option leaves existing native behavior
unchanged and does not claim health regeneration support. The private component
entry/tests demonstrate the bridge; production entry cannot currently consume it.

## Existing source owners, not absence switches

The option requires source-initialized mage combat, quest/training ownership,
validated interactions and full existing raw spell authority. The initializer
and interaction aura whitelist excludes health regeneration modifiers 88, 116
and 161. Training preserves the admitted initial/applied aura sets. Existing
player spell effects are damage/periodic damage, while the separately validated
NPC player-holder subset is aura 33. No new boolean can assert these dependencies
away. The current strict combat restore requires standing, excludes unsupported
channel/auto-repeat state, and retains first-life death consistency. Current
Spirit and maximum health come from the replayed live stat recipe, not genesis
values. Group, transform and other unsupported producers remain outside the
existing bounded host; this is not whole-install aura or map coverage.

## One clock, source order

Pinned CMaNGOS `Player::Update` accumulates the existing shared regeneration
clock only while alive. At 2,000 ms it uses elapsed/100*100 and retains the
remainder; full health and combat do not stop that clock. Dead frames neither
accrue nor discard the remainder. `RegenerateAll` processes health before mana.

The native frame stages the existing mana owner privately to obtain its exact
scheduled diff once. It evaluates and publishes health first, then commits the
staged mana and emits its event. This is valid only in the closed domain above:
health has no mana operand or intervening observer/script callback. There is no
second timer, reset on heal, or regenerated interval guessed from wall time.
The whole frame remains transactional on numeric or persistence failure.

The GPL simulation primitive follows source numeric order: uint32 Spirit rounds
to binary32, then the unsuffixed `Spirit * 0.11 + 1` expression uses binary64,
then assignment and rate/time arithmetic use binary32; final signed conversion
truncates. It checks signed addition before maximum-health clamping and refuses
unsupported overflow rather than using saturating casts. Full health and combat
short-circuit unused numeric operands. Source oracle provenance and supported
numeric domain are in `benilla-sim/PROVENANCE.md` and the primitive.

## Evidence boundary

The compact checkpoint retains valid reachable current resources and existing
lifecycle/stat/clock proofs; it does not authenticate an entire past health
trajectory. TickRecord replay from the prior checkpoint establishes exact
command/event evolution. The health event is a semantic native event; the client
uses its existing consolidated health descriptor, not a claimed byte-identical
upstream packet schedule. Nothing here resurrects a player or introduces ghost,
corpse registration, food, sitting, in-combat regeneration auras, or default rate.
