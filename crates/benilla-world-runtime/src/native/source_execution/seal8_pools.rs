// SPDX-License-Identifier: GPL-3.0-or-later
//! Numeric projection of observed pool operands, never manager selections or F0 inputs.
use super::retained_seal8::{parse, scan_seal8, Seal8Context, SEAL8_TRACE_HASH};
use super::*;
use benilla_sim::source_rng::{DrawCursor, EventKind, Mt19937, MtState, SourceRandom, TraceEvent};
use rustc_apfloat::{ieee::Double, Float, Round};
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub struct Seal8PoolReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub mt_raw_words_checked: u64,
    pub pool_double_rolls_matched: u64,
    pub pool_shuffles_matched: u64,
    pub shuffled_members_matched: u64,
    pub independently_loaded_pool_selections_matched: u64,
    pub mp_transitions_matched: u64,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
    pub first_refusal: Refusal,
}
struct Engine {
    value: Mt19937,
    position: u64,
}
struct Shuffle {
    engine: u64,
    initial: Mt19937,
    position: u64,
    before: Vec<u64>,
    after: Vec<u64>,
    words: Vec<(u64, u64)>,
}
#[derive(Default)]
struct Chance {
    engine: Option<u64>,
    initial: Option<Mt19937>,
    position: u64,
    words: Vec<(u64, u64)>,
}
#[derive(Default)]
struct Projector {
    aliases: BTreeMap<u64, u64>,
    engines: BTreeMap<u64, Engine>,
    shuffles: BTreeMap<(u64, u64), Shuffle>,
    chances: BTreeMap<u64, Chance>,
    raw: u64,
    rolls: u64,
    shuffled: u64,
    members: u64,
}
fn address(text: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        text.and_then(|v| v.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.pool.projection_address"))?,
        16,
    )
    .map_err(|_| refuse("execution.pool.projection_address"))
}
fn mismatch(id: &str) -> Refusal {
    refuse(id)
}
impl Projector {
    fn event(&mut self, context: Seal8Context, kind: &str, payload: &str) -> Result<()> {
        let mut fields = payload.split_whitespace();
        match kind {
            "logical_birth" => {
                let id = parse(fields.next())?;
                if fields.next() == Some("mt_engine") {
                    let _: u64 = parse(fields.next())?;
                    let _: u64 = parse(fields.next())?;
                    let pointer = address(fields.next())?;
                    require(
                        self.aliases.insert(pointer, id).is_none() && self.aliases.len() <= 64,
                        "execution.pool.projection_engine_lifetime",
                    )?;
                }
            }
            "logical_death" => {
                let id: u64 = parse(fields.next())?;
                self.engines.remove(&id);
                self.aliases.retain(|_, value| *value != id);
            }
            "mt_state" => {
                let pointer = address(fields.next())?;
                let id = *self
                    .aliases
                    .get(&pointer)
                    .ok_or_else(|| refuse("execution.pool.projection_engine_birth"))?;
                let mut words = [0; 624];
                for word in &mut words {
                    *word = parse(fields.next())?;
                }
                let index = parse(fields.next())?;
                require(fields.next().is_none(), "execution.pool.projection_fields")?;
                let value = Mt19937::from_state(MtState { words, index })
                    .map_err(|_| refuse("execution.pool.projection_state"))?;
                if let Some(current) = self.engines.get(&id) {
                    // A later snapshot is an expected value, never a reseed input.
                    require(
                        current.value == value,
                        "execution.pool.projection_state_divergence",
                    )?;
                } else {
                    self.engines.insert(id, Engine { value, position: 0 });
                }
            }
            "rng_call"
                if payload.starts_with("rand_chance ")
                    && payload.contains("Pools/PoolManager.cpp") =>
            {
                require(
                    self.chances
                        .insert(context.thread, Chance::default())
                        .is_none(),
                    "execution.pool.projection_nested_roll",
                )?;
            }
            "rng_result" if self.chances.contains_key(&context.thread) => {
                require(
                    fields.next() == Some("rand_chance"),
                    "execution.pool.projection_roll_owner",
                )?;
                let text = fields
                    .next()
                    .ok_or_else(|| refuse("execution.pool.projection_fields"))?;
                let expected = Double::from_str_r(text, Round::NearestTiesToEven)
                    .map_err(|_| refuse("execution.pool.projection_float"))?
                    .value
                    .to_bits() as u64;
                let chance = self
                    .chances
                    .remove(&context.thread)
                    .ok_or_else(|| refuse("execution.pool.projection_roll_owner"))?;
                let mut engine = chance
                    .initial
                    .ok_or_else(|| refuse("execution.pool.projection_roll_engine"))?;
                let mut cursor = DrawCursor {
                    raw: chance.position,
                    next_span: 0,
                };
                let mut raw = Vec::new();
                let mut sink = |event: TraceEvent| {
                    if let EventKind::Raw { position, word, .. } = event.kind {
                        raw.push((position, word));
                    }
                };
                let actual = SourceRandom::new(context.thread, &mut engine, &mut cursor, &mut sink)
                    .map_err(|_| mismatch("execution.pool.projection_rng"))?
                    .rand_chance()
                    .map_err(|_| mismatch("execution.pool.projection_rng"))?;
                require(
                    actual.to_bits() == expected && raw == chance.words,
                    "execution.pool.projection_roll_divergence",
                )?;
                self.rolls += 1;
            }
            "rng_shuffle_begin" if payload.contains("PoolGroup") && payload.contains("RollOne") => {
                let operation = parse(fields.next())?;
                let engine = parse(fields.next())?;
                let current = self
                    .engines
                    .get(&engine)
                    .ok_or_else(|| refuse("execution.pool.projection_shuffle_engine"))?;
                let shuffle = Shuffle {
                    engine,
                    initial: current.value.clone(),
                    position: current.position,
                    before: vec![],
                    after: vec![],
                    words: vec![],
                };
                require(
                    operation == context.operation
                        && self
                            .shuffles
                            .insert((context.thread, operation), shuffle)
                            .is_none(),
                    "execution.pool.projection_shuffle_owner",
                )?;
            }
            "rng_shuffle_member" => {
                let operation = parse(fields.next())?;
                if let Some(shuffle) = self.shuffles.get_mut(&(context.thread, operation)) {
                    let phase = fields.next();
                    let index: usize = parse(fields.next())?;
                    let value: u64 = parse(fields.next())?;
                    let list = match phase {
                        Some("before") => &mut shuffle.before,
                        Some("after") => &mut shuffle.after,
                        _ => return Err(refuse("execution.pool.projection_shuffle_phase")),
                    };
                    require(
                        index == list.len() && index < 2_000_000 && value <= u64::from(u32::MAX),
                        "execution.pool.projection_shuffle_member",
                    )?;
                    list.push(value);
                }
            }
            "rng_shuffle_end" => {
                let operation = parse(fields.next())?;
                if let Some(mut shuffle) = self.shuffles.remove(&(context.thread, operation)) {
                    let count: usize = parse(fields.next())?;
                    require(
                        count == shuffle.before.len() && count == shuffle.after.len(),
                        "execution.pool.projection_shuffle_count",
                    )?;
                    let mut cursor = DrawCursor {
                        raw: shuffle.position,
                        next_span: 0,
                    };
                    let mut raw = Vec::new();
                    let mut sink = |event: TraceEvent| {
                        if let EventKind::Raw { position, word, .. } = event.kind {
                            raw.push((position, word));
                        }
                    };
                    SourceRandom::new(context.thread, &mut shuffle.initial, &mut cursor, &mut sink)
                        .map_err(|_| mismatch("execution.pool.projection_rng"))?
                        .shuffle(&mut shuffle.before)
                        .map_err(|_| mismatch("execution.pool.projection_rng"))?;
                    require(
                        shuffle.before == shuffle.after && raw == shuffle.words,
                        "execution.pool.projection_shuffle_divergence",
                    )?;
                    self.shuffled += 1;
                    self.members += count as u64;
                }
            }
            "mt_raw" => {
                let pointer = address(fields.next())?;
                let position = parse(fields.next())?;
                let word: u64 = parse(fields.next())?;
                let id = *self
                    .aliases
                    .get(&pointer)
                    .ok_or_else(|| refuse("execution.pool.projection_engine_birth"))?;
                let current = self
                    .engines
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.pool.projection_engine_state"))?;
                if let Some(chance) = self.chances.get_mut(&context.thread) {
                    if chance.engine.is_none() {
                        chance.engine = Some(id);
                        chance.initial = Some(current.value.clone());
                        chance.position = current.position;
                    }
                    require(
                        chance.engine == Some(id) && chance.words.len() < 4096,
                        "execution.pool.projection_roll_span",
                    )?;
                    chance.words.push((position, word));
                }
                if let Some(shuffle) = self.shuffles.get_mut(&(context.thread, context.operation)) {
                    require(
                        shuffle.engine == id && shuffle.words.len() < 2_000_000,
                        "execution.pool.projection_shuffle_span",
                    )?;
                    shuffle.words.push((position, word));
                }
                require(
                    position == current.position && word == u64::from(current.value.next_u32()),
                    "execution.pool.projection_raw_divergence",
                )?;
                current.position += 1;
                self.raw += 1;
            }
            _ => {}
        }
        Ok(())
    }
    fn finish(&self) -> Result<()> {
        require(
            self.chances.is_empty() && self.shuffles.is_empty(),
            "execution.pool.projection_truncated",
        )
    }
}
/// Pre-shuffle members are explicitly observed numeric operands, not independently
/// loaded definitions. No selected GUID, CanSpawn answer or MP transition is replayed.
pub fn compare_seal8_unsealed_pools(root: &Path) -> Result<Seal8PoolReport> {
    let mut projector = Projector::default();
    let records = scan_seal8(root, |context, kind, payload| {
        projector.event(context, kind, payload)
    })?;
    projector.finish()?;
    Ok(Seal8PoolReport {
        adapter: "benilla-seal8-08-unsealed-pool-numerics-v1",
        trace_sha256: Sha256Id(SEAL8_TRACE_HASH.into()),
        records,
        mt_raw_words_checked: projector.raw,
        pool_double_rolls_matched: projector.rolls,
        pool_shuffles_matched: projector.shuffled,
        shuffled_members_matched: projector.members,
        independently_loaded_pool_selections_matched: 0,
        mp_transitions_matched: 0,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
        first_refusal: PoolPending::SealedExecution.refusal(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context(operation: u64) -> Seal8Context {
        Seal8Context {
            event: 1,
            operation,
            thread: 1,
            world: 0,
            map_update: 0,
            map_generation: 0,
            map: 0,
            instance: 0,
            object_generation: 0,
            object_guid: 0,
        }
    }
    fn projector() -> Projector {
        let mut projector = Projector::default();
        projector
            .event(context(0), "logical_birth", "1 mt_engine 1 1 0x1")
            .unwrap();
        let state = Mt19937::default().into_state();
        let text = format!(
            "0x1 {} {}",
            state
                .words
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            state.index
        );
        projector.event(context(0), "mt_state", &text).unwrap();
        projector
    }
    fn shuffle(mut projector: Projector, after: [u64; 3]) -> Result<Projector> {
        projector.event(
            context(1),
            "rng_shuffle_begin",
            "1 1 Pools/PoolManager.cpp 248 PoolGroup<Creature>::RollOne",
        )?;
        for (index, guid) in [7, 8, 9].into_iter().enumerate() {
            projector.event(
                context(1),
                "rng_shuffle_member",
                &format!("1 before {index} {guid}"),
            )?;
        }
        projector.event(context(1), "mt_raw", "0x1 0 3499211612")?;
        for (index, guid) in after.into_iter().enumerate() {
            projector.event(
                context(1),
                "rng_shuffle_member",
                &format!("1 after {index} {guid}"),
            )?;
        }
        projector.event(context(1), "rng_shuffle_end", "1 3")?;
        Ok(projector)
    }
    #[test]
    fn numeric_shuffle_projection_matches_and_stops_at_first_changed_output() {
        let projected = shuffle(projector(), [7, 9, 8]).unwrap();
        assert_eq!(
            (projected.raw, projected.shuffled, projected.members),
            (1, 1, 3)
        );
        projected.finish().unwrap();
        assert_eq!(
            shuffle(projector(), [7, 8, 9]).err().unwrap().id,
            "execution.pool.projection_shuffle_divergence"
        );
        assert_eq!(
            projector()
                .event(context(0), "mt_raw", "0x1 0 1")
                .unwrap_err()
                .id,
            "execution.pool.projection_raw_divergence"
        );
    }
    #[test]
    fn missing_state_truncation_and_later_snapshots_never_drive_a_reseed() {
        let mut projected = Projector::default();
        assert_eq!(
            projected
                .event(context(0), "mt_raw", "0x1 0 3499211612")
                .unwrap_err()
                .id,
            "execution.pool.projection_engine_birth"
        );
        let mut projected = projector();
        projected
            .event(
                context(1),
                "rng_shuffle_begin",
                "1 1 Pools/PoolManager.cpp 248 PoolGroup<Creature>::RollOne",
            )
            .unwrap();
        assert_eq!(
            projected.finish().unwrap_err().id,
            "execution.pool.projection_truncated"
        );
        let mut projected = projector();
        projected
            .event(context(0), "mt_raw", "0x1 0 3499211612")
            .unwrap();
        let initial = Mt19937::default().into_state();
        let text = format!(
            "0x1 {} {}",
            initial
                .words
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            initial.index
        );
        assert_eq!(
            projected
                .event(context(0), "mt_state", &text)
                .unwrap_err()
                .id,
            "execution.pool.projection_state_divergence"
        );
    }
    #[test]
    fn pool_double_projection_matches_independent_gcc12_fixture_and_not_changed_results() {
        // source_distributions_gcc12.jsonl:18, seed_seq {1,2}, raw span [29,31).
        for expected in [4636478034584867041_u64, 0] {
            let mut projected = Projector::default();
            projected
                .event(context(0), "logical_birth", "1 mt_engine 1 1 0x1")
                .unwrap();
            let mut source = Mt19937::from_seed_seq(&[1, 2]);
            let initial = source.clone().into_state();
            let text = format!(
                "0x1 {} {}",
                initial
                    .words
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                initial.index
            );
            projected.event(context(0), "mt_state", &text).unwrap();
            for position in 0..29 {
                projected
                    .event(
                        context(0),
                        "mt_raw",
                        &format!("0x1 {position} {}", source.next_u32()),
                    )
                    .unwrap();
            }
            projected
                .event(
                    context(0),
                    "rng_call",
                    "rand_chance /reference/source/src/game/Pools/PoolManager.cpp 203",
                )
                .unwrap();
            for position in 29..31 {
                projected
                    .event(
                        context(0),
                        "mt_raw",
                        &format!("0x1 {position} {}", source.next_u32()),
                    )
                    .unwrap();
            }
            let result = projected.event(
                context(0),
                "rng_result",
                &format!("rand_chance {}", f64::from_bits(expected)),
            );
            if expected != 0 {
                result.unwrap();
                assert_eq!((projected.rolls, projected.raw), (1, 31));
            } else {
                assert_eq!(
                    result.unwrap_err().id,
                    "execution.pool.projection_roll_divergence"
                );
            }
        }
    }
    #[test]
    fn retained_pool_numeric_projection_or_explicit_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path = explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08"));
        if explicit.is_none() && !path.try_exists().unwrap() {
            println!("SKIP unsealed P0 pool numeric projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_pools(&path).unwrap();
        assert!(report.pool_shuffles_matched > 0 && report.mt_raw_words_checked > 0);
        assert_eq!(report.independently_loaded_pool_selections_matched, 0);
        assert_eq!(report.mp_transitions_matched, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("SKIP MP/full P0 pool selection comparison: no bound effective lists, CanSpawn outputs or replacement state");
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
