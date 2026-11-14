# Ordinary explicit-unit spell range

`spell_range::check_bits` ports CMaNGOS Classic revision
`8ec338a1704e7dcb1c0213eb7ed58f9231ade40f`, `Spell.cpp:6157–6278`
(`GetMinMaxRange`, the explicit-unit part of `CheckRange`) and
`Object.cpp:1252–1281` (`GetDistance` with `is3D=true`, `DIST_CALC_NONE`).
The source is GPL-2.0-or-later, copyright CMaNGOS contributors; this port is
GPL-3.0-or-later. No client assets or stock spell rows are included.

The caller must establish an explicit, distinct, ordinary unit target and zero
range flags, with no ranged-weapon modifier, spell-range modifier, moving-player
leeway, channel lifecycle, next-melee-swing shortcut or instant release bypass.
The [runtime host](../benilla-world-runtime/SPELL-RANGE.md) proves these
exclusions with its existing bounded source catalog. This pure function does not
certify a spell, source capture, phase, object identity or world admission.

The software IEEE binary32 mechanism (`rustc_apfloat`, round-to-nearest,
ties-to-even) performs source operations in order:

1. Add zero melee range to the row minimum; add caster and target combat reaches
2. Add combined reach to a strictly positive minimum and to the maximum
3. At non-strict release only, add `min(3.f, maximum * 0.1f)` to that maximum
4. Subtract each native XYZ coordinate, square each delta separately, add X²+Y²,
   then add Z²; there is no square root or bounding-radius adjustment
5. Compare distance squared strictly above maximum², then strictly below a
   nonzero minimum²; equal endpoints pass

Each operation rounds separately; there is no host-FPU dependence, reassociation
or FMA. Subnormal inputs and intermediates and underflow to signed/positive zero
retain source IEEE behavior. No `Real`-typed intermediate clamps or rejects them.
The raw helper permits finite coordinates inclusive ±32760, nonnegative ranges
through binary32 1,000,000 with minimum ≤ maximum, and nonnegative reaches through
binary32 1e12. These bounds preclude overflow even after squaring. Nonfinite,
over-bound and negative scalar input is an explicit error, not a gameplay
failure. Runtime `Point`, `Real` and `SpellFloat` inputs retain their stricter
normal-or-zero constructors and existing profile validation.

The outcome carries minimum, effective maximum, squared-distance bits and the
three-way range result for exact differential checks. In source, successful
`CheckRange` writes `m_maxRange`; a failed check retains its previous value.
The helper reports the effective maximum but owns no spell lifecycle.

`POLICY` and LF-normalized source hashes identify the numeric mechanism.
The exact-source [oracle](../benilla-world-runtime/tools/generate_spell_range_fixture.py)
compares input/output bits, boundary neighbors, positive/zero minimum, reach
rounding, 3D distances, release tolerance and subnormal arithmetic; observations
outside these bounds do not establish admitted-domain parity.
