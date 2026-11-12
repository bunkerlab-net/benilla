// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded SpellMgr definitions. No cast, aura execution, or admission is implied.
use super::super::production_compiler::VerifiedCatalog;
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum SpellDependency {
    CreatureTemplate,
    GameObjectTemplate,
    CreatureGuid,
    GameObjectGuid,
    StringId,
    Quest,
    Condition,
}
/// Queries the actual loaded external owner, never raw SQL membership.
pub trait SpellLoaderOwner {
    fn contains(&mut self, domain: SpellDependency, _id: u32) -> Result<bool> {
        Err(refuse(match domain {
            SpellDependency::CreatureTemplate => "execution.spells.creature_template_owner",
            SpellDependency::GameObjectTemplate => "execution.spells.gameobject_template_owner",
            SpellDependency::CreatureGuid => "execution.spells.creature_data_owner",
            SpellDependency::GameObjectGuid => "execution.spells.gameobject_data_owner",
            SpellDependency::StringId => "execution.spells.string_id_owner",
            SpellDependency::Quest => "execution.spells.quest_template_owner",
            SpellDependency::Condition => "execution.spells.condition_storage_owner",
        }))
    }
    fn skinning_loot(&mut self, _entry: u32) -> Result<u32> {
        Err(refuse("execution.spells.creature_skinning_loot_owner"))
    }
    fn script(
        &mut self,
        kind: SpellScriptKind,
        _name: &[u8],
    ) -> Result<Option<SpellScriptBinding>> {
        Err(refuse(match kind {
            SpellScriptKind::Spell => "execution.spells.spell_script_registry_owner",
            SpellScriptKind::Aura => "execution.spells.aura_script_registry_owner",
        }))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SpellRowDisposition {
    Loaded,
    Skipped(String),
    Refused(Refusal),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellRowLoad {
    pub ordinal: u64,
    pub key: u32,
    pub disposition: SpellRowDisposition,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellChain {
    pub previous: u32,
    pub first: u32,
    pub rank: u8,
    pub required: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellGroup {
    pub rule: i32,
    pub name: Box<[u8]>,
    pub spells: Vec<u32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpellStack {
    pub mask: u64,
    pub rule: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellThreat {
    pub threat: u16,
    pub multiplier: SpellFloat,
    pub ap_bonus: SpellFloat,
    pub inverse_effect_mask: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellProc {
    pub school_mask: u32,
    pub family: u32,
    pub family_masks: [u64; 3],
    pub flags: u32,
    pub extra: u32,
    pub ppm: SpellFloat,
    pub chance: SpellFloat,
    pub cooldown: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellTargetPosition {
    pub map: u32,
    pub position: [SpellFloat; 3],
    pub orientation: SpellFloat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellScriptTarget {
    pub kind: u32,
    pub entry: u32,
    pub inverse_effect_mask: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellArea {
    pub spell: u32,
    pub area: u32,
    pub quest_start: u32,
    pub quest_start_active: bool,
    pub quest_end: u32,
    pub condition: u16,
    pub aura: i32,
    pub race_mask: u32,
    pub gender: u8,
    pub autocast: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellDbcRecord {
    pub ordinal: usize,
    pub words: BTreeMap<usize, u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellScriptKind {
    Spell,
    Aura,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellScriptBinding {
    pub name: Box<[u8]>,
    pub registration: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellLearned {
    pub spell: u32,
    pub active: bool,
    pub auto_learned: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellPetAura {
    pub auras: BTreeMap<u32, u32>,
    pub remove_on_change_pet: bool,
    pub damage: i32,
}
impl SpellPetAura {
    // SpellMgr.h:1827-1835 checks the specific pet, then pet zero.
    pub fn aura(&self, pet: u32) -> u32 {
        self.auras
            .get(&pet)
            .or_else(|| self.auras.get(&0))
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SpellProcRehash {
    pub root: u32,
    pub child: u32,
    pub before: usize,
    pub after: usize,
    pub buckets_before: usize,
    pub buckets_after: usize,
    pub following_dereferences: usize,
}

/// Immutable definitions and complete negative lookups after source dispositions.
/// Refused alternatives block lookups for that key, even when another row loaded.
#[derive(Default)]
pub struct SourceSpellStores {
    templates: BTreeMap<u32, SpellTemplate>,
    catalog_sha256: Option<Sha256Id>,
    client_ids: BTreeSet<u32>,
    client_identity: Option<(Sha256Id, usize)>,
    dbc: BTreeMap<String, BTreeMap<u32, SpellDbcRecord>>,
    chains: BTreeMap<u32, SpellChain>,
    next: BTreeMap<u32, Vec<u32>>,
    groups: BTreeMap<u32, SpellGroup>,
    stacks: BTreeMap<u32, SpellStack>,
    facing: BTreeMap<u32, u32>,
    cones: BTreeMap<u32, i32>,
    threats: BTreeMap<u32, SpellThreat>,
    procs: BTreeMap<u32, SpellProc>,
    enchant_procs: BTreeMap<u32, SpellFloat>,
    elixirs: BTreeMap<u32, u8>,
    affects: BTreeMap<u32, u64>,
    learned: BTreeMap<u32, Vec<SpellLearned>>,
    skills: BTreeMap<u32, (u16, u16, u32)>,
    pet_auras: BTreeMap<u16, SpellPetAura>,
    spell_scripts: BTreeMap<u32, SpellScriptBinding>,
    aura_scripts: BTreeMap<u32, SpellScriptBinding>,
    positions: BTreeMap<u32, SpellTargetPosition>,
    targets: BTreeMap<u32, Vec<SpellScriptTarget>>,
    erased_targets: BTreeSet<u32>,
    areas: BTreeMap<u32, Vec<SpellArea>>,
    area_index: BTreeMap<u32, Vec<(u32, usize)>>,
    aura_index: BTreeMap<u32, Vec<(u32, usize)>>,
    area_index_blocks: BTreeMap<u32, Refusal>,
    aura_index_blocks: BTreeMap<u32, Refusal>,
    proc_rehashes: Vec<SpellProcRehash>,
    blocked: BTreeMap<(&'static str, u32), Refusal>,
    talent_spells: BTreeSet<u32>,
    loads: BTreeMap<&'static str, Vec<SpellRowLoad>>,
    chain_order: Vec<u32>,
}
impl SourceSpellStores {
    /// Version-one diagnostic definitions, not serialized live Spell continuation.
    pub fn inventory(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"catalog_sha256":self.catalog_sha256,"client_spell":self.client_identity,
            "templates":self.templates.len(),"chain_nodes":self.chains.len(),"chain_iteration":self.chain_order,
            "proc_entries":self.procs.len(),"threat_entries":self.threats.len(),"enchant_entries":self.enchant_procs.len(),
            "stack_entries":self.stacks.len(),"affect_entries":self.affects.len(),"learned_entries":self.learned.values().map(Vec::len).sum::<usize>(),
            "skill_entries":self.skills.len(),"pet_entries":self.pet_auras.len(),"unresolved_keys":self.blocked.len(),
            "restore":"execution.spells.f7_live_spell_restore_owner"})
    }
    pub fn validate(&self) -> Result<()> {
        require(
            self.catalog_sha256.is_some() && self.client_identity.is_some(),
            "execution.spells.catalog_binding_missing",
        )?;
        let keys = self.chain_order.iter().copied().collect::<BTreeSet<_>>();
        require(
            keys.len() == self.chain_order.len()
                && keys.len() == self.chains.len()
                && keys.iter().all(|k| self.chains.contains_key(k)),
            "execution.spells.chain_inventory",
        )?;
        Ok(())
    }
    // SpellMgr.h:2088-2096 and :2109-2115 supply these defined negative results.
    pub fn threat_multiplier(&self, id: Option<u32>) -> SpellFloat {
        id.and_then(|id| self.threats.get(&id))
            .map_or(SpellFloat(1f32.to_bits()), |t| t.multiplier)
    }
    pub fn enchant_proc_chance(&self, id: u32) -> SpellFloat {
        self.enchant_procs.get(&id).copied().unwrap_or_default()
    }
    pub fn template(&self, id: u32) -> Option<&SpellTemplate> {
        self.templates.get(&id)
    }
    pub fn templates(&self) -> &BTreeMap<u32, SpellTemplate> {
        &self.templates
    }
    pub fn client_contains(&self, id: u32) -> bool {
        self.client_ids.contains(&id)
    }
    pub fn client_identity(&self) -> Option<&(Sha256Id, usize)> {
        self.client_identity.as_ref()
    }
    pub fn dbc(&self, file: &str, id: u32) -> Result<Option<&SpellDbcRecord>> {
        Ok(self
            .dbc
            .get(file)
            .ok_or_else(|| refuse("execution.spells.dbc_not_loaded"))?
            .get(&id))
    }
    pub fn chain(&self, id: u32) -> Option<&SpellChain> {
        self.chains.get(&id)
    }
    pub fn first_rank(&self, id: u32) -> u32 {
        self.chain(id).map_or(id, |n| n.first)
    }
    pub fn next_ranks(&self, id: u32) -> &[u32] {
        self.next.get(&id).map_or(&[], Vec::as_slice)
    }
    pub fn group(&self, id: u32) -> Option<&SpellGroup> {
        self.groups.get(&id)
    }
    pub fn stack(&self, id: u32) -> Result<Option<&SpellStack>> {
        self.check_key("spell_group_spell", id)?;
        Ok(self.stacks.get(&id))
    }
    pub fn facing(&self, id: u32) -> u32 {
        self.facing.get(&id).copied().unwrap_or(0)
    }
    pub fn cone(&self, id: u32) -> Option<i32> {
        self.cones.get(&id).copied()
    }
    pub fn threat(&self, id: u32) -> Option<&SpellThreat> {
        self.threats.get(&id)
    }
    pub fn proc_event(&self, id: u32) -> Option<&SpellProc> {
        self.procs.get(&id)
    }
    pub fn enchant_proc(&self, id: u32) -> Option<SpellFloat> {
        self.enchant_procs.get(&id).copied()
    }
    pub fn target_position(&self, id: u32) -> Option<&SpellTargetPosition> {
        self.positions.get(&id)
    }
    pub fn script_targets(&self, id: u32) -> Result<&[SpellScriptTarget]> {
        self.check_key("spell_script_target", id)?;
        Ok(self.targets.get(&id).map_or(&[], Vec::as_slice))
    }
    pub fn areas(&self, id: u32) -> Result<&[SpellArea]> {
        self.check_key("spell_area", id)?;
        Ok(self.areas.get(&id).map_or(&[], Vec::as_slice))
    }
    pub fn areas_for_area(&self, id: u32) -> Result<Vec<&SpellArea>> {
        self.area_lookup(&self.area_index, &self.area_index_blocks, id)
    }
    pub fn areas_for_aura(&self, id: u32) -> Result<Vec<&SpellArea>> {
        self.area_lookup(&self.aura_index, &self.aura_index_blocks, id)
    }
    fn area_lookup(
        &self,
        index: &BTreeMap<u32, Vec<(u32, usize)>>,
        blocks: &BTreeMap<u32, Refusal>,
        id: u32,
    ) -> Result<Vec<&SpellArea>> {
        if let Some(e) = blocks.get(&id) {
            return Err(e.clone());
        }
        Ok(index
            .get(&id)
            .into_iter()
            .flatten()
            .map(|(s, i)| &self.areas[s][*i])
            .collect())
    }
    pub fn loads(&self) -> &BTreeMap<&'static str, Vec<SpellRowLoad>> {
        &self.loads
    }
    pub fn proc_rehashes(&self) -> &[SpellProcRehash] {
        &self.proc_rehashes
    }
    /// Checks table closure only, not executable effects or live cast owners.
    pub fn table_dependencies(&self, id: u32) -> Result<()> {
        for id in self.trigger_dependencies(id).keys() {
            for store in [
                "spell_group_spell",
                "spell_script_target",
                "spell_area",
                "spell_scripts",
                "spell_pet_auras",
                "spell_affect",
                "spell_proc_item_enchant",
            ] {
                self.check_key(store, *id)?;
            }
        }
        Ok(())
    }
    fn check_key(&self, store: &'static str, id: u32) -> Result<()> {
        self.blocked
            .get(&(store, id))
            .map_or(Ok(()), |e| Err(e.clone()))
    }
    fn record(
        &mut self,
        store: &'static str,
        ordinal: u64,
        key: u32,
        result: Result<Option<&str>>,
    ) {
        let disposition = match result {
            Ok(None) => SpellRowDisposition::Loaded,
            Ok(Some(s)) => SpellRowDisposition::Skipped(s.into()),
            Err(e) => {
                self.blocked.insert((store, key), e.clone());
                SpellRowDisposition::Refused(e)
            }
        };
        self.loads.entry(store).or_default().push(SpellRowLoad {
            ordinal,
            key,
            disposition,
        });
    }
    /// All three slots, including recursive alternatives and explicit missing IDs.
    /// Cycles retain an edge but do not manufacture repeated spell instances.
    pub fn trigger_dependencies(&self, root: u32) -> BTreeMap<u32, Option<[u32; 3]>> {
        let mut found = BTreeMap::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if found.contains_key(&id) {
                continue;
            }
            let triggers = self.template(id).map(|s| {
                [
                    s.effecttriggerspell1,
                    s.effecttriggerspell2,
                    s.effecttriggerspell3,
                ]
            });
            if let Some(slots) = triggers {
                pending.extend(slots.into_iter().filter(|s| *s != 0));
            }
            found.insert(id, triggers);
        }
        found
    }
    /// Builds definitions from bound F2 bytes, not the retained preparation report.
    pub fn load(catalog: &mut VerifiedCatalog, owner: &mut impl SpellLoaderOwner) -> Result<Self> {
        let mut stores = Self {
            catalog_sha256: Some(Sha256Id::parse(
                catalog.execution_identity()["catalog_sha256"]
                    .as_str()
                    .ok_or_else(|| refuse("execution.spells.catalog_binding_missing"))?,
            )?),
            ..Self::default()
        };
        catalog.execution_spell_scan("spell_template", |ordinal, v| {
            let template = SpellTemplate::decode(ordinal, v).map_err(|e| {
                super::super::production_compiler::ProductionDiagnostic { id: e.id }
            })?;
            stores
                .insert_template(template)
                .map_err(|e| super::super::production_compiler::ProductionDiagnostic { id: e.id })
        })?;
        let (sha, count, ids) = catalog.execution_client_spell_ids()?;
        stores.client_identity = Some((Sha256Id::parse(&sha)?, count));
        stores.client_ids = ids;
        for file in [
            "Map.dbc",
            "AreaTable.dbc",
            "Talent.dbc",
            "SkillLineAbility.dbc",
            "SpellCastTimes.dbc",
            "SpellRange.dbc",
            "SpellDuration.dbc",
            "SpellCategory.dbc",
            "SpellFocusObject.dbc",
            "SpellRadius.dbc",
            "SpellShapeshiftForm.dbc",
            "SpellItemEnchantment.dbc",
        ] {
            let mut view = catalog.dbc_view(file)?;
            let mut rows = BTreeMap::new();
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                let key = row["key"]
                    .as_u64()
                    .ok_or_else(|| refuse("execution.spells.dbc_layout"))?
                    as u32;
                let words = row["values"]
                    .as_array()
                    .ok_or_else(|| refuse("execution.spells.dbc_layout"))?
                    .iter()
                    .filter_map(|v| Some((v[0].as_u64()? as usize, v[2].as_u64()? as u32)))
                    .collect();
                rows.insert(key, SpellDbcRecord { ordinal, words });
            }
            stores.dbc.insert(file.into(), rows);
        }
        stores.talent_spells = stores.dbc["Talent.dbc"]
            .values()
            .flat_map(|r| (4..9).filter_map(|i| r.words.get(&i).copied()))
            .filter(|id| *id != 0)
            .collect();
        let narrow = stores
            .templates
            .iter()
            .map(|(&id, s)| {
                let e = s.effects();
                let mut w = [0; 25];
                w[0] = s.attributes;
                for i in 0..3 {
                    w[1 + i] = e[i].effect;
                    w[19 + i] = e[i].aura;
                }
                (id, w)
            })
            .collect();
        let dbc_words = |file: &str, len: usize| {
            stores.dbc[file]
                .iter()
                .map(|(&id, r)| {
                    let mut w = vec![0; len];
                    for (&i, &v) in &r.words {
                        if i < len {
                            w[i] = v;
                        }
                    }
                    (id, w)
                })
                .collect()
        };
        let chains = catalog.execution_spell_chains(
            &narrow,
            &dbc_words("Talent.dbc", 21),
            &dbc_words("SkillLineAbility.dbc", 15),
        )?;
        stores.chain_order = chains.order;
        for (id, n) in chains.nodes {
            stores.chains.insert(
                id,
                SpellChain {
                    previous: n[0],
                    first: n[1],
                    rank: n[2] as u8,
                    required: n[3],
                },
            );
        }
        for (ordinal, id, status) in chains.rows {
            stores.record(
                "spell_chain",
                ordinal,
                id,
                Ok(if status.starts_with("loaded") {
                    None
                } else {
                    Some(&status)
                }),
            );
        }
        // SpellMgr.cpp:1401-1409 returns before building the next-rank cache.
        if chains.build_next {
            stores.build_next();
        }
        for name in [
            "spell_facing",
            "spell_cone",
            "spell_target_position",
            "spell_group",
            "spell_group_spell",
            "spell_threat",
            "spell_proc_event",
            "spell_proc_item_enchant",
            "spell_script_target",
            "spell_area",
            "spell_elixir",
            "spell_affect",
            "spell_learn_spell",
            "spell_pet_auras",
        ] {
            catalog.execution_spell_scan(name, |ordinal, v| {
                stores.load_row(name, ordinal, v, owner);
                Ok(())
            })?;
            if matches!(name, "spell_threat" | "spell_proc_event") {
                stores.fill_ranks(name)?;
            }
            if name == "spell_group_spell" {
                stores.build_stacks();
            }
            if name == "spell_learn_spell" && stores.loads.get(name).is_some_and(|r| !r.is_empty())
            {
                stores.add_template_learning()?;
            }
        }
        stores.load_learn_skills()?;
        catalog.execution_spell_script_scan(|ordinal, id, name| {
            let result = stores.bind_script(id, name, owner);
            stores.record("spell_scripts", ordinal, id, result);
            Ok(())
        })?;
        Ok(stores)
    }
    pub(super) fn insert_template(&mut self, template: SpellTemplate) -> Result<()> {
        // SQLStorageImpl.h:317,394; SQLStorage.h:124-126. A wrapped extent
        // reaches an out-of-bounds index write on creation, not a source skip.
        require(
            template.id != u32::MAX,
            "execution.spells.sql_index_write_undefined",
        )?;
        self.record("spell_template", template.ordinal, template.id, Ok(None));
        self.templates.insert(template.id, template);
        Ok(())
    }
    fn build_next(&mut self) {
        for &id in &self.chain_order {
            let n = self.chains[&id];
            for p in [n.previous, n.required] {
                if p != 0 {
                    self.next.entry(p).or_default().push(id);
                }
            }
        }
    }
    fn higher(&self, root: u32) -> Result<Vec<u32>> {
        fn walk(
            s: &SourceSpellStores,
            id: u32,
            active: &mut BTreeSet<u32>,
            out: &mut Vec<u32>,
        ) -> Result<()> {
            require(
                active.insert(id),
                "execution.spells.high_rank_recursion_cycle",
            )?;
            for &child in s.next_ranks(id) {
                out.push(child);
                walk(s, child, active, out)?;
            }
            active.remove(&id);
            Ok(())
        }
        let mut out = Vec::new();
        walk(self, root, &mut BTreeSet::new(), &mut out)?;
        Ok(out)
    }
    fn load_row(
        &mut self,
        name: &'static str,
        ordinal: u64,
        v: &[serde_json::Value],
        owner: &mut impl SpellLoaderOwner,
    ) {
        let n = |i: usize| v[i].as_u64().expect("checked spell numeric layout") as u32;
        let id = n(0);
        let result = (|| -> Result<Option<&str>> {
            match name {
                "spell_cone" => {
                    self.cones.insert(id, n(1) as i32);
                }
                "spell_group" => {
                    // SpellStacking.h:32 fixes the underlying type to signed int.
                    if n(1) as i32 >= 3 {
                        return Ok(Some("invalid_rule"));
                    }
                    self.groups.entry(id).or_insert(SpellGroup {
                        rule: n(1) as i32,
                        name: decode_hex(&v[2])?,
                        spells: Vec::new(),
                    });
                }
                "spell_group_spell" => {
                    let Some(group) = self.groups.get_mut(&id) else {
                        return Ok(Some("missing_group"));
                    };
                    if !self.templates.contains_key(&n(1)) {
                        return Ok(Some("missing_spell"));
                    }
                    group.spells.push(n(1));
                }
                "spell_area" => return self.load_area(v, owner),
                "spell_pet_auras" => return self.load_pet_aura(v),
                "spell_script_target" => return self.load_script_target(v, owner),
                "spell_target_position" => {
                    if !self.dbc["Map.dbc"].contains_key(&n(1)) {
                        return Ok(Some("target_map"));
                    }
                    if (2..5).all(|i| f32::from_bits(n(i)) == 0.) {
                        return Ok(Some("target_coordinates"));
                    }
                    let Some(spell) = self.template(id) else {
                        return Ok(Some("missing_spell"));
                    };
                    if !spell
                        .effects()
                        .iter()
                        .any(|e| e.target_a == 17 || e.target_b == 17)
                    {
                        return Ok(Some("database_target"));
                    }
                    self.positions.insert(
                        id,
                        SpellTargetPosition {
                            map: n(1),
                            position: [SpellFloat(n(2)), SpellFloat(n(3)), SpellFloat(n(4))],
                            orientation: SpellFloat(n(5)),
                        },
                    );
                }
                _ => {
                    if self.template(id).is_none() {
                        return Ok(Some("missing_spell"));
                    }
                    match name {
                        "spell_facing" => {
                            self.facing.insert(id, n(1));
                        }
                        "spell_elixir" => {
                            self.elixirs.insert(id, n(1) as u8);
                        }
                        "spell_affect" => return self.load_affect(v),
                        "spell_learn_spell" => {
                            if self.template(n(1)).is_none() {
                                return Ok(Some("missing_learned_spell"));
                            }
                            if self.is_talent(n(1)) {
                                return Ok(Some("talent"));
                            }
                            self.learned.entry(id).or_default().push(SpellLearned {
                                spell: n(1),
                                active: n(2) != 0,
                                auto_learned: false,
                            });
                        }
                        "spell_threat" => {
                            if self.first_rank(id) != id && n(1) == 0 {
                                return Ok(Some("nonfirst_rank"));
                            }
                            self.threats.insert(
                                id,
                                SpellThreat {
                                    threat: n(1) as u16,
                                    multiplier: SpellFloat(n(2)),
                                    ap_bonus: SpellFloat(n(3)),
                                    inverse_effect_mask: n(4),
                                },
                            );
                        }
                        "spell_proc_event" => {
                            if self.first_rank(id) != id && f32::from_bits(n(8)) == 0. {
                                return Ok(Some("nonfirst_rank"));
                            }
                            self.procs.insert(
                                id,
                                SpellProc {
                                    school_mask: n(1),
                                    family: n(2),
                                    family_masks: [
                                        v[3].as_u64().unwrap(),
                                        v[4].as_u64().unwrap(),
                                        v[5].as_u64().unwrap(),
                                    ],
                                    flags: n(6),
                                    extra: n(7),
                                    ppm: SpellFloat(n(8)),
                                    chance: SpellFloat(n(9)),
                                    cooldown: n(10),
                                },
                            );
                        }
                        "spell_proc_item_enchant" => {
                            if self.first_rank(id) != id {
                                return Ok(Some("nonfirst_rank"));
                            }
                            let ppm = SpellFloat(n(1));
                            self.enchant_procs.insert(id, ppm);
                            for child in self.higher(id)? {
                                self.enchant_procs.insert(child, ppm);
                            }
                        }
                        _ => return Err(refuse("execution.spells.unknown_loader")),
                    }
                }
            }
            Ok(None)
        })();
        if let ("spell_area", Err(e)) = (name, &result) {
            // Retain the complete raw index operands of unresolved rows.
            if n(1) != 0 {
                self.area_index_blocks.insert(n(1), e.clone());
            }
            let aura = (n(6) as i32).unsigned_abs();
            if aura != 0 {
                self.aura_index_blocks.insert(aura, e.clone());
            }
        }
        self.record(name, ordinal, id, result);
    }
    fn build_stacks(&mut self) {
        // SpellStacking.cpp:105-113. Validate the shift only after membership loaded.
        for (&id, group) in &self.groups {
            for &spell in &group.spells {
                if !(1..=64).contains(&id) {
                    let e = refuse("execution.spells.stack_shift_undefined");
                    self.blocked.insert(("spell_group_spell", spell), e.clone());
                    for row in self
                        .loads
                        .get_mut("spell_group_spell")
                        .into_iter()
                        .flatten()
                        .filter(|r| r.key == id && r.disposition == SpellRowDisposition::Loaded)
                    {
                        row.disposition = SpellRowDisposition::Refused(e.clone());
                    }
                    continue;
                }
                let stack = self.stacks.entry(spell).or_default();
                stack.mask |= 1u64 << (id - 1);
                if stack.rule == 0 {
                    stack.rule = group.rule;
                }
            }
        }
    }
    fn fill_ranks(&mut self, name: &'static str) -> Result<()> {
        // SpellMgr.cpp:513-558,567-609,984-1001. Existing custom entries survive.
        let mut hash = super::identity_table::IdentityTable::default();
        for row in self
            .loads
            .get(name)
            .into_iter()
            .flatten()
            .filter(|r| r.disposition == SpellRowDisposition::Loaded)
        {
            hash.insert(u64::from(row.key), (), 2_000_000)?;
        }
        let roots: BTreeSet<_> = self
            .loads
            .get(name)
            .into_iter()
            .flatten()
            .filter(|r| r.disposition == SpellRowDisposition::Loaded)
            .map(|r| self.first_rank(r.key))
            .collect();
        for root in roots {
            if name == "spell_threat" && !self.threats.contains_key(&root)
                || name == "spell_proc_event" && !self.procs.contains_key(&root)
            {
                continue;
            }
            let state_buckets = hash.bucket_count();
            let higher = self.higher(root)?;
            for (index, &child) in higher.iter().enumerate() {
                if name == "spell_threat" {
                    let data = self.threats[&root];
                    self.threats.entry(child).or_insert(data);
                } else {
                    // SpellMgr.cpp:572 dereferences a stored unordered_map iterator.
                    // A preceding insertion may invalidate it; stable nodes do not
                    // make an invalidated iterator a valid source operation.
                    require(
                        state_buckets == hash.bucket_count(),
                        "execution.spells.proc_state_iterator_undefined",
                    )?;
                    let data = self.procs[&root];
                    let before = hash.len();
                    let buckets_before = hash.bucket_count();
                    if hash.insert(u64::from(child), (), 2_000_000)?
                        && buckets_before != hash.bucket_count()
                    {
                        self.proc_rehashes.push(SpellProcRehash {
                            root,
                            child,
                            before,
                            after: hash.len(),
                            buckets_before,
                            buckets_after: hash.bucket_count(),
                            following_dereferences: higher.len() - index - 1,
                        });
                    }
                    self.procs.entry(child).or_insert(data);
                }
            }
        }
        Ok(())
    }
    fn load_script_target(
        &mut self,
        v: &[serde_json::Value],
        owner: &mut impl SpellLoaderOwner,
    ) -> Result<Option<&'static str>> {
        let n = |i: usize| v[i].as_u64().unwrap() as u32;
        let id = n(0);
        let result = (|| {
            let Some(spell) = self.template(id) else {
                return Ok(Some("missing_spell"));
            };
            if !spell.effects().iter().any(|e| {
                e.effect == 27
                    || [7, 8, 38, 40, 46, 51, 52, 60].contains(&e.target_a)
                    || [7, 8, 38, 40, 46, 51, 52, 60].contains(&e.target_b)
            }) {
                return Ok(Some("script_target"));
            }
            if n(1) >= 6 {
                return Ok(Some("target_type"));
            }
            let entry = n(2);
            let domain = match n(1) {
                0 => SpellDependency::GameObjectTemplate,
                4 => SpellDependency::GameObjectGuid,
                3 => SpellDependency::CreatureGuid,
                5 => SpellDependency::StringId,
                _ => SpellDependency::CreatureTemplate,
            };
            if entry == 0 && matches!(n(1), 0 | 4) {
                return Ok(None);
            }
            if entry == 0 && matches!(n(1), 1 | 2) {
                return Ok(Some("zero_creature"));
            }
            if !owner.contains(domain, entry)? {
                return Ok(Some("missing_target"));
            }
            if matches!(n(1), 1 | 2) && id == 30427 && owner.skinning_loot(entry)? == 0 {
                return Ok(Some("missing_skinning_loot"));
            }
            Ok(None)
        })();
        // SQLMultiStorage::EraseEntry erases the entire equal range, not one row.
        match &result {
            Ok(None) => {
                if !self.erased_targets.contains(&id) {
                    self.targets.entry(id).or_default().push(SpellScriptTarget {
                        kind: n(1),
                        entry: n(2),
                        inverse_effect_mask: n(3),
                    });
                }
            }
            Ok(Some(_)) => {
                self.targets.remove(&id);
                self.erased_targets.insert(id);
            }
            Err(_) => {}
        }
        result
    }
    fn load_area(
        &mut self,
        v: &[serde_json::Value],
        owner: &mut impl SpellLoaderOwner,
    ) -> Result<Option<&'static str>> {
        let n = |i: usize| v[i].as_u64().unwrap() as u32;
        let a = SpellArea {
            spell: n(0),
            area: n(1),
            quest_start: n(2),
            quest_start_active: n(3) != 0,
            quest_end: n(4),
            condition: n(5) as u16,
            aura: n(6) as i32,
            race_mask: n(7),
            gender: n(8) as u8,
            autocast: n(9) != 0,
        };
        // SharedDefines.h:25-29 non-fixed Gender has range 0..3. Conversion precedes lookup.
        require(
            a.gender <= 3,
            "execution.spells.gender_conversion_undefined",
        )?;
        if self.template(a.spell).is_none() {
            return Ok(Some("missing_spell"));
        }
        self.check_key("spell_area", a.spell)?;
        if self.areas.get(&a.spell).into_iter().flatten().any(|b| {
            a.area == b.area
                && a.quest_start == b.quest_start
                && a.aura == b.aura
                && a.race_mask & b.race_mask != 0
                && a.gender == b.gender
        }) {
            return Ok(Some("duplicate_requirements"));
        }
        if a.area != 0 && !self.dbc["AreaTable.dbc"].contains_key(&a.area) {
            return Ok(Some("missing_area"));
        }
        if a.condition != 0 {
            if !owner.contains(SpellDependency::Condition, u32::from(a.condition))? {
                return Ok(Some("missing_condition"));
            }
        } else {
            if a.quest_start != 0 && !owner.contains(SpellDependency::Quest, a.quest_start)? {
                return Ok(Some("missing_start_quest"));
            }
            if a.quest_end != 0 {
                if !owner.contains(SpellDependency::Quest, a.quest_end)? {
                    return Ok(Some("missing_end_quest"));
                }
                if a.quest_start == a.quest_end && !a.quest_start_active {
                    return Ok(Some("same_quest"));
                }
            }
            if a.race_mask != 0 && a.race_mask & 0xff == 0 {
                return Ok(Some("race_mask"));
            }
            if a.gender > 2 {
                return Ok(Some("gender"));
            }
        }
        if a.aura != 0 {
            let aura = a
                .aura
                .checked_abs()
                .ok_or_else(|| refuse("execution.spells.area_abs_undefined"))?
                as u32;
            let Some(spell) = self.template(aura) else {
                return Ok(Some("missing_aura"));
            };
            if !spell.effects().iter().any(|e| [4, 95].contains(&e.aura)) {
                return Ok(Some("aura_type"));
            }
            if aura == a.spell {
                return Ok(Some("self_aura"));
            }
            if a.autocast && a.aura > 0 {
                // SpellMgr.cpp:2234-2268 checks both forward and reverse chains.
                if let Some(e) = self.aura_index_blocks.get(&a.spell) {
                    return Err(e.clone());
                }
                if self
                    .aura_index
                    .get(&a.spell)
                    .into_iter()
                    .flatten()
                    .any(|(s, i)| {
                        let b = self.areas[s][*i];
                        b.autocast && b.aura > 0
                    })
                {
                    return Ok(Some("autocast_chain"));
                }
                if self.areas(aura)?.iter().any(|b| b.autocast && b.aura > 0) {
                    return Ok(Some("autocast_chain"));
                }
            }
        }
        let list = self.areas.entry(a.spell).or_default();
        let i = list.len();
        list.push(a);
        if a.area != 0 {
            self.area_index
                .entry(a.area)
                .or_default()
                .push((a.spell, i));
        }
        if a.aura != 0 {
            self.aura_index
                .entry(a.aura.unsigned_abs())
                .or_default()
                .push((a.spell, i));
        }
        Ok(None)
    }
}
fn decode_hex(v: &serde_json::Value) -> Result<Box<[u8]>> {
    let s = v
        .as_str()
        .ok_or_else(|| refuse("execution.spells.text_layout"))?;
    require(s.len().is_multiple_of(2), "execution.spells.text_layout")?;
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| refuse("execution.spells.text_layout"))
        })
        .collect()
}

#[path = "spell_auxiliary.rs"]
mod auxiliary;
#[cfg(test)]
#[path = "spell_inputs_tests.rs"]
mod tests;
