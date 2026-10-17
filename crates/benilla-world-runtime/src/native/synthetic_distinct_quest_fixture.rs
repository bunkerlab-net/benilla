// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native33 two-actor quest loop, never a raw-content admission path.
//! Runtime-owned source semantics use the pinned CMaNGOS mechanisms; cell membership,
//! visibility, collision and routes remain expressly synthetic spatial evidence.
use super::{synthetic_fixture as f, synthetic_two_owner_fixture as two, *};
use benilla_sim::{
    creature_ai::movement::Check, player_progression::kill_xp, quest_lifecycle as quest,
    starter_init as init,
};

pub const HOSTILE: u64 = two::HOSTILE;
pub const GIVER: u64 = two::NEUTRAL;
pub const HOSTILE_ENTRY: u32 = 60001;
pub const GIVER_ENTRY: u32 = 96002;
pub const QUEST: u32 = 70001;
pub const ITEM: u32 = 70001;
pub const SPELL: u32 = 133;

/// Complete authored faction/flag rows, including a neutral-to-all questgiver.
pub fn interaction_rows(profile: &Profile) -> InteractionInput {
    let mut rows = synthetic_combat_fixture::source_rows(profile);
    rows.factions.push(InteractionFactionSource {
        template_id: 3,
        faction: 0,
        template_flags: 0,
        group_mask: 0,
        friend_group_mask: 0,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
        reputation_list_id: None,
    });
    for row in &mut rows.npcs {
        row.npc_flags = if row.entry == GIVER_ENTRY { 2 } else { 0 };
    }
    rows
}

pub fn setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    setup_with_spawns(HOSTILE, GIVER, [HOSTILE, GIVER])
}

/// Source-selected draft plus complete progression inputs for admission tests.
/// Stable actor IDs and update order are independent of each actor's role.
pub fn setup_with_spawns(
    hostile_spawn: u64,
    giver_spawn: u64,
    owner_order: [u64; 2],
) -> (Profile, init::StarterInput, QuestProgressionInput) {
    let mut profile = two::profile_with_spawns(hostile_spawn, giver_spawn, owner_order);
    let (_, mut initial, mut progression) = synthetic_loot_fixture::setup();
    for npc in &mut profile.npcs {
        npc.ai.corpse_decay_ms = 60_000;
        if npc.ai.creature_id == hostile_spawn {
            npc.max_health = 10;
            npc.name = "Authored hostile quest target A".into();
        } else {
            npc.ai.spawn = f::point(-2.0);
            npc.name = "Authored neutral quest giver B".into();
        }
    }
    // B keeps the ordinary Aggressive empty-AI/script/EventAI selection and
    // combat capability of the complete two-owner fixture. Neutrality suppresses
    // proactive attacks, while the selected combat host still permits retaliation.
    profile
        .initialize_interactions(interaction_rows(&profile))
        .unwrap();

    synthetic_spell_fixture::rekey(&mut profile, &mut initial);
    // A non-durable authored shirt permits the raw spell catalog to be selected
    // on this draft before the caller installs its progression inputs.
    for item in &mut initial.items {
        item.max_durability = 0;
    }
    let source = synthetic_spell_fixture::source(&profile);
    for spell in &source.spells {
        initial
            .spells
            .iter_mut()
            .find(|row| row.id == spell.words[0])
            .unwrap()
            .words = spell.words.clone().try_into().unwrap();
        profile.source_motion.as_mut().unwrap().spell_hits.insert(
            spell.words[0],
            SpellHitPolicy {
                attributes: spell.words[6..11].try_into().unwrap(),
                implicit_target_a: spell.words[82..85].try_into().unwrap(),
                implicit_target_b: spell.words[85..88].try_into().unwrap(),
                aura_interrupt_flags: spell.words[22],
            },
        );
    }
    profile
        .initialize_player(&initial, f::float(100.0), 1)
        .unwrap();
    progression.abilities = profile
        .abilities
        .iter()
        .cloned()
        .map(|ability| {
            let cast_time = source
                .spells
                .iter()
                .find(|row| row.words[0] == ability.id)
                .map(|row| CastTimeRow {
                    id: row.cast_time.id,
                    base_ms: row.cast_time.base_ms,
                    per_level_ms: row.cast_time.per_level_ms,
                    minimum_ms: row.cast_time.minimum_ms,
                });
            AbilityRecipe { ability, cast_time }
        })
        .collect();
    // Keep the entire configured level range closed, rather than relying on
    // the short legacy fixture's three supplied stat/XP/training levels.
    progression.levels = synthetic_quest_fixture::setup_with_levels(60, 60, true, 1.0f32.to_bits())
        .2
        .levels;
    let row = &mut progression.quests[0];
    row.special_flags = quest::KILL | quest::SPEAK | quest::DELIVER;
    row.required_creature = [HOSTILE_ENTRY as i32, 0, 0, 0];
    row.required_creature_count = [1, 0, 0, 0];
    row.required_item = [ITEM, 0, 0, 0];
    row.required_item_count = [1, 0, 0, 0];
    let relation = quest::CreatureQuestRelation {
        creature: GIVER_ENTRY,
        quest: QUEST,
    };
    progression.relations = quest::QuestRelations {
        starters: vec![relation.clone()],
        finishers: vec![relation],
    };
    progression.npc_rows = [HOSTILE_ENTRY, GIVER_ENTRY]
        .into_iter()
        .map(|entry| QuestNpcRow {
            entry,
            npc_flags: if entry == GIVER_ENTRY { 2 } else { 0 },
            unit_flags: 0,
        })
        .collect();
    let kill = progression.kill.creatures[0].clone();
    progression.kill.creatures = [HOSTILE_ENTRY, GIVER_ENTRY]
        .into_iter()
        .map(|entry| KillCreatureRow {
            entry,
            reputation: kill_xp::ReputationAdmission::ProvenAbsentOnKillRow { entry },
            ..kill.clone()
        })
        .collect();

    let loot = progression.loot.as_mut().unwrap();
    let creature = loot.creatures[0].clone();
    loot.creatures = [HOSTILE_ENTRY, GIVER_ENTRY]
        .into_iter()
        .map(|entry| LootCreatureRow {
            entry,
            loot_id: if entry == HOSTILE_ENTRY { 99 } else { 0 },
            minimum_money: if entry == HOSTILE_ENTRY { 3 } else { 0 },
            maximum_money: if entry == HOSTILE_ENTRY { 3 } else { 0 },
            ..creature.clone()
        })
        .collect();
    let row = &mut loot.captures[0].rows[0];
    // Positive ordinary loot is independent of whether the quest is accepted.
    row.chance_or_quest_chance_bits = 100.0f32.to_bits();
    row.min_count_or_reference = 1;
    row.max_count = 1;
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    for item in &mut items {
        if let Some(raw) = initial.items.iter().find(|raw| raw.id == item.entry) {
            item.maximum_durability = raw.max_durability;
        }
    }
    loot.inventory = synthetic_loot_fixture::inventory_catalog(items);
    profile
        .initialize_spell_authority(&initial, source)
        .unwrap();
    profile.initialize_facing_authority().unwrap();
    (profile, initial, progression)
}

pub fn profile() -> Profile {
    profile_with_spawns(HOSTILE, GIVER, [HOSTILE, GIVER])
}

pub fn profile_with_spawns(hostile_spawn: u64, giver_spawn: u64, owner_order: [u64; 2]) -> Profile {
    let (mut profile, initial, progression) =
        setup_with_spawns(hostile_spawn, giver_spawn, owner_order);
    profile
        .initialize_quest_progression(&initial, progression)
        .unwrap();
    profile
}

pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], f::player(), 42).unwrap()
}

pub fn command(world: &NativeWorld, command: Command) -> OwnedInput {
    synthetic_quest_fixture::command(world, command)
}

/// Synthetic spatial witnesses only. Escaped gameplay semantics fail closed.
#[derive(Default)]
pub struct CompleteAuthority {
    pub base: two::CompleteAuthority,
    pub queries: Vec<AuthorityRequest>,
    pub derived: Vec<AuthorityReply>,
    pub line_of_sight: Option<Check>,
    pub impact_visibility: Option<Check>,
}

impl Authority for CompleteAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.queries.push(request.clone());
        let facts = match &request.query {
            Query::QuestStatus { .. }
            | Query::QuestInteraction { .. }
            | Query::LootInteraction { .. }
            | Query::Npc { .. }
            | Query::NativeMelee { .. }
            | Query::Melee { .. }
            | Query::Spell { .. }
            | Query::SourceSpell(
                SourceSpellQuery::Semantics(_)
                | SourceSpellQuery::Range { .. }
                | SourceSpellQuery::Facing(_),
            )
            | Query::SourceMotion(
                MotionQuery::HostileSpellRelation(_)
                | MotionQuery::VisitorSight(_)
                | MotionQuery::Visitor(_)
                | MotionQuery::Combat(
                    CombatQuery::NpcSemantics(_)
                    | CombatQuery::MeleeCombatEntry(_)
                    | CombatQuery::MeleeDamage(_)
                    | CombatQuery::MeleeFront(_),
                )
                | MotionQuery::Spatial(benilla_sim::creature_motion::Query::Bearing(_)),
            ) => {
                return Err(NativeError::AuthorityUnavailable(
                    "distinct quest fixture semantic query escaped selected source host",
                ));
            }
            Query::SourceSpell(SourceSpellQuery::LineOfSight(_)) => {
                SourceSpellFacts::LineOfSight(self.line_of_sight.unwrap_or(Check::Passed))
            }
            Query::SourceSpell(SourceSpellQuery::ImpactVisibility(_)) => {
                SourceSpellFacts::ImpactVisibility(self.impact_visibility.unwrap_or(Check::Passed))
            }
            _ => return self.base.resolve(request),
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::SourceSpell(facts),
        })
    }

    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.derived.push(reply.clone());
        self.base.observe_derived(reply)
    }
}

pub type SpatialOnly = CompleteAuthority;

pub fn spatial_only() -> CompleteAuthority {
    CompleteAuthority::default()
}
