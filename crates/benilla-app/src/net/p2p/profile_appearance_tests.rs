use super::*;
use benilla_ui::script::{
    GlueCall, GlueCreateCatalog, GlueCreateClass, GlueCreateCounts, GlueCreateRace, UiScript,
};
use std::rc::Rc;

fn word(index: u32, count: u32) -> u32 {
    ((u64::from(2 * index + 1) << 31) / u64::from(count)) as u32
}

// Synthetic domains deliberately exceed the former profile caps and vary colours by style.
// Actual client counts are covered separately through create_catalog and the user's DBCs.
fn chooser_catalogs() -> (GlueCreateCatalog, AvatarCatalog) {
    let mut glue = GlueCreateCatalog::default();
    let mut avatars = AvatarCatalog::default();
    for race in 1..=8 {
        glue.races.push(GlueCreateRace {
            id: race,
            name: format!("Race{race}"),
            file: format!("Race{race}"),
            faction: None,
            tokens: [String::new(), String::new(), String::new()],
        });
        let classes = benilla_profile::valid_classes(race).to_vec();
        glue.class_lists.insert(race, classes.clone());
        for &class in &classes {
            glue.classes.insert(
                class,
                GlueCreateClass {
                    name: format!("Class{class}"),
                    file: format!("Class{class}"),
                },
            );
        }
        for sex in [0, 1] {
            let ranges = [2 + race % 3, 2 + sex, 16 + race % 4, 4, 9 + sex + race % 2];
            let hair_colors: Vec<u8> = (0..ranges[2])
                .map(|style| 1 + (style + race + sex) % 4)
                .collect();
            glue.counts.insert(
                (race, sex),
                GlueCreateCounts {
                    skin: i32::from(ranges[0]),
                    face: i32::from(ranges[1]),
                    hair_styles: i32::from(ranges[2]),
                    hair_colors: hair_colors.iter().copied().map(i32::from).collect(),
                    facial_hair: i32::from(ranges[4]),
                },
            );
            glue.models.insert((race, sex));
            avatars.0.insert(
                (race, sex),
                Body {
                    display: 100 + u32::from(race),
                    faction: 1,
                    classes: classes.clone(),
                    ranges,
                    hair_colors,
                },
            );
        }
    }
    (glue, avatars)
}

fn assert_create_accepted(script: &mut UiScript, avatars: &AvatarCatalog) -> Appearance {
    script.take_glue_calls();
    script.run("CreateCharacter('Cataloglook')").unwrap();
    let calls = script.take_glue_calls();
    let [GlueCall::CreateCharacter(request)] = calls.as_slice() else {
        panic!("expected one create request: {calls:?}");
    };
    let input = NewCharacter {
        name: request.name.clone(),
        race: request.race,
        class: request.class,
        gender: request.sex,
        appearance: Appearance {
            skin: request.skin,
            face: request.face,
            hair_style: request.hair_style,
            hair_color: request.hair_color,
            facial_hair: request.facial_hair,
        },
    };
    input
        .validate()
        .expect("actual chooser request fits profile schema");
    avatars
        .validate(input.race, input.class, input.gender, input.appearance)
        .expect("actual chooser request fits avatar catalog");
    input.appearance
}

fn exercise_chooser(glue: GlueCreateCatalog, avatars: AvatarCatalog) -> (bool, bool) {
    let glue = Rc::new(glue);
    let mut high_hair = false;
    let mut high_beard = false;
    for (index, race) in glue
        .races
        .iter()
        .enumerate()
        .filter(|(_, r)| (1..=8).contains(&r.id))
    {
        for sex in [0, 1] {
            assert!(glue.models.contains(&(race.id, sex)));
            let mut script = UiScript::new_glue().unwrap();
            script.set_glue_create_catalog(Rc::clone(&glue));
            script
                .run("CreateFrame('ModelFFX', 'Create') SetCharCustomizeFrame('Create')")
                .unwrap();
            script.script_glue_create_draws(&[
                word(u32::from(sex), 2),
                word(index as u32, glue.races.len() as u32),
                0,
                0,
                0,
                0,
                0,
            ]);
            script.run("ResetCharCustomize()").unwrap();
            let look = script.glue_create_look().unwrap();
            assert_eq!((look.race, look.sex), (race.id, sex));
            for class_index in 0..glue.class_lists[&race.id].len() {
                script
                    .run(&format!("SetSelectedClass({})", class_index + 1))
                    .unwrap();
                let mut state = 0x1234_5678u32;
                for _ in 0..64 {
                    let draws = std::array::from_fn::<_, 5, _>(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        state
                    });
                    script.script_glue_create_draws(&draws);
                    script.run("RandomizeCharCustomization()").unwrap();
                    let appearance = assert_create_accepted(&mut script, &avatars);
                    high_hair |= appearance.hair_style > 15;
                    high_beard |= appearance.facial_hair > 8;
                    // Repeated manual edits must stay valid after the randomized choice.
                    for axis in 1..=5 {
                        script
                            .run(&format!("CycleCharCustomization({axis}, 1)"))
                            .unwrap();
                        assert_create_accepted(&mut script, &avatars);
                    }
                }
            }
            // A body switch must not retain an incompatible appearance from the old body.
            script.run(&format!("SetSelectedSex({})", 2 - sex)).unwrap();
            assert_create_accepted(&mut script, &avatars);
            let next = (index + 1) % glue.races.len();
            script
                .run(&format!(
                    "SetSelectedRace({}) SetSelectedClass(1)",
                    next + 1
                ))
                .unwrap();
            assert_create_accepted(&mut script, &avatars);
        }
    }
    (high_hair, high_beard)
}

#[test]
fn stock_random_cycle_and_body_switches_reach_profile_for_all_classic_bodies() {
    let (glue, avatars) = chooser_catalogs();
    assert_eq!(exercise_chooser(glue, avatars), (true, true));
}

#[test]
fn zero_or_missing_catalog_choices_are_not_synthesized_as_zero() {
    let (_, mut avatars) = chooser_catalogs();
    for axis in 0..5 {
        let original = avatars.0[&(3, 0)].ranges[axis];
        avatars.0.get_mut(&(3, 0)).unwrap().ranges[axis] = 0;
        assert!(avatars.validate(3, 1, 0, Appearance::default()).is_err());
        avatars.0.get_mut(&(3, 0)).unwrap().ranges[axis] = original;
    }
    // A hole in style-specific colour availability is not permission to use colour zero.
    avatars.0.get_mut(&(3, 0)).unwrap().hair_colors[1] = 0;
    let mut look = Appearance {
        hair_style: 1,
        ..Appearance::default()
    };
    assert!(avatars.validate(3, 1, 0, look).is_err());
    look.hair_style = 2;
    assert!(
        avatars.validate(3, 1, 0, look).is_ok(),
        "other styles remain available"
    );
    avatars.0.get_mut(&(3, 0)).unwrap().hair_colors.truncate(2);
    assert!(avatars.validate(3, 1, 0, look).is_err());
}

#[test]
fn installed_stock_random_requests_fit_actual_catalog_and_profile() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = benilla_formats::open_chain(&data).expect("open chain");
    let create = benilla_formats::CharCreateCatalog::load(&mut chain).expect("create catalog");
    let classes = benilla_formats::load_chr_classes(&mut chain).expect("classes");
    let factions = benilla_formats::load_faction_catalog(&mut chain).expect("factions");
    let creatures = crate::entities::Creatures::from_catalog(
        benilla_formats::load_creature_catalog(&mut chain).expect("creatures"),
    );
    let glue = crate::ui_script::glue_create::create_catalog(
        &create,
        Some(&classes),
        Some(&factions),
        Some(&creatures),
    );
    assert_eq!(glue.races.len(), 8);
    exercise_chooser(glue, AvatarCatalog::from_client(&create, None));
}
