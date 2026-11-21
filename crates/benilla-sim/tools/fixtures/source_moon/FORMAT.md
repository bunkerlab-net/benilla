# Lunar oracle fixture format

## Scope

These fixtures are authored C++ component evidence. They are not historical F1 inputs, a bound historical sine implementation, production admission or proof of a complete lunar runtime. The expected data must be generated independently of Rust results. The oracle owner has not accessed the Rust implementation worktree, its handoff or its history.

Only the input manifest, expected data and provenance receipt are public fixture material. No C++ implementation, original comments, source-oriented identifiers, disassembly or extracted algorithm accompanies the clean-room handoff. The independent implementer may read this document and `local://moon-spec.md`.

## Files

The intended fixture directory is `crates/benilla-sim/tools/fixtures/source_moon/`.

- `cases.json` is the registered authored input manifest. Its registration hash, count and independence statement are retained in provenance. It has `format`, `scope` and `cases` fields.
- `expected.jsonl` has one complete expected result object per input case, in input order. Each object has `case` and `records` fields.
- `provenance.json` binds source, builder, numerical environment, input and output identities, completion counts and the observation-equivalence proof.

Each case has an integer `case` identity and a string `kind`. Case identities are consecutive from zero and cannot be renumbered after comparison. The data-format version is independent of the native save format.

## Input kinds

| Kind | Additional fields | Meaning |
| --- | --- | --- |
| `event` | `zone`, `epoch`, `clocks` | Calculate the full event boundary from a signed Unix-second input and four independent signed Unix-second nested clock observations. |
| `next_date` | `zone`, `civil`, `clocks` | Calculate the following new-moon civil date. `civil` is year, month, day, hour, minute. The two clock observations are independent. The month follows the mathematical service's one-based convention, including source-reached month zero. |
| `candidate` | `cycle` | Evaluate a single integer new-moon cycle index. No clock observations. |
| `civil_to_day` | `civil` | Convert year, month, day to an integer Julian day. No clock observations. |
| `day_to_civil` | `day` | Convert an integer Julian day to year, month, day. No clock observations. |

A missing `zone` on a pure arithmetic case means the oracle's UTC test environment. It does not add timezone operations to that case. Input integers are exact signed values, not decimal renderings of floating-point numbers. The manifest's clock arrays are authored controls, not expected computed outputs.

## Expected record representation

Each `records` element is an array. Its first item is the operation label; following items are the operation's input and output fields listed below. Record order is mandatory. The public representation omits the private transport's repeated case identity and sequence number. The array position is the sequence number.

Floating-point words are lowercase strings of exactly 16 hexadecimal digits. They encode the complete unsigned 64-bit binary64 word, including the sign bit. They are not numbers to parse as decimal and are not truncated significands. Integer JSON fields contain exact signed values of the widths documented in the mathematical specification. A comparison must reject a noninteger or out-of-range integer rather than round it.

### Case and final-result records

| Operation | Remaining items |
| --- | --- |
| `begin` | Input kind. Exactly one, first in the case. |
| `result` | Input kind, then its final payload. Exactly one per case. |
| `complete` | Number of consumed nested clock observations. Exactly one, last in the case. |

The `result` record has exactly three items. Its payload is an array for `event`, `next_date`, `candidate` and `day_to_civil`. Those arrays contain, respectively, start/end epoch seconds; year/month/day/hour/minute; integer Julian day/fractional-day word/full Julian-date word; and year/month/day. The `civil_to_day` payload is a single integer Julian day, not an array.

A process failure, missing completion marker, surplus result or reordered case is a refusal, not a shortened successful fixture.

### Clock and calendar records

| Operation | Remaining items |
| --- | --- |
| `time` | Returned signed Unix seconds from the next declared authored clock observation. |
| `local` | Input epoch seconds, output calendar fields, output offset seconds, output timezone abbreviation. |
| `make` | Input calendar fields consumed by the conversion, output epoch seconds, output normalised calendar fields, output offset seconds, output timezone abbreviation. |

The seven input calendar fields are second, minute, hour, day of month, zero-based month, year minus 1900 and daylight-saving indicator. The nine output calendar fields have those seven values followed by weekday and zero-based day of year. Sunday is weekday zero. Offset seconds and abbreviation are separate output fields.

Only the semantic input fields consumed by the pinned calendar conversion are recorded for `make`. Uninitialised padding and initially unwritten, unused fields are not invented as authored inputs. The complete ordered stream still includes every conversion call, including conversions whose result later contributes no arithmetic value.

All 204 event cases measured four `time`, ten `local` and six `make` calls. The earlier prediction of nine `local` calls omitted the event-input local conversion at `src/game/GameEvents/GameEventMgr.cpp:1213`, which precedes the second event-input conversion at `:1277`. All 960 `next_date` cases measured two `time`, three `local` and one `make` call. No call was removed to obtain these counts.

The returned −1 epoch value is a signed numerical observation. Consumers must not invent a failure policy merely from that value. The oracle and its receipt must distinguish actual process or conversion failures.

### Numerical service records

| Operation | Remaining items |
| --- | --- |
| `candidate_begin` | Signed cycle index, phase selector. The required selector is zero. |
| `sine` | Binary64 input word, binary64 result word, in actual observed call order. |
| `candidate_end` | Integer Julian day, fractional-day word, full Julian-date word. |
| `civil_day` | Input year/month/day array, output integer Julian day. |
| `day_civil` | Input integer Julian day, output year/month/day array. |
| `next_begin` | Input year/month/day/hour/minute array, phase selector zero. |
| `next_end` | Output year/month/day/hour/minute array. |
| `summer_day_begin` | Input integer Julian day for the daylight-saving probe. |
| `summer_day_end` | Probe result, zero or one. |
| `summer_civil_begin` | Input year/month/day array for the daylight-saving probe. |
| `summer_civil_end` | Probe result, zero or one. |

Every candidate interval must contain exactly two `sine` records. Candidate evaluations can repeat. Neither repeated candidates nor repeated sine operands may be deduplicated. The candidate's whole day and fractional day remain distinct even if their rounded sum has the same output word as another decomposition.

The oracle measured solar-anomaly sine first and lunar-anomaly sine second in all 13,213 candidate evaluations. Both executables' 26,426 sine observations matched exactly in order. Private return-address observations matched actual sine-call instructions. This binds the registered component corpus, not historical F1 dispatch.

## What constitutes an observation proof

The C++ oracle keeps an original-flags direct executable and a separately observed shared-library executable. The original lunar translation unit remains unmodified in both. The shared variant adds position-independent code only to permit symbol-boundary observations. Its extra mathematical-boundary records are accepted only when removing those additional records produces exactly the direct executable's complete ordered `begin`, `time`, `local`, `make`, `sine`, `result` and `complete` stream for every registered case.

That equality is a finite component-instrumentation proof, not a general claim about changed compiler flags and not historical execution identity. The receipt must say whether it passed and how many cases were checked. A failing projection refuses the corpus. Private call-site observations and disassembly prove the direct executable's actual sine-call ordering; source expression order alone does not prove it. No assembly is supplied to the clean-room implementer.

The published provenance is required to bind both executable hashes and the hashes of the comparison streams and compiled source units. It must also distinguish the original compiler flags from the shared observer's additional position-independent option. If a different observation technique is needed, the receipt and this section must describe that actual technique before publication.

## Strict comparison and the sine boundary

Compare equal case counts, equal record counts, exact operation labels, every integer and every floating word in order. No tolerance, filtering, reordering, resynchronisation or final-date-only comparison is permitted. Calendar input values and sine output values are not authorised native execution inputs merely because they appear in this expected corpus.

Historical glibc 2.36 binary64 sine dispatch remains refused until independently bound by the F1 owner. A unit-level arithmetic test using recorded sine responses must check each requested operand and label the result as conditional component evidence. Such a test does not implement sine, prove historical dispatch or authorise gameplay. The expected sine results must never become a silent production fallback.

## Status

The completed corpus contains 11,192 cases: 10,001 candidates, 960 next dates, 204 events, 16 civil-to-day conversions and 11 day-to-civil conversions. Every complete common stream matched between the two executables. The expected-data SHA-256 is `7584fa394b751b3dba8c43e5b943d5ced97b317f8ec2ba9463d18c3482267aba`. `provenance.json` records the executed checks, measured counts, call-order proofs and environment. No Rust comparison, historical sine binding or F1 acceptance is claimed.
