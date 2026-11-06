# Facing numeric source fixtures

These authored numeric inputs exercise current-pose facing arithmetic. They contain
no world rows or game assets. Expected bits come from unchanged CMaNGOS bodies in
the pinned F0 GCC 12.2/glibc 2.36 image, not Rust arithmetic.

## Source profile

`tools/facing_source_hashes.sha256` checks complete LF-normalized source files.
The generator extracts `WorldObject::GetAngleAt`, `HasInArcAt`,
`MapManager::NormalizeOrientation` and `M_PI_F` unchanged. The C++ harness uses the
image's actual float `atan2` and `fmod` overloads. Static assertions check their
widths. RNE and disabled FTZ/DAZ are mandatory. The compiler disables contraction
and fast math. The private receipt records the image, commands and source hashes.

The Rust float atan2/atanf port retains glibc 2.36's Sun Microsystems 1993 notice
in `src/facing/atan2.rs`. Its hashes and limits are in `FACING.md`.

## Reproduce

From the repository root:

```sh
python3 crates/benilla-sim/tools/generate_facing_fixture.py \
  /path/to/mangos-classic \
  /path/to/glibc-2.36/sysdeps/ieee754/flt-32 \
  --evidence-dir /main/benilla-config/facing --check
mise exec -- mbx test --locked -p benilla-sim --test facing
```

The 3,351 rows retain the previous input corpus, including eight formerly hard
inputs and their neighbors. The final column marks those retained inputs, not a
branch inside the replacement algorithm. Boolean arguments/results use zero/one.
`sample-digests.tsv` covers 131,072 raw atan2 pairs and 8,192 each of normalization,
bearing and arc inputs. Each operation restarts xorshift32 at `0xa17a2f41` with
shifts 13/17/5. Python and Rust generate inputs independently and hash input/result
words little-endian. These component checks do not establish sealed F1 acceptance.
