# Bounded native quest kernels

`benilla_sim::quest` implements the pinned CMaNGOS Classic numeric quest reward
block and the independent `SatisfyQuestLevel`, `SatisfyQuestClass` and
`SatisfyQuestRace` predicates. It is a pure library, with no world rows, quest
text, database, assets, filesystem, clock, RNG or network dependency. It does
not implement `CanTakeQuest`, `CanRewardQuest`, `RewardQuest`, kill credit,
quest-log mutation, persistence or a runnable native-content profile.

## Numerical contract

The compatibility identity is
`QUEST_REWARD_POLICY_ID = "cmangos-8ec338a1-quest-binary32-rne-v1"`.
Include it and the exact rate bits in native rules/content hashes. This does not
change the authored encounter/world fixture, its fixed quest XP, existing
rational kill-XP or armor policies, replay formats or save semantics.

`quest_xp_value(QuestXpInput, Option<u8>)` ports `Quest::XPValue`:

- `quest_level` is the pinned loader's unsigned `QuestLevel`, bounded 0..255.
  Zero returns zero XP; it is not replaced by the player's level. A negative SQL
  value must not be reinterpreted as a supported scaling quest
- `reward_money_max_level` is the raw `RewMoneyMaxLevel` u32, before flags or rates
- Player level is 1..255, or `None` for the source's null-player branch
- Raw money converts to binary32, then divides by the source binary32 constant:
  0.6 for quest levels 1..60, 1.2/2.4/3.6/4.8 for 61/62/63/64, 6 for 65..255
- A player up to five levels above the quest gets full XP; differences of
  six/seven/eight/nine get 0.8/0.6/0.4/0.2, ten or more gets 0.1. Each multiplication
  rounds to binary32 before `ceilf`. These are algorithm probes, not claims that
  a selected Classic realm supports playable characters above its configured cap
- The final ceiling is converted to uint32. Undefined/out-of-range source casts
  are errors, never Rust's saturating casts, wrapping, clipping or a fixed fallback

All floating arithmetic uses the already pinned `rustc_apfloat::ieee::Single`
integer-limb implementation. Rounding is nearest-ties-even after each source
operation, toward positive infinity for `ceilf`, then toward zero for C++ integer
casts. Constants are binary32 bit patterns; no host f32/f64 arithmetic is on the
Rust authority path. INEXACT is expected; other software-float exceptions fail.
Rational algebra is not equivalent: quest level 63, player level 70, raw max-level
money 60 yields 11 XP from source binary32, versus 10 from exact decimal ratios.

`Binary32Rate::from_bits` accepts positive normal binary32 values through 1000
and positive zero. It rejects NaN, infinities, negatives including negative zero,
subnormals and higher rates. `ONE` and `ZERO` are explicit named constants; there
is no Default. Missing world configuration is not evidence for a rate of one.
Rates and numeric errors are unsupported-domain signals for the compiler/runtime.

`reward_or_required_money(raw_i32, money_rate)` ports `GetRewOrReqMoney`:
positive money is converted to float, multiplied by the money rate, then truncated
to int32. Zero/negative requirements bypass rate scaling. This narrow getter
preserves int32 minimum, but `reward_quote` rejects it because upstream
`CanRewardQuest` performs an undefined signed negation on that value.

`reward_quote(QuestRewardInput, RewardContext)` ports only the numeric statements
within `Player::RewardQuest`:

1. Calculate XPValue, convert its integer result back to binary32, multiply by
   XP quest rate, truncate to uint32. `display_xp` is computed even at level cap
2. Below the explicit configured player cap, `xp_to_give` is `Some(display_xp)`.
   This is the amount passed to `GiveXP`, not proof that XP was granted. `GiveXP`
   itself rejects zero XP/dead players, checks the cap, loads thresholds and runs
   `GiveLevel`; none of that lifecycle is implemented here
3. At/above cap, `xp_to_give` is None and `max_level_money` is Some. Raw
   RewMoneyMaxLevel is zeroed only by QuestFlags 0x0100, multiplied by the money
   rate, and truncated to int32. This flag does not zero normal/display XP
4. `reward_or_required_money` is a separate signed money operation after the
   cap conversion. Do not add both amounts and apply one operation:
   `Player::ModifyMoney` clamps each operation separately (Player.h:1389)
5. A negative ordinary amount sets `money_required_before_reward`.
   `CanRewardQuest` checks existing funds before either award; cap money cannot
   pay this prerequisite. The quote does not perform this check or debit money

All fields must be calculated successfully before a caller begins mutation. The
quote is not an idempotency key, transaction, status transition or duplicate-reward
protection. Computing it twice has no side effects. Other quest flags are outside
this arithmetic's remit; they are neither supported nor silently cleared by it.

## Three gates, not complete eligibility

`eligibility_gates(QuestEligibilityInput, QuestPlayerIdentity)` returns separate
`level`, `class` and `race` booleans. It deliberately has no `can_take_quest` result.
The minimum and maximum levels are both inclusive. A maximum of zero does not
mean unlimited in the source predicate: any loader normalization belongs upstream
of the call and must be audited separately. A zero class/race requirement means
unrestricted; otherwise source masks intersect `1 << (native_id - 1)`.

Input identity accepts Classic playable race IDs 1..8 and class IDs 1..5,7..9,11;
level must be nonzero. A legal race/class pairing must still be verified by the
selected `playercreateinfo` row. Requirement masks are full u32 values, not an
invented class ordinal or list. Loader validation of unknown bits remains required.
No optional failure-response packet is emitted by this pure predicate API.

Prerequisite support is intentionally absent. `SatisfyQuestPreviousQuest` consumes
loader-derived `prevQuests`, not just the selected row's signed PrevQuestId. It
mixes rewarded versus current status, negative exclusive-group all-member rules,
reverse NextQuestId relations and branch exceptions. Next/previous chain checks,
breadcrumb targets and dependent breadcrumbs are additional predicates. Empty
captured relations are not proof that those dependencies do not exist.

## Native compiler and runtime contract

The existing native-source gate must remain closed after linking these helpers.
No rule profile is made runnable by this module. Required integration steps:

1. Read the complete selected local `quest_template` row and all loader-derived
   quest relation/dependency closure, native questgiver relations, selected player
   identity and actual world configuration. Preserve source IDs and provenance
2. Map QuestLevel/RewMoneyMaxLevel to `QuestXpInput`; map RewOrReqMoney and the
   loader's uint16 QuestFlags to `QuestRewardInput`; resolve native configuration
   `CONFIG_FLOAT_RATE_XP_QUEST`, `CONFIG_FLOAT_RATE_DROP_MONEY`, and
   `CONFIG_UINT32_MAX_PLAYER_LEVEL` into `RewardContext`. A selected export's
   maximum captured level is not a substitute for the configured player cap
3. Map resolved MinLevel, MaxLevel, RequiredClasses and RequiredRaces into
   `QuestEligibilityInput`. Evaluate those predicates only as part of the complete
   ordered native acceptance check. Separately establish status, active state,
   skill, condition, reputation, timed/weekly limits, prerequisites/exclusive and
   breadcrumb/chain gates. `CanAddQuest` separately checks log space/source items
4. Every native field/flag must be consumed by a supported mechanism or rejected
   explicitly. In particular, conditions/scripts, repeatable/weekly lifecycle,
   exclusive/prerequisite chains, source/reward spells and mail, item requirements,
   chosen items, reputation, exploration, casts and escort objectives do not gain
   support from these numeric functions. Do not drop these fields or adapt the
   quest to the authored fixed-kill fixture
5. Before reward, implement actual completion/reward eligibility, reward-choice
   and inventory checks, and the order of source side effects. Store authoritative
   quest progress, inventory, currency, XP/level/stats and reward history in the
   same committed transaction. Preserve max-level conversion versus GiveXP and
   ordered currency application; duplicate/replay/reconnect cannot award twice
6. Keep `quest.native_eligibility_credit_reward_required` and dependency blockers
   until all selected native semantics are implemented. `quest.dynamic_xp` may
   only be resolved when a versioned compiled/native runtime calls this exact
   policy with actual source fields/configuration. Formula availability alone
   cannot remove the importer/runtime gate or validate real quest playability

## Evidence and reproduction

Source hashes and grant are in [PROVENANCE.md](PROVENANCE.md). The independent
oracle generator validates the full source files, extracts the three Quest
methods and three Player predicates verbatim, and extracts an unchanged numeric
block from Player::RewardQuest. It uses mock accessors and captures the exact
GiveXP/ModifyMoney call arguments. It does not reimplement these formulas in
Python or call the Rust implementation to generate expectations.

```sh
python3 crates/benilla-sim/tools/generate_quest_fixture.py /path/to/pinned/sources
cargo test -p benilla-sim
cargo test -p benilla-sim --release --test quest_parity
cargo clippy -p benilla-sim --all-targets -- -D warnings
```

The directory needs QuestDef.cpp, QuestDef.h and Player.cpp at the pinned revision.
The generator uses g++ C++17, FE_TONEAREST, no fast math, no contraction and
undefined/float-cast-overflow sanitizers. Reproduction must leave fixtures unchanged.
Cargo tests do not need a C++ compiler, network, native DB or installed game files.

Committed numeric probes cover 3,298 XP inputs, 144 signed-money inputs, 8,640
reward-block combinations and 4,032 eligibility inputs, plus targeted bad-domain,
overflow, ordering and rounding assertions. These are original arithmetic probes,
not native database rows. Linux amd64 is the local verified execution target;
other supported platforms must run these fixtures before parity is claimed there.
No full native quest, real content import, UI flow, multiplayer or durable reward
transaction was established by these tests.
