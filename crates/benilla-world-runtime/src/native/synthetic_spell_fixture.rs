// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored raw-shaped spell rows and spatial witnesses, never native content.
use super::*;
use benilla_sim::starter_init as init;

pub fn hooks() -> SpellHookSources {
    SpellHookSources {
        spell_area: SpellAreaSourceRows::CompleteTable(vec![]),
        spell_script_names: SpellSourceRows::Complete(vec![]),
        conditions: SpellSourceRows::Complete(vec![]),
        spell_proc_event: SpellSourceRows::Complete(vec![]),
        spell_affect: SpellSourceRows::Complete(vec![]),
        spell_bonus_data: SpellSourceRows::Complete(vec![]),
        spell_threat: SpellSourceRows::Complete(vec![]),
        spell_elixir: SpellSourceRows::Complete(vec![]),
        spell_group_membership: SpellSourceRows::Complete(vec![]),
        spell_rank_chain: SpellSourceRows::Complete(vec![]),
    }
}
pub fn source(p: &Profile) -> SpellAuthorityInput {
    let spells = p
        .abilities
        .iter()
        .filter_map(|a| {
            let AbilityAdmission::Available(s) = &a.admission else {
                return None;
            };
            let mut w = vec![0; 173];
            w[0] = a.id;
            w[1] = a.school.into();
            w[2] = s.category;
            w[13] = 2;
            w[18] = 901;
            w[19] = s.recovery_ms;
            w[20] = s.category_recovery_ms;
            w[21] = u32::from(s.damage_pushback) * 2 + u32::from(s.movement_interrupts);
            w[32] = s.mana_cost;
            w[36] = 902;
            w[37] = s.projectile_speed.bits();
            w[58] = u32::MAX;
            w[157] = s.global_category;
            w[158] = s.global_recovery_ms;
            w[160] = 3;
            w[161] = 1;
            w[164] = 1;
            w[165] = 1;
            let mut duration = None;
            for (i, effect) in s.effects.iter().enumerate() {
                let value = match effect {
                    spell::MageEffect::SchoolDamage(v) => {
                        w[61 + i] = 2;
                        v
                    }
                    spell::MageEffect::PeriodicDamage {
                        value,
                        amplitude_ms,
                        duration_ms,
                    } => {
                        w[61 + i] = 6;
                        w[91 + i] = 3;
                        w[94 + i] = *amplitude_ms;
                        w[30] = 903;
                        duration = Some(SpellDurationSource {
                            id: 903,
                            base_ms: *duration_ms as i32,
                            per_level_ms: 0,
                            maximum_ms: *duration_ms as i32,
                        });
                        value
                    }
                    _ => continue,
                };
                w[27] = value.max_level.into();
                w[28] = value.base_level.into();
                w[29] = value.spell_level.into();
                w[64 + i] = value.die_sides as u32;
                w[67 + i] = value.base_dice as u32;
                w[70 + i] = value.dice_per_level.bits();
                w[73 + i] = value.points_per_level.bits();
                w[76 + i] = value.base_points as u32;
                w[82 + i] = 6;
                w[100 + i] = 1;
                w[167 + i] = 0x3f80_0000;
            }
            Some(SpellAuthoritySource {
                words: w,
                server_side: 0,
                server_attributes: 0,
                bonus_coefficients: [0; 3],
                attack_power_coefficients: [0; 3],
                cast_time: SpellCastTimeSource {
                    id: 901,
                    base_ms: s.cast_ms as i32,
                    per_level_ms: 0,
                    minimum_ms: 0,
                },
                duration,
                range: SpellRangeSource {
                    id: 902,
                    minimum: spell::SpellFloat::ZERO,
                    maximum: synthetic_fixture::float(30.0),
                    flags: 0,
                },
                category_flags: 0,
                facing_flags: 1,
                hooks: hooks(),
            })
        })
        .collect();
    SpellAuthorityInput {
        spells,
        npcs: p
            .npcs
            .iter()
            .map(|n| NpcSpellSource {
                entry: n.entry,
                resistances: [0; 6],
                mechanic_immune_mask: 0,
                school_immune_mask: 0,
                minimum_level: n.ai.level,
                maximum_level: n.ai.level,
                rank: 0,
                subtype: 0,
                creature_type: 7,
                spell_list_id: 0,
                script_id: 0,
                ai_name: String::new(),
                addon_auras: SpellSourceRows::Complete(vec![]),
                spawn_addon_auras: SpellSourceRows::Complete(vec![]),
                immunity_set: SpellSourceRows::Complete(vec![]),
                event_scripts: SpellSourceRows::Complete(vec![]),
            })
            .collect(),
        session: SpellSessionSource {
            player_stand_state: 0,
            player_gm_mode: false,
            latency_ms: 0,
            session_scripts: SpellSourceRows::Complete(vec![]),
        },
        modifier_hooks: hooks(),
        initial_cast_dependencies: SpellSourceRows::Complete(vec![]),
        modifier_spell_ids: SpellSourceRows::Complete(
            p.abilities
                .iter()
                .filter_map(|a| {
                    matches!(a.admission, AbilityAdmission::Available(_)).then_some(a.id)
                })
                .chain(
                    p.combat
                        .iter()
                        .flat_map(|c| c.initial_auras.iter().map(|a| a.spell_id)),
                )
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        ),
    }
}
pub fn rekey(p: &mut Profile, initial: &mut init::StarterInput) {
    // ID 133 is source-audited, but every value in this fixture is authored.
    // Re-key only this fixture, never the legacy generic component fixtures.
    p.abilities[0].id = 133;
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.spell_id = 133;
    }
    if let Some(motion) = p.source_motion.as_mut() {
        let hit = motion.spell_hits.remove(&60001).unwrap();
        motion.spell_hits.insert(133, hit);
    }
    initial.spells[0].id = 133;
    initial.spells[0].words[0] = 133;
    if let init::Rows::Complete(rows) = &mut initial.default_spells {
        rows[0] = 133;
    }
    if let init::Rows::Complete(rows) = &mut initial.skill_abilities {
        rows[0].spell = 133;
    }
    initial.active_support[0].spell_id = 133;
}
pub fn inputs() -> (Profile, init::StarterInput, SpellAuthorityInput) {
    let mut p = synthetic_combat_fixture::profile();
    let mut initial = synthetic_fixture::initialized_starter_input();
    rekey(&mut p, &mut initial);
    initial.items[0].max_durability = 0;
    let sources = source(&p);
    for s in &sources.spells {
        initial
            .spells
            .iter_mut()
            .find(|r| r.id == s.words[0])
            .unwrap()
            .words = s.words.clone().try_into().unwrap();
    }
    p.initialize_player(&initial, synthetic_fixture::float(100.0), 1)
        .unwrap();
    (p, initial, sources)
}
pub fn profile() -> Profile {
    let (mut p, initial, sources) = inputs();
    p.initialize_spell_authority(&initial, sources).unwrap();
    p
}
#[derive(Default)]
pub struct SpatialOnly {
    pub base: synthetic_combat_fixture::SpatialOnly,
    pub queries: Vec<AuthorityRequest>,
    pub derived: Vec<AuthorityReply>,
    pub line_of_sight: Option<ai::movement::Check>,
    pub fail_range_phase: Option<SourceSpellPhase>,
    pub facing: Option<ai::movement::Check>,
    pub impact_visibility: Option<ai::movement::Check>,
    pub stale: bool,
    pub wrong_kind: bool,
    pub fail_phase: Option<SourceSpellPhase>,
}
impl Authority for SpatialOnly {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.queries.push(request.clone());
        assert!(
            !matches!(request.query, Query::Spell { .. }),
            "legacy authored SpellFacts escaped selected source host"
        );
        if let Query::SourceLocation(query) = &request.query {
            return Ok(AuthorityReply {
                request: request.clone(),
                facts: Facts::SourceLocation(SourceLocationFacts::new(
                    query,
                    &authored_location_catalog(),
                    7,
                )?),
            });
        }
        let Query::SourceSpell(query) = &request.query else {
            return self.base.resolve(request);
        };
        let facts = match query {
            SourceSpellQuery::Semantics(_) => panic!("derived spell facts escaped source host"),
            SourceSpellQuery::LineOfSight(_) => SourceSpellFacts::LineOfSight(
                self.line_of_sight.unwrap_or(ai::movement::Check::Passed),
            ),
            SourceSpellQuery::Range { .. } => panic!("derived spell range escaped source host"),
            SourceSpellQuery::Facing(_) => {
                SourceSpellFacts::Facing(self.facing.unwrap_or(ai::movement::Check::Passed))
            }
            SourceSpellQuery::ImpactVisibility(_) => SourceSpellFacts::ImpactVisibility(
                self.impact_visibility
                    .unwrap_or(ai::movement::Check::Passed),
            ),
        };
        let mut reply = AuthorityReply {
            request: request.clone(),
            facts: Facts::SourceSpell(facts),
        };
        if self.stale {
            reply.request.binding.tick += 1;
        }
        if self.wrong_kind {
            reply.facts = Facts::SourceSpell(SourceSpellFacts::Range(SpellRangeResult::Within));
        }
        Ok(reply)
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.derived.push(reply.clone());
        if let Query::SourceSpell(SourceSpellQuery::Range { context, .. }) = &reply.request.query {
            if Some(context.phase) == self.fail_range_phase {
                return Err(NativeError::AuthorityUnavailable(
                    "synthetic range receipt fault",
                ));
            }
        }
        if let Query::SourceSpell(SourceSpellQuery::Semantics(q)) = &reply.request.query {
            if Some(q.phase) == self.fail_phase {
                return Err(NativeError::AuthorityUnavailable(
                    "synthetic spell receipt fault",
                ));
            }
        }
        self.base.observe_derived(reply)
    }
}
pub fn spatial_only() -> SpatialOnly {
    let mut a = SpatialOnly::default();
    a.base.base.visits = Some(false);
    a
}

/// Deliberately authored full WDBC framing; never extracted game assets.
pub fn authored_location_catalog() -> SourceLocationCatalog {
    fn dbc<const N: usize>(rows: &[[u32; N]]) -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        for value in [rows.len() as u32, N as u32, (N * 4) as u32, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for row in rows {
            for value in row {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.push(0);
        bytes
    }
    let areas = [(9, 12, 7), (12, 0, 8), (148, 0, 9)].map(|(id, zone, flag)| {
        let mut row = [0; 25];
        row[0] = id;
        row[2] = zone;
        row[3] = flag;
        row
    });
    SourceLocationCatalog::decode(&dbc(&areas), &dbc::<20>(&[]), &dbc(&[[0; 42]])).unwrap()
}
pub fn initialize_authored_location(profile: &mut Profile) {
    let catalog = authored_location_catalog();
    let snapshot = SourceLocationSnapshotBinding {
        map: profile.map_id,
        content: profile.content_digest,
        source_epoch: [73; 32],
        inventory: [74; 32],
        policy: [75; 32],
        catalog: catalog.digest(),
    };
    profile
        .initialize_source_location(snapshot, catalog)
        .unwrap();
}
