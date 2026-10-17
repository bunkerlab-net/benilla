// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Pure, bounded InitialCast mechanism plans. The authored facade remains a
//! synthetic mechanism contract. Source-derived plans are built privately during
//! actual starter learning, from retained indexed dependencies and order evidence.
//! Neither path certifies capture authenticity or native admission. No RNG is
//! sampled here; the owned plan retains every occurrence, including Detect.
//! Source: SpellTargets.cpp, Spell.cpp, Object.cpp, SpellEffects.cpp, Unit.cpp,
//! SpellAuras.cpp at the pin above. Existing crate GPL notices apply.
use super::{stats, InitError, InitialAura, Result, Rows, SpellRow};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const INITIAL_CAST_PHASE_POLICY: &str = "cmangos-8ec338a1-authored-initial-cast-phases-v1";
pub const MAX_AUTHORED_INITIAL_CASTS: usize = 256;
const ONE: u32 = 0x3f80_0000;

/// Mechanism-test assumptions, not source proof. Every assertion must be explicit.
/// The successful checks include the caller's whole location/condition scope;
/// candidate spell-ID absence is insufficient to assert the global area scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredPhasePrerequisites {
    pub fresh_unshapeshifted_player_not_in_world: bool,
    pub successful_conditions_and_location_checks: bool,
    pub spell_and_aura_scripts_absent: bool,
    pub spell_modifiers_absent: bool,
    pub neutral_proc_and_finish_paths: bool,
    pub server_side: u32,
    pub server_attributes: u32,
    /// The server's non-DBC coefficient cells, retained even on this unused path.
    pub effect_bonus_coefficient_bits: [u32; 3],
    pub effect_bonus_coefficient_from_ap_bits: [u32; 3],
}

/// Authored derived values only: neither Missing nor Found indexed-store evidence.
/// A zero value must be supplied, never inferred from a raw index or missing row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthoredPhaseResolutions {
    pub cast_time_index: u32,
    pub cast_time_ms: i32,
    pub duration_index: u32,
    pub duration_ms: i32,
}

/// One authored occurrence. Repeated IDs remain separate; this API performs no
/// learning, recursion, duplicate suppression, sorting, or creation/entry replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredInitialCast {
    pub row: SpellRow,
    pub prerequisites: Option<AuthoredPhasePrerequisites>,
    pub resolutions: Option<AuthoredPhaseResolutions>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InitialEffectPhase {
    Targetless,
    SelfHolder,
    /// Exact Detect3050 effect49: UNKNOWN target type, no target fill or dispatch.
    UnscheduledDetect,
}

/// Amount operations are seed-independent. Only exact Dodge81 may request a die;
/// the handler is empty, but its request and evaluated value cannot be discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InitialAmountOperation {
    Constant { value: i32 },
    PendingRandomNoOp { low: i32, high: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialEffectOperation {
    pub slot: u8,
    pub opcode: u32,
    pub phase: InitialEffectPhase,
    pub aura: u32,
    pub misc: i32,
    pub amount: Option<InitialAmountOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialCastOccurrencePlan {
    ordinal: u32,
    row: SpellRow,
    provenance: InitialCastProvenance,
    source_hash: [u8; 32],
    effects: Vec<InitialEffectOperation>,
}
impl InitialCastOccurrencePlan {
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    pub fn row(&self) -> &SpellRow {
        &self.row
    }
    pub fn source_hash(&self) -> [u8; 32] {
        self.source_hash
    }
    pub fn effects(&self) -> &[InitialEffectOperation] {
        &self.effects
    }
    pub fn targetless_mask(&self) -> u8 {
        self.mask(InitialEffectPhase::Targetless)
    }
    pub fn self_mask(&self) -> u8 {
        self.mask(InitialEffectPhase::SelfHolder)
    }
    fn mask(&self, phase: InitialEffectPhase) -> u8 {
        self.effects
            .iter()
            .filter(|e| e.phase == phase)
            .fold(0, |m, e| m | (1 << e.slot))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InitialCastProvenance {
    Authored(Box<AuthoredInitialCast>),
    SourceDerived { dependency_identity: [u8; 32] },
}
/// Only the preparation reducer may create this internal, resolved mechanism input.
#[derive(Clone, Copy)]
pub(super) struct SourcePhaseFields {
    pub server_side: u32,
    pub server_attributes: u32,
    pub bonus_coefficients: [u32; 3],
    pub attack_power_coefficients: [u32; 3],
}
#[derive(Clone, Copy)]
pub(super) struct SourcePhaseResolutions {
    pub cast_time_index: u32,
    pub cast_time_ms: i32,
    pub duration_index: u32,
    pub duration_ms: i32,
}
/// No public occurrence-list constructor exists for source-derived plans.
pub(super) struct SourcePlanBuilder {
    context_identity: [u8; 32],
    occurrences: Vec<InitialCastOccurrencePlan>,
    aura_keys: BTreeSet<(u32, i32)>,
    pending: usize,
}
impl SourcePlanBuilder {
    pub(super) fn new(context_identity: [u8; 32]) -> Self {
        Self {
            context_identity,
            occurrences: vec![],
            aura_keys: BTreeSet::new(),
            pending: 0,
        }
    }
    pub(super) fn append(
        &mut self,
        row: &SpellRow,
        fields: SourcePhaseFields,
        resolutions: SourcePhaseResolutions,
        dependency_identity: [u8; 32],
    ) -> Result<Vec<InitialEffectOperation>> {
        if self.occurrences.len() == MAX_AUTHORED_INITIAL_CASTS {
            return Err(InitError::Invalid("source initial cast count"));
        }
        let effects = classify(row, &fields, resolutions)?;
        for effect in &effects {
            if effect.phase == InitialEffectPhase::SelfHolder
                && !self.aura_keys.insert((effect.aura, effect.misc))
            {
                return Err(InitError::Invalid("source initial aura stacking"));
            }
            if matches!(
                effect.amount,
                Some(InitialAmountOperation::PendingRandomNoOp { .. })
            ) {
                self.pending += 1;
                if self.pending > 1 {
                    return Err(InitError::Invalid(
                        "source initial pending occurrence bound",
                    ));
                }
            }
        }
        self.occurrences.push(InitialCastOccurrencePlan {
            ordinal: self.occurrences.len() as u32,
            row: row.clone(),
            provenance: InitialCastProvenance::SourceDerived {
                dependency_identity,
            },
            source_hash: row.source_hash(),
            effects: effects.clone(),
        });
        Ok(effects)
    }
    pub(super) fn finish(self) -> InitialCastPlan {
        let mut h = Sha256::new();
        h.update(b"cmangos-8ec338a1-source-initial-cast-plan-v1\0");
        h.update(self.context_identity);
        h.update(plan_identity(&self.occurrences));
        InitialCastPlan {
            occurrences: self.occurrences,
            identity: h.finalize().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialIntegerDrawRequest {
    pub occurrence: u32,
    pub spell_id: u32,
    pub effect_slot: u8,
    pub low: i32,
    pub high: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialIntegerDrawResult {
    pub request: InitialIntegerDrawRequest,
    pub value: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialProficiencyMasks {
    pub weapon: u32,
    pub armor: u32,
}

/// Observable projected mechanism events. AuraApplied records source modifier
/// dispatch; this module does not apply stats, create skills, or grant reputation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InitialPhaseEvent {
    CastStart {
        occurrence: u32,
        spell_id: u32,
    },
    IntegerDraw(InitialIntegerDrawResult),
    Amount {
        occurrence: u32,
        slot: u8,
        phase: InitialEffectPhase,
        value: i32,
    },
    Dispatch {
        occurrence: u32,
        slot: u8,
        phase: InitialEffectPhase,
        opcode: u32,
    },
    HolderCreated {
        occurrence: u32,
    },
    AuraConstructed {
        occurrence: u32,
        aura: InitialAura,
    },
    AuraRegistered {
        occurrence: u32,
        slot: u8,
    },
    AuraApplied {
        occurrence: u32,
        slot: u8,
    },
    CastComplete {
        occurrence: u32,
        spell_id: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialPhaseHolder {
    pub occurrence: u32,
    pub spell_id: u32,
    pub source_hash: [u8; 32],
    pub auras: Vec<InitialAura>,
}

/// Immutable source-bound plan; no deserializer can bypass the classifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialCastPlan {
    occurrences: Vec<InitialCastOccurrencePlan>,
    identity: [u8; 32],
}
impl serde::Serialize for InitialCastPlan {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.identity, serializer)
    }
}
/// A realized authored-roll trace, not a seeded native-genesis receipt.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InitialCastEvaluation {
    plan_identity: [u8; 32],
    initial_masks: InitialProficiencyMasks,
    final_masks: InitialProficiencyMasks,
    draws: Vec<InitialIntegerDrawResult>,
    events: Vec<InitialPhaseEvent>,
    holders: Vec<InitialPhaseHolder>,
}
impl InitialCastEvaluation {
    pub fn plan_identity(&self) -> [u8; 32] {
        self.plan_identity
    }
    pub fn initial_masks(&self) -> InitialProficiencyMasks {
        self.initial_masks
    }
    pub fn final_masks(&self) -> InitialProficiencyMasks {
        self.final_masks
    }
    pub fn draws(&self) -> &[InitialIntegerDrawResult] {
        &self.draws
    }
    pub fn events(&self) -> &[InitialPhaseEvent] {
        &self.events
    }
    pub fn holders(&self) -> &[InitialPhaseHolder] {
        &self.holders
    }
}

impl InitialCastPlan {
    pub fn from_authored(casts: &[AuthoredInitialCast]) -> Result<Self> {
        if casts.len() > MAX_AUTHORED_INITIAL_CASTS {
            return Err(InitError::Invalid("authored initial cast count"));
        }
        let mut occurrences = Vec::with_capacity(casts.len());
        let mut aura_keys = BTreeSet::new();
        for (ordinal, cast) in casts.iter().enumerate() {
            let effects = classify_authored(cast)?;
            for effect in &effects {
                if effect.phase == InitialEffectPhase::SelfHolder
                    && !aura_keys.insert((effect.aura, effect.misc))
                {
                    return Err(InitError::Invalid("authored initial aura stacking"));
                }
            }
            occurrences.push(InitialCastOccurrencePlan {
                ordinal: ordinal as u32,
                row: cast.row.clone(),
                provenance: InitialCastProvenance::Authored(Box::new(cast.clone())),
                source_hash: cast.row.source_hash(),
                effects,
            });
        }
        let identity = plan_identity(&occurrences);
        Ok(Self {
            occurrences,
            identity,
        })
    }
    pub fn occurrences(&self) -> &[InitialCastOccurrencePlan] {
        &self.occurrences
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn draw_requests(&self) -> Vec<InitialIntegerDrawRequest> {
        self.occurrences
            .iter()
            .flat_map(|cast| scheduled(cast).filter_map(move |effect| request(cast, effect)))
            .collect()
    }
    /// Rolls correspond exactly to draw_requests(). Validate every roll before
    /// projecting any handler/state event. Failure returns no partial receipt.
    pub fn evaluate(
        &self,
        rolls: &[i32],
        initial_masks: InitialProficiencyMasks,
    ) -> Result<InitialCastEvaluation> {
        let requests = self.draw_requests();
        if rolls.len() != requests.len() {
            return Err(InitError::Invalid("authored initial integer draw count"));
        }
        let draws: Vec<_> = requests
            .into_iter()
            .zip(rolls)
            .map(|(request, &value)| {
                if !(request.low..=request.high).contains(&value) {
                    return Err(InitError::Invalid("authored initial integer draw range"));
                }
                Ok(InitialIntegerDrawResult { request, value })
            })
            .collect::<Result<_>>()?;
        let mut out = InitialCastEvaluation {
            plan_identity: self.identity,
            initial_masks,
            final_masks: initial_masks,
            draws,
            events: Vec::new(),
            holders: Vec::new(),
        };
        let mut draw_index = 0;
        for cast in &self.occurrences {
            let occurrence = cast.ordinal;
            let spell_id = cast.row.id;
            out.events.push(InitialPhaseEvent::CastStart {
                occurrence,
                spell_id,
            });
            for effect in cast
                .effects
                .iter()
                .filter(|e| e.phase == InitialEffectPhase::Targetless)
            {
                execute_effect(cast, effect, &mut draw_index, &mut out)?;
            }
            if cast.self_mask() != 0 {
                out.events
                    .push(InitialPhaseEvent::HolderCreated { occurrence });
                let mut auras = Vec::new();
                for effect in cast
                    .effects
                    .iter()
                    .filter(|e| e.phase == InitialEffectPhase::SelfHolder)
                {
                    let amount = execute_effect(cast, effect, &mut draw_index, &mut out)?;
                    let aura = InitialAura {
                        spell_id,
                        effect_index: effect.slot,
                        aura_type: effect.aura,
                        misc_value: effect.misc,
                        amount,
                    };
                    out.events
                        .push(InitialPhaseEvent::AuraConstructed { occurrence, aura });
                    auras.push(aura);
                }
                // Unit::AddSpellAuraHolder registers ALL auras before applying any.
                for aura in &auras {
                    out.events.push(InitialPhaseEvent::AuraRegistered {
                        occurrence,
                        slot: aura.effect_index,
                    });
                }
                for aura in &auras {
                    out.events.push(InitialPhaseEvent::AuraApplied {
                        occurrence,
                        slot: aura.effect_index,
                    });
                }
                out.holders.push(InitialPhaseHolder {
                    occurrence,
                    spell_id,
                    source_hash: cast.source_hash,
                    auras,
                });
            }
            out.events.push(InitialPhaseEvent::CastComplete {
                occurrence,
                spell_id,
            });
        }
        Ok(out)
    }
    /// Recompute all events and results, including no-state casts and requests.
    pub fn verify_evaluation(&self, evaluated: &InitialCastEvaluation) -> Result<()> {
        let rolls: Vec<_> = evaluated.draws.iter().map(|draw| draw.value).collect();
        if self.evaluate(&rolls, evaluated.initial_masks)? != *evaluated {
            return Err(InitError::Invalid("authored initial evaluation binding"));
        }
        Ok(())
    }
}

fn scheduled(cast: &InitialCastOccurrencePlan) -> impl Iterator<Item = &InitialEffectOperation> {
    [
        InitialEffectPhase::Targetless,
        InitialEffectPhase::SelfHolder,
    ]
    .into_iter()
    .flat_map(move |phase| cast.effects.iter().filter(move |e| e.phase == phase))
}
fn request(
    cast: &InitialCastOccurrencePlan,
    effect: &InitialEffectOperation,
) -> Option<InitialIntegerDrawRequest> {
    match effect.amount {
        Some(InitialAmountOperation::PendingRandomNoOp { low, high }) => {
            Some(InitialIntegerDrawRequest {
                occurrence: cast.ordinal,
                spell_id: cast.row.id,
                effect_slot: effect.slot,
                low,
                high,
            })
        }
        _ => None,
    }
}
fn execute_effect(
    cast: &InitialCastOccurrencePlan,
    effect: &InitialEffectOperation,
    draw_index: &mut usize,
    out: &mut InitialCastEvaluation,
) -> Result<i32> {
    let value = match effect.amount {
        Some(InitialAmountOperation::Constant { value }) => value,
        Some(InitialAmountOperation::PendingRandomNoOp { .. }) => {
            let result = *out
                .draws
                .get(*draw_index)
                .ok_or(InitError::Invalid("initial draw sequence"))?;
            if Some(result.request) != request(cast, effect) {
                return Err(InitError::Invalid("initial draw sequence"));
            }
            *draw_index += 1;
            out.events.push(InitialPhaseEvent::IntegerDraw(result));
            let w = &cast.row.words;
            stats::effect_amount_with_addition(
                w[76 + effect.slot as usize] as i32,
                w[67 + effect.slot as usize],
                result.value,
            )?
        }
        None => return Err(InitError::Invalid("unscheduled initial amount")),
    };
    out.events.push(InitialPhaseEvent::Amount {
        occurrence: cast.ordinal,
        slot: effect.slot,
        phase: effect.phase,
        value,
    });
    out.events.push(InitialPhaseEvent::Dispatch {
        occurrence: cast.ordinal,
        slot: effect.slot,
        phase: effect.phase,
        opcode: effect.opcode,
    });
    if effect.opcode == 60 {
        let w = &cast.row.words;
        let mask = if w[58] == 2 {
            &mut out.final_masks.weapon
        } else {
            &mut out.final_masks.armor
        };
        // EffectProficiency grants the WHOLE mask only if there is no overlap.
        if *mask & w[59] == 0 {
            *mask |= w[59];
        }
    }
    Ok(value)
}

fn plan_identity(casts: &[InitialCastOccurrencePlan]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(INITIAL_CAST_PHASE_POLICY.as_bytes());
    h.update([0]);
    h.update((casts.len() as u64).to_le_bytes());
    for cast in casts {
        h.update(cast.ordinal.to_le_bytes());
        h.update(cast.source_hash);
        match &cast.provenance {
            InitialCastProvenance::Authored(authored) => {
                // Presence is validated, but retained here so this identity has no neutral
                // defaults even if the internal representation is extended later.
                if let Some(p) = &authored.prerequisites {
                    h.update([
                        1,
                        u8::from(p.fresh_unshapeshifted_player_not_in_world),
                        u8::from(p.successful_conditions_and_location_checks),
                        u8::from(p.spell_and_aura_scripts_absent),
                        u8::from(p.spell_modifiers_absent),
                        u8::from(p.neutral_proc_and_finish_paths),
                    ]);
                    for value in [p.server_side, p.server_attributes]
                        .into_iter()
                        .chain(p.effect_bonus_coefficient_bits)
                        .chain(p.effect_bonus_coefficient_from_ap_bits)
                    {
                        h.update(value.to_le_bytes());
                    }
                } else {
                    h.update([0]);
                }
                if let Some(r) = authored.resolutions {
                    h.update([1]);
                    h.update(r.cast_time_index.to_le_bytes());
                    h.update(r.cast_time_ms.to_le_bytes());
                    h.update(r.duration_index.to_le_bytes());
                    h.update(r.duration_ms.to_le_bytes());
                } else {
                    h.update([0]);
                }
            }
            InitialCastProvenance::SourceDerived {
                dependency_identity,
            } => {
                h.update([1]);
                h.update(dependency_identity);
            }
        }
    }
    h.finalize().into()
}

/// Semantic morphology guards audited against the pinned core's
/// sql/base/dbc/original_data/Spell.sql. These are not complete bundled rows,
/// loaded-store equality, or indexed source lookups.
struct Shape {
    effects: &'static [(u32, u32, i32)],
    item_class: i32,
    item_mask: u32,
}
fn shape(id: u32) -> Result<Shape> {
    let (effects, item_class, item_mask): (&[(u32, u32, i32)], i32, u32) = match id {
        81 => (&[(20, 0, 90)], -1, u32::MAX),
        203 => (&[(25, 0, 0)], 2, 8192),
        204 => (&[(26, 0, 0)], -1, 0),
        227 => (&[(25, 0, 0), (60, 0, 0)], 2, 1024),
        522 => (&[(37, 0, 0)], -1, u32::MAX),
        668 => (&[(39, 0, 7)], -1, u32::MAX),
        2382 => (&[(25, 0, 0), (60, 0, 0)], 2, 16384),
        3050 => (&[(49, 0, 0)], -1, u32::MAX),
        5009 => (&[(25, 0, 0), (60, 0, 0)], 2, 524288),
        9078 => (&[(60, 0, 0)], 4, 2),
        9125 => (&[(60, 0, 0)], 4, 1),
        20597 => (&[(6, 98, 43), (6, 98, 55)], -1, 0),
        20598 => (&[(6, 137, 4)], -1, 0),
        20599 => (&[(6, 156, 0)], -1, 0),
        20864 => (&[(6, 98, 54), (6, 98, 160)], -1, 0),
        _ => {
            return Err(InitError::Unsupported {
                domain: "authored initial cast morphology",
                id,
                detail: 0,
            });
        }
    };
    Ok(Shape {
        effects,
        item_class,
        item_mask,
    })
}
fn classify_authored(cast: &AuthoredInitialCast) -> Result<Vec<InitialEffectOperation>> {
    let p = cast
        .prerequisites
        .as_ref()
        .ok_or(InitError::Invalid("missing authored initial prerequisites"))?;
    if !p.fresh_unshapeshifted_player_not_in_world
        || !p.successful_conditions_and_location_checks
        || !p.spell_and_aura_scripts_absent
        || !p.spell_modifiers_absent
        || !p.neutral_proc_and_finish_paths
    {
        return Err(InitError::Invalid(
            "nonneutral authored initial prerequisites",
        ));
    }
    let r = cast
        .resolutions
        .ok_or(InitError::Invalid("missing authored initial resolutions"))?;
    classify(
        &cast.row,
        &SourcePhaseFields {
            server_side: p.server_side,
            server_attributes: p.server_attributes,
            bonus_coefficients: p.effect_bonus_coefficient_bits,
            attack_power_coefficients: p.effect_bonus_coefficient_from_ap_bits,
        },
        SourcePhaseResolutions {
            cast_time_index: r.cast_time_index,
            cast_time_ms: r.cast_time_ms,
            duration_index: r.duration_index,
            duration_ms: r.duration_ms,
        },
    )
}
fn classify(
    row: &SpellRow,
    p: &SourcePhaseFields,
    r: SourcePhaseResolutions,
) -> Result<Vec<InitialEffectOperation>> {
    let w = &row.words;
    let reject = || InitError::Unsupported {
        domain: "authored initial cast morphology",
        id: row.id,
        detail: 0,
    };
    let shape = shape(row.id)?;
    if p.server_side != 0 || p.server_attributes != 0 || p.attack_power_coefficients != [0; 3] {
        return Err(InitError::Invalid(
            "nonneutral authored initial prerequisites",
        ));
    }
    if r.cast_time_index != w[18]
        || r.duration_index != w[30]
        || r.cast_time_ms != 0
        || row.duration_ms != Some(r.duration_ms)
        || !matches!(r.duration_ms, -1 | 0)
    {
        return Err(InitError::Invalid("authored initial resolution mismatch"));
    }
    if row.id != w[0]
        || row.previous_rank.is_some()
        || row.talent
        || row.external_initial_hooks
        || !matches!(&row.learned_spells, Rows::Complete(v) if v.is_empty())
    {
        return Err(reject());
    }
    let holder = shape.effects[0].0 == 6;
    if holder && !(r.duration_ms == -1 || (r.duration_ms == 0 && w[30] == 0)) {
        return Err(InitError::Invalid("authored initial holder permanence"));
    }
    let weapon_group = matches!(row.id, 81 | 203 | 204 | 227 | 522 | 668 | 2382 | 5009);
    if w[1] != 0
        || w[6] != if holder || row.id == 81 { 0x50 } else { 0xc0 }
        || w[7] != if row.id == 3050 { 0x0200_0000 } else { 0 }
        || w[25] != 101
        || w[36] != 1
        || w[58] as i32 != shape.item_class
        || w[59] != shape.item_mask
        || w[164] != u32::from(weapon_group)
        || w[165] != u32::from(weapon_group)
        || w[166] != if holder { 0 } else { u32::MAX }
    {
        return Err(reject());
    }
    // Only the listed source-proven inert or separately checked cells may vary.
    // No effect opcode implicitly opens another field, target, or callback path.
    for (index, &value) in w.iter().enumerate() {
        if !matches!(index, 0 | 3 | 6 | 7 | 18 | 25 | 27..=30 | 36 | 38 | 58 | 59 | 61..=69 | 73..=78 | 82..=84 | 91..=93 | 97..=99 | 106..=108 | 115..=155 | 164..=169)
            && value != 0
        {
            return Err(reject());
        }
    }
    let mut effects = Vec::with_capacity(shape.effects.len());
    for slot in 0..3 {
        // Spell construction caches simple integer values even for unscheduled slots.
        let dice = i32::try_from(w[67 + slot]).map_err(|_| InitError::Numeric("base dice"))?;
        (w[76 + slot] as i32)
            .checked_add(dice)
            .ok_or(InitError::Numeric("cached initial effect value"))?;
        let expected = shape.effects.get(slot).copied().unwrap_or((0, 0, 0));
        if (w[61 + slot], w[91 + slot], w[106 + slot] as i32) != expected
            || w[82 + slot] != u32::from(holder && expected.0 != 0)
            || w[97 + slot] != if row.id == 81 && slot == 0 { ONE } else { 0 }
            || w[167 + slot] != if !holder || expected.0 != 0 { ONE } else { 0 }
            || p.bonus_coefficients[slot]
                != if (holder || row.id == 81) && expected.0 != 0 {
                    ONE
                } else {
                    0
                }
        {
            return Err(reject());
        }
        if expected.0 == 0 {
            if [64, 67, 73, 76].iter().any(|&base| w[base + slot] != 0) {
                return Err(reject());
            }
            continue;
        }
        let phase = if holder {
            InitialEffectPhase::SelfHolder
        } else if row.id == 3050 {
            InitialEffectPhase::UnscheduledDetect
        } else {
            InitialEffectPhase::Targetless
        };
        let amount = if row.id == 3050 {
            if (w[64 + slot], w[67 + slot], w[73 + slot], w[76 + slot])
                != (1, 1, 0x40a0_0000, u32::MAX)
            {
                return Err(reject());
            }
            None // No HandleEffect, no scaling/rank lookup, no amount, no RNG.
        } else if row.id == 81 {
            if (w[64 + slot], w[67 + slot], w[73 + slot], w[76 + slot]) != (6, 0, 0, 0) {
                return Err(reject());
            }
            Some(InitialAmountOperation::PendingRandomNoOp { low: 0, high: 6 })
        } else {
            if !matches!(w[64 + slot], 0 | 1) || w[73 + slot] != 0 {
                return Err(reject());
            }
            let value = stats::constant_effect_amount(w[76 + slot] as i32, w[67 + slot])?;
            if expected.1 == 98 {
                i16::try_from(value)
                    .map_err(|_| InitError::Numeric("initial skill aura amount"))?;
            }
            if holder && value.unsigned_abs() > 1_000_000 {
                return Err(InitError::Numeric("initial aura amount"));
            }
            Some(InitialAmountOperation::Constant { value })
        };
        effects.push(InitialEffectOperation {
            slot: slot as u8,
            opcode: expected.0,
            phase,
            aura: expected.1,
            misc: expected.2,
            amount,
        });
    }
    Ok(effects)
}

#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
pub(super) mod tests;
