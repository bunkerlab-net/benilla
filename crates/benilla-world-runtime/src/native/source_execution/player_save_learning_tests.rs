// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::source_execution::player::player_save_tests::{
    payload, prepared_operands, unsigned, Operand,
};
use crate::native::source_execution::player::tests::construct;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

fn fixture() -> (SourcePlayer, SourceObjects, SourceUnits) {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let player = construct(&mut objects, &mut units);
    (player, objects, units)
}

#[test]
fn changed_dependent_spell_checks_delete_before_insert() {
    let (mut player, _, _) = fixture();
    let mut dependent = SourcePlayerSpell {
        state: SourcePlayerSpellState::New,
        active: true,
        dependent: true,
        disabled: false,
    };
    player.spells.insert(133, dependent);
    // Player.cpp:16267-16269. New dependent spells never write a row.
    player.save_spells().unwrap();
    dependent.state = SourcePlayerSpellState::Changed;
    player.spells.insert(133, dependent);
    // Player.cpp:16264-16265. The delete does not check the dependent flag.
    assert_eq!(
        player.save_spells().unwrap_err().id,
        "execution.player_save.Player_16265.spell_delete_owner"
    );
    dependent.state = SourcePlayerSpellState::Removed;
    player.spells.insert(133, dependent);
    assert_eq!(
        player.save_spells().unwrap_err().id,
        "execution.player_save.Player_16265.spell_delete_owner"
    );
    dependent.state = SourcePlayerSpellState::New;
    dependent.dependent = false;
    player.spells.insert(133, dependent);
    assert_eq!(
        player.save_spells().unwrap_err().id,
        "execution.player_save.Player_16269.spell_insert_owner"
    );
}

fn capture(
    rows: &mut Vec<RowOperation>,
) -> impl FnMut(SourcePlayerSaveObservation) -> Result<()> + '_ {
    |observation| {
        let SourcePlayerSaveObservation::Row(row) = observation else {
            panic!("save helpers do not start a transaction");
        };
        rows.push(row);
        Ok(())
    }
}

struct MapClock(Option<i64>);

impl SpellCooldownClock for MapClock {
    fn map_clock(&mut self) -> Result<i64> {
        Ok(self.0.take().expect("one map-clock read"))
    }

    fn world_clock(&mut self) -> Result<i64> {
        panic!("Player.cpp:3689 reads the map clock");
    }
}

#[test]
fn cooldown_delete_precedes_clock_and_filters_permanent_and_expired_entries() {
    let (player, objects, mut units) = fixture();
    let cd = &mut units.get_mut(&objects, player.object).unwrap().cooldowns;
    cd.lockouts.insert(1, 40);
    for (spell, spell_expiry, category_expiry, permanent) in [(1, 30, 29, false), (2, 40, 40, true)]
    {
        cd.spells.insert(
            spell,
            SourceCooldownData {
                spell,
                category: 1,
                spell_expiry,
                category_expiry,
                permanent,
                item: 0,
            },
        );
    }
    let mut rows = Vec::new();
    // Player.cpp:3684-3689. Failure to stage the delete must not read the clock.
    let mut clock = MapClock(None);
    let failure = player.save_spell_cooldowns(&objects, &units, 9, &mut clock, &mut |_| {
        Err(refuse("test.observe.delete"))
    });
    assert_eq!(failure.unwrap_err().id, "test.observe.delete");
    let mut clock = MapClock(Some(30));
    player
        .save_spell_cooldowns(&objects, &units, 9, &mut clock, &mut capture(&mut rows))
        .unwrap();
    assert_eq!(clock.0, None);
    assert_eq!(rows, [RowOperation::Delete(RowDelete::CooldownsByGuid(9))]);
    // Player.cpp:3701 skips only if both are expired.
    for (spell_expiry, category_expiry) in [(31, 30), (30, 31)] {
        let entry = units
            .get_mut(&objects, player.object)
            .unwrap()
            .cooldowns
            .spells
            .get_mut(&1)
            .unwrap();
        entry.spell_expiry = spell_expiry;
        entry.category_expiry = category_expiry;
        rows.clear();
        assert_eq!(
            player
                .save_spell_cooldowns(
                    &objects,
                    &units,
                    9,
                    &mut MapClock(Some(30)),
                    &mut capture(&mut rows)
                )
                .unwrap_err()
                .id,
            "execution.player_save.Player_3707.cooldown_insert_owner"
        );
        assert_eq!(rows, [RowOperation::Delete(RowDelete::CooldownsByGuid(9))]);
    }
}

#[test]
fn entered_instance_delete_uses_session_account_and_preserves_expiry_boundary() {
    let (mut player, _, _) = fixture();
    let session = SourceWorldSession::construct(u32::MAX, false);
    let mut rows = Vec::new();
    player.add_new_instance_id(12, || Ok(4000)).unwrap();
    player
        .add_new_instance_id(12, || panic!("existing timer must not be refreshed"))
        .unwrap();
    // Player.cpp:20497 expires only when strictly older than the update clock.
    player.update_new_instance_id_timers(3_604_000);
    assert_eq!(
        player
            .save_entered_instance_timers(&session, &mut capture(&mut rows))
            .unwrap_err()
            .id,
        "execution.player_save.Player_20482.instance_timer_insert_owner"
    );
    player.update_new_instance_id_timers(3_604_001);
    player
        .save_entered_instance_timers(&session, &mut capture(&mut rows))
        .unwrap();
    assert_eq!(
        rows,
        [
            RowOperation::Delete(RowDelete::EnteredInstancesByAccount(u32::MAX)),
            RowOperation::Delete(RowDelete::EnteredInstancesByAccount(u32::MAX))
        ]
    );
}

#[test]
fn tutorial_state_only_changes_on_a_different_live_word() {
    let mut session = SourceWorldSession::construct(23, false);
    let mut rows = Vec::new();
    session
        .save_tutorials_data(&mut capture(&mut rows))
        .unwrap();
    assert!(rows.is_empty());
    session.bind_tutorials(None);
    session.set_tutorial_word(7, 1).unwrap();
    session
        .save_tutorials_data(&mut capture(&mut rows))
        .unwrap();
    let mut words = [0; 8];
    words[7] = 1;
    assert_eq!(
        rows,
        [RowOperation::Insert(RowInsert::Tutorial(TutorialRow {
            account: 23,
            words
        }))]
    );
    session.bind_tutorials(Some(words));
    session.set_tutorial_word(7, 1).unwrap();
    session
        .save_tutorials_data(&mut capture(&mut rows))
        .unwrap();
    assert_eq!(rows.len(), 1);
    session.set_tutorial_word(7, 2).unwrap();
    assert_eq!(
        session.set_tutorial_word(8, 0).unwrap_err().id,
        "execution.session.WorldSession_359.tutorial_index_undefined"
    );
    // An observer refusal leaves the live change pending.
    assert_eq!(
        session
            .save_tutorials_data(&mut |_| Err(refuse("test.observe.tutorial")))
            .unwrap_err()
            .id,
        "test.observe.tutorial"
    );
    session
        .save_tutorials_data(&mut capture(&mut rows))
        .unwrap();
    words[7] = 2;
    assert_eq!(
        rows[1],
        RowOperation::Update(RowUpdate::Tutorial(TutorialRow { account: 23, words }))
    );
}

#[test]
fn authored_deleted_skill_saves_in_hash_order_before_forgotten_skill_stop() {
    let (mut player, objects, _) = fixture();
    for id in [6, 8, 95] {
        player.bind_loaded_skill(&objects, [id, 0], [1, 5]).unwrap();
    }
    player.update_skill(&objects, 6, 2).unwrap();
    // No ported caller produces this state until Player.cpp:5388 removal lands.
    player.skill_status.get_mut(8).unwrap().state = SkillState::Deleted;
    player.forgotten_skills.insert(162, 1);
    let mut rows = Vec::new();
    player.save_skills(4, &mut capture(&mut rows)).unwrap();
    assert_eq!(
        rows,
        [
            RowOperation::Delete(RowDelete::Skill { guid: 4, skill: 8 }),
            RowOperation::Update(RowUpdate::Skill(SkillRow {
                guid: 4,
                skill: 6,
                value: 3,
                max: 5
            })),
        ]
    );
    player.forgotten_skills.insert(162, 2);
    let mut again = Vec::new();
    assert_eq!(
        player
            .save_skills(4, &mut capture(&mut again))
            .unwrap_err()
            .id,
        "execution.player_save.Player_16246.forgotten_skill_owner"
    );
    assert_eq!(again, rows);

    // C:p1580-1598 resolved all ten written slots. A malformed live index refuses
    // at the source value read before the row can enter a publishable batch.
    player.skill_status.get_mut(6).unwrap().slot = usize::MAX;
    let mut invalid = Vec::new();
    assert_eq!(
        player
            .save_skills(4, &mut capture(&mut invalid))
            .unwrap_err()
            .id,
        "execution.player_save.Player_16211.skill_slot_undefined"
    );
    assert_eq!(invalid, rows[..1]);
}

#[test]
fn actions_save_changed_and_deleted_buttons_in_numeric_order() {
    let (mut player, _, _) = fixture();
    for (button, state, packed_data) in [
        (9, SourcePlayerActionState::Unchanged, 20),
        (6, SourcePlayerActionState::Deleted, 3),
        (2, SourcePlayerActionState::Changed, 0x80ab_cdef),
    ] {
        player
            .action_buttons
            .insert(button, SourcePlayerAction { state, packed_data });
    }
    let mut rows = Vec::new();
    player.save_actions(4, &mut capture(&mut rows)).unwrap();
    assert_eq!(
        rows,
        [
            RowOperation::Update(RowUpdate::Action(ActionRow {
                guid: 4,
                button: 2,
                action: 0x00ab_cdef,
                kind: 0x80,
            })),
            RowOperation::Delete(RowDelete::Action { guid: 4, button: 6 }),
        ]
    );
}

#[test]
fn aura_delete_filters_live_holders_and_effects_before_insert_stop() {
    let (player, mut objects, mut units) = fixture();
    let other = construct(&mut objects, &mut units);
    let mut spells = SourceSpellStores::default();
    let target = player.object();
    let auras = &mut units.get_mut(&objects, target).unwrap().auras;
    for t in [
        SpellTemplate {
            id: 10,
            attributes: 0x40,
            ..Default::default()
        },
        SpellTemplate {
            id: 11,
            attributesex: 0x04,
            ..Default::default()
        },
        SpellTemplate {
            id: 12,
            mechanic: 5,
            ..Default::default()
        },
        SpellTemplate {
            id: 13,
            aurainterruptflags: 0x0008_0000,
            ..Default::default()
        },
        SpellTemplate {
            id: 14,
            ..Default::default()
        },
        SpellTemplate {
            id: 1386,
            ..Default::default()
        },
    ] {
        let mut holder =
            SourceAuraHolder::new(&t, target, 0, 0, if t.id == 1386 { -1 } else { 10 }).unwrap();
        let mut effect = SourceAura::self_spell_effect(&t, 0, 3, 0, 0, 1, 0).unwrap();
        if t.id == 14 {
            effect.persistent = true;
            let mut area = effect.clone();
            area.persistent = false;
            area.area = Some(SourceAreaAura {
                kind: SourceAreaAuraKind::Party,
                radius_bits: 0,
                original_rank_spell: None,
            });
            holder.caster = other.object();
            holder.effects[1] = Some(area);
        }
        holder.effects[0] = Some(effect);
        auras.store_aura_holder(t.id, &mut Some(holder)).unwrap();
        spells.insert_template(t).unwrap();
    }
    let mut rows = Vec::new();
    // SpellAuras.cpp:666-674,5868-5895. Each holder or effect is excluded.
    player
        .save_auras(&objects, &units, &spells, 9, &mut capture(&mut rows))
        .unwrap();
    assert_eq!(rows, [RowOperation::Delete(RowDelete::AurasByGuid(9))]);
    let auras = &mut units.get_mut(&objects, target).unwrap().auras;
    let area = auras
        .aura_holders()
        .find(|(id, _)| id.spell() == 14)
        .unwrap()
        .0;
    auras.aura_holder_mut(area).unwrap().caster = target;
    rows.clear();
    // A self-cast area effect reaches Player.cpp:15904.
    assert_eq!(
        player
            .save_auras(&objects, &units, &spells, 9, &mut capture(&mut rows))
            .unwrap_err()
            .id,
        "execution.player_save.Player_15904.aura_insert_owner"
    );
    assert_eq!(rows, [RowOperation::Delete(RowDelete::AurasByGuid(9))]);
}

fn proxy(root: &Path) -> Vec<Value> {
    BufReader::new(File::open(root.join("proxy-requests.jsonl")).unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect()
}

fn sql_result(root: &Path, row: &Value) -> Vec<Vec<u8>> {
    assert_eq!(row["kind"], "response");
    let stream =
        fs::read(root.join(format!("connection-{}-responses.bin", row["connection"]))).unwrap();
    let start = usize::try_from(row["wire_offset"].as_u64().unwrap()).unwrap();
    let size = usize::try_from(row["bytes"].as_u64().unwrap()).unwrap();
    let mut input = &stream[start..start + size];
    let mut rows = Vec::new();
    let columns = usize::from(input[4]);
    let mut packet = 0;
    while !input.is_empty() {
        let size = u32::from_le_bytes([input[0], input[1], input[2], 0]) as usize;
        let body = &input[4..4 + size];
        if packet > columns + 1 && body[0] != 0xfe {
            rows.push(body.to_vec());
        }
        input = &input[4 + size..];
        packet += 1;
    }
    rows
}

fn digits<const N: usize>(mut row: &[u8]) -> [u32; N] {
    let words = std::array::from_fn(|_| {
        let len = usize::from(row[0]);
        let word = std::str::from_utf8(&row[1..=len]).unwrap().parse().unwrap();
        row = &row[len + 1..];
        word
    });
    assert!(row.is_empty());
    words
}

fn assert_prepared(proxy: &[Value], prepare: usize, execute: usize, sql: &str, binds: &[Operand]) {
    let statement = payload(&proxy[prepare - 1]);
    assert_eq!(statement[0], 22, "p{prepare}");
    assert_eq!(statement[1..], *sql.as_bytes(), "p{prepare}");
    let id = proxy[prepare]["native_statement"].as_u64().unwrap() as u32;
    let actual = prepared_operands(&payload(&proxy[execute - 1]), id, binds.len());
    assert_eq!(actual, binds, "p{execute}");
}

fn u32_bind(value: u32) -> Operand {
    unsigned(3, value.to_le_bytes())
}

fn u16_bind(value: u16) -> Operand {
    unsigned(2, value.to_le_bytes())
}

fn assert_filtered_deletes(proxy: &[Value], sites: [usize; 5], rows: [&RowOperation; 3]) {
    let [cooldown_prepare, cooldown, aura_prepare, aura, instance] = sites;
    let [RowOperation::Delete(RowDelete::CooldownsByGuid(guid)), RowOperation::Delete(RowDelete::AurasByGuid(aura_guid)), RowOperation::Delete(RowDelete::EnteredInstancesByAccount(account))] =
        rows
    else {
        panic!("wrong delete sequence: {rows:?}");
    };
    assert_prepared(
        proxy,
        cooldown_prepare,
        cooldown,
        "DELETE FROM character_spell_cooldown WHERE guid = ?",
        &[u32_bind(*guid)],
    );
    assert_prepared(
        proxy,
        aura_prepare,
        aura,
        "DELETE FROM character_aura WHERE guid = ?",
        &[u32_bind(*aura_guid)],
    );
    assert_eq!(
        payload(&proxy[instance - 1]),
        format!("\x03DELETE FROM account_instances_entered WHERE AccountId = '{account}'")
            .into_bytes(),
        "p{instance}"
    );
}

fn project(
    player: &SourcePlayer,
    live: &PlayerLearning<'_, '_>,
    session: &SourceWorldSession,
    guid: u32,
    now: i64,
) -> Vec<RowOperation> {
    let mut rows = Vec::new();
    {
        let mut observe = capture(&mut rows);
        // Player.cpp:15769-15774,15777. Call only the helpers in this slice.
        player.save_spells().unwrap();
        player
            .save_spell_cooldowns(
                live.objects,
                live.units,
                guid,
                &mut MapClock(Some(now)),
                &mut observe,
            )
            .unwrap();
        player.save_actions(guid, &mut observe).unwrap();
        player
            .save_auras(live.objects, live.units, live.spells, guid, &mut observe)
            .unwrap();
        player.save_skills(guid, &mut observe).unwrap();
        player
            .save_entered_instance_timers(session, &mut observe)
            .unwrap();
        session.save_tutorials_data(&mut observe).unwrap();
    }
    rows
}

pub(in super::super) fn assert_create_learning_recording(
    root: &Path,
    guid: u32,
    rows: &[RowOperation],
) -> TutorialRow {
    let p = proxy(root);
    // C:p1481,1491. Auth loaded an empty tutorial table before Create.
    assert!(sql_result(root, &p[1491 - 1]).is_empty());
    assert_eq!(payload(&p[1502 - 1]), b"\x03START TRANSACTION");
    assert_prepared(
        &p,
        1504,
        1509,
        "DELETE FROM characters WHERE guid = ?",
        &[u32_bind(guid)],
    );
    assert_eq!(payload(&p[1607 - 1]), b"\x03COMMIT");
    assert_filtered_deletes(
        &p,
        [1554, 1557, 1572, 1575, 1600],
        [&rows[0], &rows[6], &rows[17]],
    );
    for (i, op) in rows[1..6].iter().enumerate() {
        let RowOperation::Insert(RowInsert::Action(row)) = op else {
            panic!("unexpected button row {op:?}");
        };
        assert_prepared(
            &p,
            1559,
            1562 + i * 2,
            "INSERT INTO character_action (guid,button,action,type) VALUES (?, ?, ?, ?)",
            &[
                u32_bind(row.guid),
                u32_bind(u32::from(row.button)),
                u32_bind(row.action),
                u32_bind(u32::from(row.kind)),
            ],
        );
    }
    for (i, op) in rows[7..17].iter().enumerate() {
        let RowOperation::Insert(RowInsert::Skill(row)) = op else {
            panic!("unexpected skill row {op:?}");
        };
        assert_prepared(
            &p,
            1577,
            1580 + i * 2,
            "INSERT INTO character_skills (guid, skill, value, max) VALUES (?, ?, ?, ?)",
            &[
                u32_bind(row.guid),
                u32_bind(row.skill),
                u16_bind(row.value),
                u16_bind(row.max),
            ],
        );
    }
    let RowOperation::Insert(RowInsert::Tutorial(tutorial)) = rows[18] else {
        panic!("unexpected tutorial row {:?}", rows[18]);
    };
    let binds = std::iter::once(u32_bind(u32::try_from(tutorial.account).unwrap()))
        .chain(tutorial.words.into_iter().map(u32_bind))
        .collect::<Vec<_>>();
    assert_prepared(&p, 1602, 1605,
        "INSERT INTO character_tutorial (account,tut0,tut1,tut2,tut3,tut4,tut5,tut6,tut7) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)", &binds);
    tutorial
}

pub(in super::super) fn assert_logout_learning_recordings(
    trace: &Path,
    player: &SourcePlayer,
    live: &PlayerLearning<'_, '_>,
    tutorial_words: [u32; 8],
) {
    let northshire03 = trace
        .ancestors()
        .nth(5)
        .unwrap()
        .join("northshire-03/source-choice");
    assert_authored_a_logout(&northshire03.join("a"), live, player, tutorial_words);
    assert_authored_b_logout(&northshire03.join("b"), live, player);
}

fn bind_login_skills(player: &mut SourcePlayer, objects: &SourceObjects, root: &Path, row: &Value) {
    for entry in sql_result(root, row) {
        let [skill, value, max] = digits(&entry).map(|n| u16::try_from(n).unwrap());
        player
            .bind_loaded_skill(objects, [skill, 0], [value, max])
            .unwrap();
    }
    assert_eq!(player.skills().len(), 10);
}

fn assert_authored_a_logout(
    root: &Path,
    catalog: &PlayerLearning<'_, '_>,
    created: &SourcePlayer,
    tutorial: [u32; 8],
) {
    let p = proxy(root);
    // Authored fixture for unported 23c2 login. A:p1653-1654 supplies the skill slot order.
    let (mut player, objects, mut units) = fixture();
    bind_login_skills(&mut player, &objects, root, &p[1654 - 1]);
    player.race = 1;
    player.spells = created.spells.clone();
    player.action_buttons = created.action_buttons.clone();
    for action in player.action_buttons.values_mut() {
        action.state = SourcePlayerActionState::Unchanged;
    }
    let mut session = SourceWorldSession::construct(1, true);
    session.bind_tutorials(Some(tutorial));
    // Player.cpp:4810-4816,5033-5064. A config :1530 and packets :1143,2055,3294.
    player.update_skill(&objects, 95, 1).unwrap();
    player.update_skill(&objects, 95, 1).unwrap();
    let unit = units.get_mut(&objects, player.object).unwrap();
    unit.unit_class = Some(8);
    unit.power_fields = Some(SourceUnitPowerFields {
        level: 2,
        create_health: 0,
        create_mana: 0,
        auxiliary_current: None,
        auxiliary_maximum: None,
    });
    let mut live = PlayerLearning {
        objects: &objects,
        units: &mut units,
        cast: None,
        skills: catalog.skills,
        spells: catalog.spells,
        items: catalog.items,
        class: 8,
    };
    // Authored calls for GiveLevel at Player.cpp:2742,5637. A packet :2814 gives the cap updates.
    for (skill, value) in [
        (6, 10),
        (8, 10),
        (95, 3),
        (136, 1),
        (162, 1),
        (228, 1),
        (237, 10),
        (754, 10),
    ] {
        player
            .set_skill(
                &mut live,
                SourcePlayerSkill {
                    id_step: [skill, 0],
                    value_max: [value, 10],
                    bonus: [0; 2],
                },
            )
            .unwrap();
    }
    player.update_skill(&objects, 95, 1).unwrap();
    // A:p1833 supplies the save phase seconds. There are no finite live deadlines.
    let rows = project(&player, &live, &session, 1, 1_795_250_531_000);
    assert_eq!(rows.len(), 11);
    assert_filtered_deletes(
        &p,
        [1560, 1860, 1578, 1862, 1883],
        [&rows[0], &rows[1], &rows[10]],
    );
    for (i, op) in rows[2..10].iter().enumerate() {
        let RowOperation::Update(RowUpdate::Skill(row)) = op else {
            panic!("unexpected A skill row {op:?}");
        };
        assert_prepared(
            &p,
            1864,
            1867 + i * 2,
            "UPDATE character_skills SET value = ?, max = ? WHERE guid = ? AND skill = ?",
            &[
                u16_bind(row.value),
                u16_bind(row.max),
                u32_bind(row.guid),
                u32_bind(row.skill),
            ],
        );
    }
}

fn assert_authored_b_logout(root: &Path, catalog: &PlayerLearning<'_, '_>, created: &SourcePlayer) {
    let p = proxy(root);
    // Authored fixture for unported 23c2 login. B:p1484-1485 supplies unchanged skills.
    let (mut player, objects, mut units) = fixture();
    bind_login_skills(&mut player, &objects, root, &p[1485 - 1]);
    player.spells = created.spells.clone();
    player.action_buttons = created.action_buttons.clone();
    for action in player.action_buttons.values_mut() {
        action.state = SourcePlayerActionState::Unchanged;
    }
    let mut session = SourceWorldSession::construct(1, true);
    // B:p757,1491. The existing account tutorial row is clean.
    let tutorial = sql_result(root, &p[1491 - 1]);
    assert_eq!(tutorial.len(), 1);
    session.bind_tutorials(Some(digits(&tutorial[0])));
    let live = PlayerLearning {
        objects: &objects,
        units: &mut units,
        cast: None,
        skills: catalog.skills,
        spells: catalog.spells,
        items: catalog.items,
        class: 8,
    };
    // B:p1564 supplies the save phase seconds. There are no finite live deadlines.
    let rows = project(&player, &live, &session, 1, 1_795_251_238_000);
    let [cooldown, aura, timer] = rows.as_slice() else {
        panic!("unexpected B rows {rows:?}");
    };
    assert_filtered_deletes(&p, [1566, 1569, 1571, 1574, 1576], [cooldown, aura, timer]);
}
