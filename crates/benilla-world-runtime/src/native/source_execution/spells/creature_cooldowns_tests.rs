use super::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Random(Vec<[u32; 2]>);
impl CreatureRandom for Random {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        unreachable!()
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        assert_eq!(min, max);
        self.0.push([min, max]);
        Ok(min)
    }
}
struct Owner {
    kind: SourceObjectKind,
    list: Option<CreatureSpellList>,
    fallback: Option<[u32; 2]>,
    random: Random,
    calls: RefCell<Vec<&'static str>>,
    master_lookup: Option<(SourceObjects, ObjectHandle, SourceControlGuids)>,
}
impl Owner {
    fn new(list: Option<CreatureSpellList>) -> Self {
        Self {
            kind: SourceObjectKind::Creature,
            list,
            fallback: None,
            random: Random::default(),
            calls: RefCell::default(),
            master_lookup: None,
        }
    }
    fn record(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }
}
impl CreatureCooldownOwner for Owner {
    fn caster_kind(&self) -> Result<SourceObjectKind> {
        self.record("kind");
        Ok(self.kind)
    }
    fn spell_cooldown(&mut self, spell: u32) -> Result<CreatureCooldownMatch> {
        self.record("list");
        creature_spell_cooldown(self.list.as_ref(), spell, &mut self.random)
    }
    fn creature_cooldown(&mut self, _: u32) -> Result<Option<u32>> {
        self.record("fallback");
        self.fallback
            .map(|[min, max]| self.random.creature_urand(min, max))
            .transpose()
    }
    fn spell_mod_owner(&self) -> Result<Option<ObjectHandle>> {
        self.record("modifier");
        Ok(None)
    }
    fn clock(&self) -> Result<i64> {
        self.record("clock");
        Ok(100)
    }
    fn category_flags(&self, _: u32) -> Result<Option<u32>> {
        self.record("category");
        Ok(Some(0))
    }
    fn master(&mut self) -> Result<Option<ObjectHandle>> {
        self.record("master");
        let Some((objects, unit, guids)) = &self.master_lookup else {
            return Ok(None);
        };
        Ok(guids
            .get_master(objects, *unit, None, |_| unreachable!())?
            .unit())
    }
    fn is_player(&self, unit: ObjectHandle) -> Result<bool> {
        self.record("is_player");
        Ok(self.master_lookup.as_ref().unwrap().0.get(unit)?.kind == SourceObjectKind::Player)
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 42,
        category: 7,
        recoverytime: 10,
        categoryrecoverytime: 20,
        ..Default::default()
    }
}
fn list(repeat: u32, flags: u32) -> CreatureSpellList {
    let mut loaded = LoadedCreatureSpellLists::default();
    loaded.load_legacy(
        &[[9, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0]],
        &BTreeSet::from([9]),
        &BTreeMap::from([(42, 0)]),
        &BTreeMap::from([((9, 42), [repeat, repeat])]),
    );
    let mut list = loaded.lists.remove(&900).unwrap();
    list.spells.get_mut(&0).unwrap().flags = flags;
    list
}

fn bind_master(owner: &mut Owner, map_available: bool, kind: SourceObjectKind) {
    let mut objects = SourceObjects::default();
    let caster = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let master = objects.construct(1, kind).unwrap();
    let map = MapIdentity {
        map: 1,
        instance: 0,
        creation_generation: 1,
    };
    let guid = if kind == SourceObjectKind::Player {
        SourceGuid::without_entry(HighGuid::Player, 10)
    } else {
        SourceGuid::with_entry(HighGuid::Creature, 10, 10)
    };
    objects
        .assign_guid(caster, 0, SourceGuid::with_entry(HighGuid::Creature, 42, 1))
        .unwrap();
    objects.get_mut(caster).unwrap().map = Some(map);
    objects.add_to_world(caster).unwrap();
    if !map_available {
        objects.get_mut(caster).unwrap().map = None;
    }
    objects.assign_guid(master, 0, guid).unwrap();
    objects.get_mut(master).unwrap().map = Some(map);
    if kind == SourceObjectKind::Player {
        objects.register_player_lookup(master).unwrap();
    }
    objects.add_to_world(master).unwrap();
    owner.master_lookup = Some((
        objects,
        caster,
        SourceControlGuids {
            charmer: guid,
            creator: SourceGuid::default(),
        },
    ));
}

#[test]
fn creature_cooldown_found_and_category_found_select_only_the_matching_duration() {
    for flags in [0, 4] {
        let mut owner = Owner::new(Some(list(25, flags)));
        let mut cd = SourceSpellCooldowns::default();
        cd.add_creature_cooldown(&template(), 0, false, false, &mut owner)
            .unwrap();
        let entry = &cd.spells[&42];
        assert_eq!(
            (entry.spell_expiry, entry.category_expiry),
            if flags == 0 { (125, 120) } else { (110, 125) }
        );
        assert_eq!(owner.random.0, [[25, 25]]);
        assert_eq!(
            *owner.calls.borrow(),
            ["list", "modifier", "clock", "category", "master"]
        );
    }
}

#[test]
fn creature_cooldown_fallback_hit_keeps_the_not_found_result_quirk() {
    let mut owner = Owner::new(None);
    owner.fallback = Some([37, 37]);
    let mut cd = SourceSpellCooldowns::default();
    cd.add_creature_cooldown(&template(), 0, false, false, &mut owner)
        .unwrap();
    assert_eq!(
        (cd.spells[&42].spell_expiry, cd.spells[&42].category_expiry),
        (110, 120)
    );
    assert_eq!(owner.random.0, [[37, 37]]);
    assert_eq!(
        *owner.calls.borrow(),
        ["list", "fallback", "modifier", "clock", "category", "master"]
    );
}

#[test]
fn creature_cooldown_missing_fallback_and_zero_results_write_nothing() {
    let t = SpellTemplate {
        recoverytime: 0,
        categoryrecoverytime: 0,
        ..template()
    };
    for matched in [false, true] {
        let mut owner = Owner::new(matched.then(|| list(0, 0)));
        bind_master(&mut owner, false, SourceObjectKind::Creature);
        let mut cd = SourceSpellCooldowns::default();
        cd.add_creature_cooldown(&t, 0, false, false, &mut owner)
            .unwrap();
        assert!(cd.is_empty());
        assert_eq!(owner.random.0.len(), usize::from(matched));
        assert_eq!(
            *owner.calls.borrow(),
            if matched {
                vec!["list"]
            } else {
                vec!["list", "fallback"]
            }
        );
    }
}

#[test]
fn creature_cooldown_forced_duration_skips_scans_and_forwards_permanence() {
    let mut owner = Owner::new(Some(list(99, 4)));
    owner.fallback = Some([88, 88]);
    let mut cd = SourceSpellCooldowns::default();
    cd.add_creature_cooldown(&template(), 9, true, true, &mut owner)
        .unwrap();
    assert!(owner.random.0.is_empty());
    assert_eq!(*owner.calls.borrow(), ["modifier", "clock", "master"]);
    assert_eq!(cd.spells[&42].spell_expiry, 109);
    assert_eq!(cd.spells[&42].category_expiry, 0);
    assert!(cd.spells[&42].permanent);
    assert!(cd.categories.is_empty());
    assert!(!cd.ready_at(&template(), false, 10000).unwrap());
}

#[test]
fn creature_cooldown_master_refusal_keeps_the_completed_container_insert() {
    let mut owner = Owner::new(None);
    bind_master(&mut owner, false, SourceObjectKind::Creature);
    let mut cd = SourceSpellCooldowns::default();
    assert_eq!(
        cd.add_creature_cooldown(&template(), 0, false, false, &mut owner)
            .unwrap_err()
            .id,
        "execution.relations.Map_2371.GetUnit_map_owner"
    );
    assert_eq!(
        (cd.spells[&42].spell_expiry, cd.spells[&42].category_expiry),
        (110, 120)
    );
    assert_eq!(cd.categories[&7], 42);
    assert_eq!(
        *owner.calls.borrow(),
        ["list", "fallback", "modifier", "clock", "category", "master"]
    );
}

#[test]
fn creature_cooldown_only_player_master_reaches_the_packet_refusal() {
    for (kind, expected) in [
        (SourceObjectKind::Creature, None),
        (
            SourceObjectKind::Player,
            Some("execution.spells.Creature_3154.spell_cooldown_packet_owner"),
        ),
    ] {
        let mut owner = Owner::new(None);
        bind_master(&mut owner, true, kind);
        let mut cd = SourceSpellCooldowns::default();
        assert_eq!(
            cd.add_creature_cooldown(&template(), 0, false, false, &mut owner)
                .err()
                .map(|e| e.id),
            expected.map(str::to_owned)
        );
        assert_eq!(cd.categories[&7], 42);
        assert_eq!(cd.spells[&42].category_expiry, 120);
        assert_eq!(
            *owner.calls.borrow(),
            [
                "list",
                "fallback",
                "modifier",
                "clock",
                "category",
                "master",
                "is_player"
            ]
        );
    }
}

#[test]
fn creature_cooldown_scan_uses_map_order_and_draws_even_for_equal_zero_bounds() {
    let mut spells = list(0, 0);
    let mut second = spells.spells[&0].clone();
    second.repeat = [77, 77];
    second.flags = 4;
    spells.spells.insert(1, second);
    let mut random = Random::default();
    assert_eq!(
        creature_spell_cooldown(Some(&spells), 42, &mut random).unwrap(),
        CreatureCooldownMatch::Spell(0)
    );
    assert_eq!(random.0, [[0, 0]]);
    assert_eq!(
        creature_spell_cooldown(Some(&spells), 99, &mut random).unwrap(),
        CreatureCooldownMatch::Missing
    );
    assert_eq!(random.0.len(), 1);
}

#[test]
fn cooldown_sender_dispatches_creature_world_and_player_after_early_exits() {
    let mut owner = Owner::new(None);
    owner.kind = SourceObjectKind::Player;
    let mut cd = SourceSpellCooldowns::default();
    let mut t = template();
    t.attributes = 0x40;
    cd.send_spell_cooldown(&t, false, &mut owner).unwrap();
    t.attributes = 0;
    cd.send_spell_cooldown(&t, true, &mut owner).unwrap();
    assert!(owner.calls.borrow().is_empty());
    assert_eq!(
        cd.send_spell_cooldown(&t, false, &mut owner)
            .unwrap_err()
            .id,
        "execution.spells.Player_2199.AddCooldown_owner"
    );
    owner.kind = SourceObjectKind::GameObject;
    t.attributes = 0x0200_0000;
    owner.calls.borrow_mut().clear();
    cd.send_spell_cooldown(&t, false, &mut owner).unwrap();
    assert!(!cd.spells[&42].permanent);
    assert_eq!(*owner.calls.borrow(), ["kind", "clock", "category"]);
    let mut creature = Owner::new(None);
    let mut cd = SourceSpellCooldowns::default();
    cd.send_spell_cooldown(&t, false, &mut creature).unwrap();
    assert!(cd.spells[&42].permanent);
    assert!(creature.calls.borrow().contains(&"list"));
}

#[test]
fn creature_cooldown_index_distinguishes_unbound_from_loaded_empty() {
    let mut definitions = LoadedCreatureSpellLists::default();
    assert_eq!(
        definitions.cooldown_range(9, 42).unwrap_err().id,
        "execution.creature.ObjectMgr_GetCreatureCooldown_owner"
    );
    definitions.cooldowns = Some(BTreeMap::from([((9, 42), [37, 37])]));
    assert_eq!(definitions.cooldown_range(9, 42).unwrap(), Some([37, 37]));
    assert_eq!(definitions.cooldown_range(9, 43).unwrap(), None);
}

#[test]
fn creature_cooldown_fallback_draws_only_for_a_matching_row() {
    let definitions = CreatureSpellMapData(std::sync::Arc::new(LoadedCreatureSpellLists {
        cooldowns: Some(BTreeMap::from([((9, 42), [37, 37])])),
        ..LoadedCreatureSpellLists::default()
    }));
    let mut random = Random::default();
    assert_eq!(
        creature_cooldown(Some(&definitions), 9, 41, &mut random).unwrap(),
        None
    );
    assert!(random.0.is_empty());
    assert_eq!(
        creature_cooldown(Some(&definitions), 9, 42, &mut random).unwrap(),
        Some(37)
    );
    assert_eq!(random.0, [[37, 37]]);
}

#[test]
fn live_creature_spell_list_is_empty_at_construction_and_excluded_from_saved_bytes() {
    let mut fields = SourceUnitFields::constructed_creature();
    assert!(fields.spell_list.is_none());
    assert_eq!(fields.spell_list_disabled, None);
    let saved = serde_json::to_vec(&fields).unwrap();
    fields.spell_list = Some(list(25, 4));
    fields.spell_list_disabled = Some(true);
    assert_eq!(serde_json::to_vec(&fields).unwrap(), saved);
}
