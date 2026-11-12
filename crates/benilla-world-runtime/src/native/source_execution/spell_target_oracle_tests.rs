use super::*;

// FNV-1a over names with a zero terminator and little-endian u32 operands.
// Expected signatures came from compiling the unchanged 8ec338a1 table and
// Initialize bodies, SpellTargets.cpp:26-467, with one in-memory SQL entry.
struct Signature(u64);
impl Signature {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    fn byte(&mut self, value: u8) {
        self.0 = (self.0 ^ u64::from(value)).wrapping_mul(0x100000001b3);
    }
    fn word(&mut self, value: u32) {
        for b in value.to_le_bytes() {
            self.byte(b);
        }
    }
    fn name(&mut self, value: &str) {
        for b in value.bytes().chain([0]) {
            self.byte(b);
        }
    }
    fn data(&mut self, d: &SpellTargetingData) {
        for i in 0..3 {
            self.word(d.implicit_type[i] as u32);
            for right in 0..2 {
                self.word(u32::from(d.target_mask[i][right]));
                self.word(u32::from(d.ignored_targets[i][right]));
                self.word(d.filtering_scheme[i][right] as u32);
            }
        }
    }
}

#[test]
fn all_static_table_operands_match_compiled_source_signature() {
    // SpellTargets.cpp:31-231, defaults :26-28 and SpellTargets.h:40.
    let mut h = Signature::new();
    for t in SPELL_TARGET_INFO_TABLE {
        h.name(t.name);
        h.word(t.target_type as u32);
        h.word(t.filter as u32);
        h.word(t.enumerator as u32);
        h.word(t.los as u32);
    }
    for e in SPELL_EFFECT_INFO_TABLE {
        h.name(e.name);
        h.word(e.required_target as u32);
        h.word(e.default_target);
    }
    assert_eq!(h.0, 0xf7471592dfe8a302);
}

#[test]
fn all_implicit_pairs_for_dynamic_effects_match_compiled_source() {
    // SpellTargets.cpp:262-313,430-465. 4 * 64 * 64 = 16,384 cases.
    let mut h = Signature::new();
    for effect in [3, 32, 64, 77] {
        for a in 0..64 {
            for b in 0..64 {
                let s = SpellTemplate {
                    id: 1,
                    effect1: effect,
                    effectimplicittargeta1: a,
                    effectimplicittargetb1: b,
                    ..Default::default()
                };
                h.data(&initialize_spell(&s, || Ok(&[])).unwrap());
            }
        }
    }
    assert_eq!(h.0, 0xa8dcfc1bdaca9a25);
}

#[test]
fn mixed_effects_chains_radii_and_inverse_masks_match_compiled_source() {
    // SpellTargets.cpp:241-467,508-528. 130 * 64 * 8 * 2 = 133,120 cases.
    let mut h = Signature::new();
    for effect in 0..130 {
        for a in 0..64 {
            for variant in 0..8 {
                for mixed in 0..2 {
                    let effects = std::array::from_fn::<_, 3, _>(|i| {
                        if mixed != 0 {
                            (effect + i as u32 * 37) % 130
                        } else {
                            effect
                        }
                    });
                    let b = std::array::from_fn::<_, 3, _>(|i| {
                        if variant % 2 != 0 {
                            a
                        } else {
                            (a + 17 * i as u32) % 64
                        }
                    });
                    let chain =
                        std::array::from_fn::<_, 3, _>(
                            |i| if variant & 2 != 0 { i as u32 } else { 0 },
                        );
                    let radius =
                        std::array::from_fn::<_, 3, _>(
                            |i| if variant & 4 != 0 { i as u32 } else { 0 },
                        );
                    let s = SpellTemplate {
                        id: if variant == 7 {
                            26052
                        } else if variant == 6 {
                            42005
                        } else {
                            1
                        },
                        effect1: effects[0],
                        effect2: effects[1],
                        effect3: effects[2],
                        effectimplicittargeta1: a,
                        effectimplicittargeta2: a,
                        effectimplicittargeta3: a,
                        effectimplicittargetb1: b[0],
                        effectimplicittargetb2: b[1],
                        effectimplicittargetb3: b[2],
                        effectchaintarget1: chain[0],
                        effectchaintarget2: chain[1],
                        effectchaintarget3: chain[2],
                        effectradiusindex1: radius[0],
                        effectradiusindex2: radius[1],
                        effectradiusindex3: radius[2],
                        ..Default::default()
                    };
                    let rows = [SpellScriptTarget {
                        kind: 1,
                        entry: 7,
                        inverse_effect_mask: variant,
                    }];
                    h.data(&initialize_spell(&s, || Ok(&rows)).unwrap());
                }
            }
        }
    }
    assert_eq!(h.0, 0x7b74b681bbfed9e1);
}
