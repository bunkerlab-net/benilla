// SPDX-License-Identifier: GPL-3.0-or-later
//! Observed insertion-history fixtures do not establish source cell membership.
use super::retained_seal8::{parse, scan_seal8, Seal8Context, SEAL8_TRACE_HASH};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Serialize)]
pub struct Seal8UpdateSetCounts {
    pub births: u64,
    pub insertion_attempts: u64,
    pub unique_insertions: u64,
    pub duplicates: u64,
    pub rehashes: u64,
    pub traversals: u64,
    pub yields: u64,
    pub update_begins: u64,
    pub update_ends: u64,
}
#[derive(Debug, Serialize)]
pub struct Seal8UpdateSetReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub history_fixture_matches: Seal8UpdateSetCounts,
    pub independently_derived_cell_memberships_matched: u64,
    pub pending: Vec<&'static str>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
struct Life {
    pointer: u64,
    object: bool,
    update_set: bool,
}
struct History {
    set: SourceUpdateSet,
    thread: u64,
    map_update: u64,
    map: MapIdentity,
    nodes: BTreeMap<u64, u64>,
    order: Option<Vec<ObjectHandle>>,
    next: usize,
    updating: Option<u64>,
    yielded: bool,
    complete: bool,
}
fn pointer(field: Option<&str>) -> Result<u64> {
    let value = u64::from_str_radix(
        field
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.seal8_update_set.pointer"))?,
        16,
    )
    .map_err(|_| refuse("execution.seal8_update_set.pointer"))?;
    require(
        value != 0 && value.is_multiple_of(8),
        "execution.seal8_update_set.pointer_alignment",
    )?;
    Ok(value)
}
fn handle(id: u64) -> ObjectHandle {
    ObjectHandle {
        thread: id >> 32,
        construction: id & 0xffff_ffff,
    }
}
impl History {
    fn context(&self, c: Seal8Context) -> Result<()> {
        require(
            self.thread == c.thread
                && self.map_update == c.map_update
                && self.map.map == c.map
                && self.map.instance == c.instance
                && self.map.creation_generation == c.map_generation,
            "execution.seal8_update_set.context",
        )
    }
    fn expected(&self) -> Result<ObjectHandle> {
        self.order
            .as_ref()
            .and_then(|order| order.get(self.next))
            .copied()
            .ok_or_else(|| refuse("execution.seal8_update_set.extra_or_early_update"))
    }
}
/// Validate every represented mutation, lifetime token, yield and Update pair.
/// Insert records are labelled fixture operations, never gameplay visit inputs.
pub fn compare_seal8_unsealed_update_sets(root: &Path) -> Result<Seal8UpdateSetReport> {
    let mut lives = BTreeMap::<u64, Life>::new();
    let mut seen = BTreeSet::new();
    let mut pointers = BTreeMap::<u64, u64>::new();
    let mut histories = BTreeMap::<u64, History>::new();
    let mut current = BTreeMap::<u64, u64>::new();
    let mut nodes = BTreeSet::new();
    let mut counts = Seal8UpdateSetCounts::default();
    let records = scan_seal8(root, |c, kind, payload| {
        let mut fields = payload.split_whitespace();
        match kind {
            "logical_birth" => {
                let id: u64 = parse(fields.next())?;
                let kind = fields
                    .next()
                    .ok_or_else(|| refuse("execution.seal8_update_set.birth_kind"))?;
                let thread: u64 = parse(fields.next())?;
                let ordinal: u64 = parse(fields.next())?;
                let ptr = pointer(fields.next())?;
                require(
                    thread == c.thread
                        && ordinal > 0
                        && ordinal <= u64::from(u32::MAX)
                        && id == (thread << 32 | ordinal),
                    "execution.seal8_update_set.birth_identity",
                )?;
                require(
                    seen.insert(id) && !pointers.contains_key(&ptr) && !nodes.contains(&ptr),
                    "execution.seal8_update_set.overlapping_lifetime",
                )?;
                lives.insert(
                    id,
                    Life {
                        pointer: ptr,
                        object: kind == "object",
                        update_set: kind == "update_set",
                    },
                );
                pointers.insert(ptr, id);
                counts.births += 1;
            }
            "logical_death" => {
                let id: u64 = parse(fields.next())?;
                let life = lives
                    .remove(&id)
                    .ok_or_else(|| refuse("execution.seal8_update_set.unknown_death"))?;
                pointers.remove(&life.pointer);
                if let Some(history) = histories.remove(&id) {
                    require(
                        history.complete,
                        "execution.seal8_update_set.destroyed_before_traversal_end",
                    )?;
                    for token in history.nodes.values() {
                        nodes.remove(token);
                    }
                    require(
                        current.remove(&history.thread) == Some(id),
                        "execution.seal8_update_set.current_container",
                    )?;
                }
            }
            "container_begin" => {
                let id: u64 = parse(fields.next())?;
                let map: u32 = parse(fields.next())?;
                let instance: u32 = parse(fields.next())?;
                let buckets: usize = parse(fields.next())?;
                let load = fields.next();
                require(
                    lives.get(&id).is_some_and(|life| life.update_set)
                        && !histories.contains_key(&id)
                        && !current.contains_key(&c.thread)
                        && map == c.map
                        && instance == c.instance
                        && buckets == 1
                        && load == Some("0x1p+0"),
                    "execution.seal8_update_set.initial_state",
                )?;
                let map = MapIdentity {
                    map,
                    instance,
                    creation_generation: c.map_generation,
                };
                histories.insert(
                    id,
                    History {
                        set: SourceUpdateSet::new(map)?,
                        thread: c.thread,
                        map_update: c.map_update,
                        map,
                        nodes: BTreeMap::new(),
                        order: None,
                        next: 0,
                        updating: None,
                        yielded: false,
                        complete: false,
                    },
                );
                current.insert(c.thread, id);
            }
            "container_insert" => {
                let id = parse(fields.next())?;
                let ptr = pointer(fields.next())?;
                let generation = parse(fields.next())?;
                let inserted: u8 = parse(fields.next())?;
                let before: usize = parse(fields.next())?;
                let after: usize = parse(fields.next())?;
                let bucket: usize = parse(fields.next())?;
                let hash: u64 = parse(fields.next())?;
                let node = pointer(fields.next())?;
                let h = histories
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.seal8_update_set.container_missing"))?;
                h.context(c)?;
                require(
                    h.order.is_none() && !h.complete && inserted <= 1,
                    "execution.seal8_update_set.mutation_outside_collection",
                )?;
                require(
                    lives
                        .get(&generation)
                        .is_some_and(|life| life.object && life.pointer == ptr)
                        && pointers.get(&ptr) == Some(&generation),
                    "execution.seal8_update_set.stale_generation",
                )?;
                let receipt =
                    h.set
                        .insert_token(handle(generation), ptr, UpdateInsertion::Generic)?;
                require(
                    receipt.inserted == (inserted == 1)
                        && receipt.buckets_before == before
                        && receipt.buckets_after == after
                        && receipt.bucket == bucket
                        && receipt.hash == hash,
                    "execution.seal8_update_set.insert_hash_bucket_rehash",
                )?;
                if receipt.inserted {
                    require(
                        !pointers.contains_key(&node) && nodes.insert(node),
                        "execution.seal8_update_set.node_overlap",
                    )?;
                    h.nodes.insert(generation, node);
                    counts.unique_insertions += 1;
                } else {
                    require(
                        h.nodes.get(&generation) == Some(&node),
                        "execution.seal8_update_set.duplicate_node_changed",
                    )?;
                    counts.duplicates += 1;
                }
                counts.insertion_attempts += 1;
                counts.rehashes += u64::from(before != after);
            }
            "container_traversal" => {
                let id = parse(fields.next())?;
                let size: usize = parse(fields.next())?;
                let buckets: usize = parse(fields.next())?;
                let h = histories
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.seal8_update_set.container_missing"))?;
                h.context(c)?;
                require(
                    h.order.is_none() && h.set.len() == size && h.set.bucket_count() == buckets,
                    "execution.seal8_update_set.traversal_state",
                )?;
                h.order = Some(h.set.order().collect());
                counts.traversals += 1;
            }
            "object_update" => {
                let map: u32 = parse(fields.next())?;
                let ptr = pointer(fields.next())?;
                let _: u32 = parse(fields.next())?; // observed low GUID
                let _: u32 = parse(fields.next())?; // observed entry
                let _: u32 = parse(fields.next())?; // observed type
                let generation: u64 = parse(fields.next())?;
                let id = current
                    .get(&c.thread)
                    .ok_or_else(|| refuse("execution.seal8_update_set.update_without_container"))?;
                let h = histories.get_mut(id).unwrap();
                h.context(c)?;
                require(
                    map == c.map
                        && h.expected()? == handle(generation)
                        && h.updating.is_none()
                        && !h.complete
                        && lives
                            .get(&generation)
                            .is_some_and(|life| life.object && life.pointer == ptr)
                        && c.object_generation == generation,
                    "execution.seal8_update_set.update_begin_order_or_lifetime",
                )?;
                h.updating = Some(generation);
                counts.update_begins += 1;
            }
            "container_yield" => {
                let id = parse(fields.next())?;
                let generation: u64 = parse(fields.next())?;
                let bucket: usize = parse(fields.next())?;
                let h = histories
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.seal8_update_set.container_missing"))?;
                h.context(c)?;
                let life = lives
                    .get(&generation)
                    .ok_or_else(|| refuse("execution.seal8_update_set.dead_yield"))?;
                require(
                    h.expected()? == handle(generation)
                        && h.updating == Some(generation)
                        && !h.yielded
                        && life.object
                        && pointers.get(&life.pointer) == Some(&generation)
                        && (life.pointer % h.set.bucket_count() as u64) as usize == bucket,
                    "execution.seal8_update_set.pinned_yield",
                )?;
                h.yielded = true;
                counts.yields += 1;
            }
            "object_update_end" => {
                let generation: u64 = parse(fields.next())?;
                let id = current
                    .get(&c.thread)
                    .ok_or_else(|| refuse("execution.seal8_update_set.end_without_container"))?;
                let h = histories.get_mut(id).unwrap();
                h.context(c)?;
                require(
                    h.updating == Some(generation) && h.yielded,
                    "execution.seal8_update_set.update_end_pair",
                )?;
                h.updating = None;
                h.yielded = false;
                h.next += 1;
                counts.update_ends += 1;
            }
            "container_traversal_end" => {
                let id = parse(fields.next())?;
                let h = histories
                    .get_mut(&id)
                    .ok_or_else(|| refuse("execution.seal8_update_set.container_missing"))?;
                h.context(c)?;
                require(
                    !h.complete
                        && h.order.as_ref().is_some_and(|order| h.next == order.len())
                        && h.updating.is_none(),
                    "execution.seal8_update_set.truncated_traversal",
                )?;
                h.complete = true;
            }
            _ => return Ok(()),
        }
        require(
            fields.next().is_none(),
            "execution.seal8_update_set.surplus_fields",
        )
    })?;
    require(
        histories.is_empty() && current.is_empty() && nodes.is_empty(),
        "execution.seal8_update_set.truncated_container_lifetime",
    )?;
    Ok(Seal8UpdateSetReport {
        adapter: "benilla-seal8-08-unsealed-pinned-update-set-fixtures-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?, records, history_fixture_matches: counts,
        independently_derived_cell_memberships_matched: 0,
        pending: vec!["Sealed execution and native constructor/input occurrence binding",
            "Full allocation extents and source subobject lifetime joins",
            "Authoritative player/farsight/active poses, visibility and cell-registration histories",
            "Independent whole-cell membership and insertion-cause comparison",
            "Actual reached entity Update handlers and remote unload/reload scenario"],
        reference_execution_sealed: false, whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_seal8_update_set_projection_or_clean_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path =
            explicit
                .clone()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap()
                .join("benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08")
                });
        if explicit.is_none() && !path.try_exists().unwrap() {
            eprintln!(
                "SKIP unsealed seal8-08 update-set history fixtures: set BENILLA_F1_SEAL8_FRAMING"
            );
            return;
        }
        let report = compare_seal8_unsealed_update_sets(&path).unwrap();
        assert!(report.history_fixture_matches.insertion_attempts > 0);
        assert_eq!(report.independently_derived_cell_memberships_matched, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
