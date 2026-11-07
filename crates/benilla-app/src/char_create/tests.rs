use super::*;

fn varying_hair_colors() -> DialCounts {
    DialCounts {
        counts: [4, 3, 3, 8, 2],
        hair_colors: vec![8, 2, 5],
    }
}

#[test]
fn hair_style_changes_clamp_color_and_color_cycles_use_current_style() {
    let counts = varying_hair_colors();
    let mut dials = [3, 2, 0, 7, 1];
    counts.cycle(&mut dials, 2, 1);
    assert_eq!(dials, [3, 2, 1, 1, 1]);
    counts.cycle(&mut dials, 3, 1);
    assert_eq!(dials[3], 0);
    counts.cycle(&mut dials, 3, -1);
    assert_eq!(dials[3], 1);
    counts.cycle(&mut dials, 2, -1);
    assert_eq!(dials[3], 1, "keep a still-valid colour");
    counts.cycle(&mut dials, 2, -1);
    assert_eq!(dials[2], 2, "style wraps backwards");
    counts.cycle(&mut dials, 3, -1);
    counts.cycle(&mut dials, 3, -1);
    assert_eq!(dials[3], 4, "colour wraps in the new style's range");
}

#[test]
fn race_or_gender_change_clamps_style_before_color() {
    let counts = varying_hair_colors();
    let mut dials = [u8::MAX; 5];
    counts.clamp(&mut dials);
    assert_eq!(dials, [3, 2, 2, 4, 1]);
    dials[2] = 1;
    counts.clamp(&mut dials);
    assert_eq!(dials[3], 1);
}

#[test]
fn repeated_randomization_uses_the_new_style_color_range() {
    let counts = varying_hair_colors();
    let mut dials = [u8::MAX; 5];
    let mut rng = 0;
    let mut seen = [false; 3];
    let mut seen_high_color = false;
    for _ in 0..1024 {
        counts.randomize(&mut dials, &mut rng);
        for (dial, value) in dials.iter().enumerate() {
            assert!(*value < counts.get(dial, dials[2]));
        }
        seen[usize::from(dials[2])] = true;
        seen_high_color |= dials[2] == 0 && dials[3] > 1;
    }
    assert!(seen.into_iter().all(|style| style));
    assert!(
        seen_high_color,
        "do not restrict every style to the smallest palette"
    );
}

#[test]
fn missing_catalog_has_single_zero_choice_after_all_operations() {
    let mut sel = CreateSelection {
        race: 1,
        dials: [u8::MAX; 5],
        ..Default::default()
    };
    sel.clamp(None);
    assert_eq!(sel.dials, [0; 5]);
    for dial in 0..5 {
        cycle_dial(&mut sel, None, dial, -1);
        assert_eq!(sel.dials, [0; 5]);
    }
    randomize(&mut sel, None, &mut 0);
    assert_eq!(sel.dials, [0; 5]);
}

#[test]
fn installed_catalog_randomize_and_cycle_are_valid_for_every_race_and_gender() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::open_chain(&data).expect("open chain");
    let catalog =
        CharCreate(benilla_formats::CharCreateCatalog::load(&mut chain).expect("catalog"));
    for race in 1..=8 {
        for sex in 0..=1 {
            let mut sel = CreateSelection {
                race,
                sex,
                dials: [u8::MAX; 5],
                ..Default::default()
            };
            let counts = dial_counts(Some(&catalog), race, sex);
            sel.clamp(Some(&catalog));
            let mut rng = 0;
            for _ in 0..256 {
                randomize(&mut sel, Some(&catalog), &mut rng);
                for dial in 0..5 {
                    for dir in [-1, 1] {
                        cycle_dial(&mut sel, Some(&catalog), dial, dir);
                        for (i, value) in sel.dials.iter().enumerate() {
                            assert!(
                                *value < counts.get(i, sel.dials[2]),
                                "race {race}, sex {sex}, dial {i}: {:?}",
                                sel.dials
                            );
                        }
                    }
                }
            }
        }
    }
}
