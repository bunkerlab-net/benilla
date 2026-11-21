// SPDX-License-Identifier: GPL-3.0-or-later
// List insertion and rehash derived from GNU libstdc++ 14 bits/hashtable.h.
// Copyright (C) 2007-2024 Free Software Foundation, Inc.
// Original GPL notice and GCC Runtime Library Exception 3.1 are reproduced
// in ../GCC-RUNTIME-NOTICE.txt. This source port remains GPL-3.0-or-later.
//! Private, bounded GCC 14 libstdc++ `unordered_set<uint16>` reference order.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f swaps References with a
//! fresh empty set, then inserts retained MODR values in group/record order.
//! Only that sequence is modeled: identity hash, modulo range hash, the pinned
//! prime-growth schedule, maximum load factor one, and duplicate rejection.
//! There is no reserve, erase, clear, alternate profile, or arbitrary rehash API.
//! This is an explicit compatibility profile, not C++ portable set semantics.
//!
//! Each key indexes its own next link. Buckets hold predecessor links, exactly
//! as in libstdc++; membership is an 8 KiB bitmap. An insertion never scans a
//! chain. Each rehash visits the old global list once; the fixed growth schedule
//! bounds total rehash work. Interrupted mutation invalidates the whole state.

use std::fmt;

pub(crate) const POLICY: &str = "gcc14-libstdcxx-u16-identity-prime-load1-fresh-insert-v1";

const KEY_COUNT: usize = u16::MAX as usize + 1;
const MAX_BUCKETS: usize = 131_072;
const HEAD: u32 = KEY_COUNT as u32;
const END: u32 = HEAD + 1;
const ALLOCATION_POLL_KEYS: usize = 1_024;

// Measured from the guarded GCC 14.2.0 source oracle, checking bucket_count at
// EVERY cardinality through 65,536. These are (first distinct count, buckets),
// not a guessed prime table; bucket_schedule.tsv is the independent receipt.
const GROWTH: &[(usize, usize)] = &[
    (0, 1),
    (1, 13),
    (14, 29),
    (30, 59),
    (60, 127),
    (128, 257),
    (258, 541),
    (542, 1109),
    (1110, 2357),
    (2358, 5087),
    (5088, 10273),
    (10274, 20753),
    (20754, 42043),
    (42044, 85229),
];

// Validate this one fixed profile once, never parse a fixture during insertion.
// The complete domain needs fewer than 2*KEY_COUNT node visits in rehashes and
// fewer than 4*KEY_COUNT bucket initializations over the state's whole lifetime.
const _: () = {
    assert!(GROWTH.len() > 1);
    assert!(GROWTH[0].0 == 0 && GROWTH[0].1 == 1);
    let mut index = 1;
    let mut rehash_nodes = 0;
    let mut initialized_buckets = 1;
    while index < GROWTH.len() {
        let (at, buckets) = GROWTH[index];
        assert!(at > 0 && at <= KEY_COUNT);
        assert!(buckets >= at && buckets <= MAX_BUCKETS);
        assert!(buckets > GROWTH[index - 1].1);
        if index == 1 {
            assert!(at == 1);
        } else {
            assert!(at == GROWTH[index - 1].1 + 1);
        }
        rehash_nodes += at - 1;
        initialized_buckets += buckets;
        index += 1;
    }
    assert!(GROWTH[GROWTH.len() - 1].1 >= KEY_COUNT);
    assert!(rehash_nodes < 2 * KEY_COUNT);
    assert!(initialized_buckets < 4 * KEY_COUNT);
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceOrderError {
    Allocation,
    Cancelled,
    InvalidState,
}

impl fmt::Display for ReferenceOrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Allocation => "vmap.doodad_reference_order_allocation",
            Self::Cancelled => "vmap.doodad_reference_order_cancelled",
            Self::InvalidState => "vmap.doodad_reference_order_invalid_state",
        })
    }
}

impl std::error::Error for ReferenceOrderError {}

/// Always starts after the source's fresh-empty swap. This type cannot express
/// a reused bucket allocation, changed load factor, or non-insertion operation.
pub(crate) struct ReferenceOrder {
    next: Vec<u32>,
    buckets: Vec<u32>,
    present: [u64; KEY_COUNT / 64],
    head: u32,
    len: usize,
    next_growth: usize,
    failed: bool,
}

impl ReferenceOrder {
    pub(crate) fn fresh(cancelled: &dyn Fn() -> bool) -> Result<Self, ReferenceOrderError> {
        check_cancel(cancelled)?;
        Ok(Self {
            next: filled_links(KEY_COUNT, cancelled)?,
            buckets: filled_links(1, cancelled)?,
            present: [0; KEY_COUNT / 64],
            head: END,
            len: 0,
            next_growth: 1,
            failed: false,
        })
    }

    /// Returns false for a duplicate without growing or changing any links.
    /// Any error invalidates this state; callers must discard it, not resume.
    pub(crate) fn insert(
        &mut self,
        key: u16,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, ReferenceOrderError> {
        if self.failed || self.len > KEY_COUNT {
            return Err(ReferenceOrderError::InvalidState);
        }
        let result = self.insert_inner(key, cancelled);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn insert_inner(
        &mut self,
        key: u16,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, ReferenceOrderError> {
        check_cancel(cancelled)?;
        let index = usize::from(key);
        let mask = 1_u64 << (index % 64);
        if self.present[index / 64] & mask != 0 {
            return Ok(false);
        }
        if self.len >= KEY_COUNT {
            return Err(ReferenceOrderError::InvalidState);
        }
        let new_len = self.len + 1;
        if let Some(&(at, bucket_count)) = GROWTH.get(self.next_growth) {
            if new_len > at {
                return Err(ReferenceOrderError::InvalidState);
            }
            if new_len == at {
                self.rehash(bucket_count, cancelled)?;
                self.next_growth += 1;
            }
        }
        // hashtable.h::_M_insert_unique_node grows BEFORE inserting the key.
        insert_bucket_front(
            &mut self.next,
            &mut self.head,
            &mut self.buckets,
            u32::from(key),
        )?;
        self.present[index / 64] |= mask;
        self.len = new_len;
        Ok(true)
    }

    fn rehash(
        &mut self,
        bucket_count: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), ReferenceOrderError> {
        let mut buckets = filled_links(bucket_count, cancelled)?;
        let mut cursor = self.head;
        self.head = END;
        // hashtable.h::_M_rehash(..., true_type) walks OLD global order and
        // bucket-front inserts each old node. Save its old successor first.
        for _ in 0..self.len {
            check_cancel(cancelled)?;
            if cursor >= HEAD {
                return Err(ReferenceOrderError::InvalidState);
            }
            let next = self.next[cursor as usize];
            insert_bucket_front(&mut self.next, &mut self.head, &mut buckets, cursor)?;
            cursor = next;
        }
        if cursor != END {
            return Err(ReferenceOrderError::InvalidState);
        }
        self.buckets = buckets;
        Ok(())
    }

    /// Consumes the state, returning exactly its final global iteration order.
    /// No prefix can escape after cancellation or a malformed internal chain.
    pub(crate) fn into_order(
        self,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<u16>, ReferenceOrderError> {
        self.collect_order(cancelled)
    }

    fn collect_order(&self, cancelled: &dyn Fn() -> bool) -> Result<Vec<u16>, ReferenceOrderError> {
        if self.failed || self.len > KEY_COUNT {
            return Err(ReferenceOrderError::InvalidState);
        }
        check_cancel(cancelled)?;
        let mut order = Vec::new();
        order
            .try_reserve_exact(self.len)
            .map_err(|_| ReferenceOrderError::Allocation)?;
        let mut cursor = self.head;
        for _ in 0..self.len {
            check_cancel(cancelled)?;
            let key = u16::try_from(cursor).map_err(|_| ReferenceOrderError::InvalidState)?;
            order.push(key);
            cursor = self.next[usize::from(key)];
        }
        if cursor != END {
            return Err(ReferenceOrderError::InvalidState);
        }
        Ok(order)
    }
}

/// A bucket stores its predecessor, not its first node. New buckets prepend to
/// the global list; occupied buckets insert immediately after the predecessor.
fn insert_bucket_front(
    next: &mut [u32],
    head: &mut u32,
    buckets: &mut [u32],
    key: u32,
) -> Result<(), ReferenceOrderError> {
    if key >= HEAD || buckets.is_empty() || buckets.len() > MAX_BUCKETS {
        return Err(ReferenceOrderError::InvalidState);
    }
    let bucket_count = buckets.len();
    let bucket = key as usize % bucket_count;
    let predecessor = buckets[bucket];
    if predecessor == END {
        if *head != END {
            if *head >= HEAD {
                return Err(ReferenceOrderError::InvalidState);
            }
            buckets[*head as usize % bucket_count] = key;
        }
        next[key as usize] = *head;
        *head = key;
        buckets[bucket] = HEAD;
    } else if predecessor == HEAD {
        if *head >= HEAD {
            return Err(ReferenceOrderError::InvalidState);
        }
        next[key as usize] = *head;
        *head = key;
    } else if predecessor < HEAD {
        let successor = next[predecessor as usize];
        if successor >= HEAD {
            return Err(ReferenceOrderError::InvalidState);
        }
        next[key as usize] = successor;
        next[predecessor as usize] = key;
    } else {
        return Err(ReferenceOrderError::InvalidState);
    }
    Ok(())
}

fn filled_links(
    count: usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<u32>, ReferenceOrderError> {
    if count == 0 || count > MAX_BUCKETS {
        return Err(ReferenceOrderError::InvalidState);
    }
    check_cancel(cancelled)?;
    let mut links = Vec::new();
    links
        .try_reserve_exact(count)
        .map_err(|_| ReferenceOrderError::Allocation)?;
    while links.len() < count {
        check_cancel(cancelled)?;
        links.resize(count.min(links.len() + ALLOCATION_POLL_KEYS), END);
    }
    Ok(links)
}

fn check_cancel(cancelled: &dyn Fn() -> bool) -> Result<(), ReferenceOrderError> {
    if cancelled() {
        Err(ReferenceOrderError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        collections::{BTreeMap, BTreeSet},
        fs,
        path::PathBuf,
    };

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-geometry-tests/tools/fixtures/wmo_doodad_order_source")
            .join(name)
    }

    fn rows(text: &str) -> Vec<Vec<&str>> {
        text.lines()
            .filter(|row| !row.is_empty() && !row.starts_with('#'))
            .map(|row| row.split('\t').collect())
            .collect()
    }

    fn words(name: &str) -> Vec<u16> {
        let bytes = fs::read(fixture(name)).unwrap();
        let (words, remainder) = bytes.as_chunks::<2>();
        assert!(remainder.is_empty());
        words.iter().map(|word| u16::from_le_bytes(*word)).collect()
    }

    fn number(value: &str) -> usize {
        value.parse().unwrap()
    }

    fn profile() -> BTreeMap<String, String> {
        let text = fs::read_to_string(fixture("container_profile.tsv")).unwrap();
        let mut profile = BTreeMap::new();
        for row in rows(&text) {
            assert_eq!(row.len(), 2);
            assert!(profile
                .insert(row[0].to_owned(), row[1].to_owned())
                .is_none());
        }
        profile
    }

    fn populated(count: u16) -> ReferenceOrder {
        let mut order = ReferenceOrder::fresh(&|| false).unwrap();
        for key in 0..count {
            assert_eq!(order.insert(key, &|| false), Ok(true));
        }
        order
    }

    #[test]
    fn profile_has_one_fresh_state_and_bounded_storage() {
        assert_eq!(
            POLICY,
            "gcc14-libstdcxx-u16-identity-prime-load1-fresh-insert-v1"
        );
        let order = ReferenceOrder::fresh(&|| false).unwrap();
        assert_eq!(order.buckets, [END]);
        assert_eq!(order.next.len(), KEY_COUNT);
        assert_eq!(std::mem::size_of_val(&order.present), 8 * 1024);
        assert_eq!(order.into_order(&|| false), Ok(Vec::new()));
        assert_eq!(
            filled_links(MAX_BUCKETS + 1, &|| false),
            Err(ReferenceOrderError::InvalidState)
        );
        assert_eq!(
            filled_links(0, &|| false),
            Err(ReferenceOrderError::InvalidState)
        );
    }

    #[test]
    fn occupied_bucket_insertion_is_not_global_reverse_or_sorted() {
        let mut order = ReferenceOrder::fresh(&|| false).unwrap();
        for key in [0, 13, 26, 1, 14] {
            assert_eq!(order.insert(key, &|| false), Ok(true));
        }
        let next = order.next.clone();
        let buckets = order.buckets.clone();
        for key in [13, 0, 14, 26, 1] {
            assert_eq!(order.insert(key, &|| false), Ok(false));
        }
        assert_eq!(order.next, next);
        assert_eq!(order.buckets, buckets);
        assert_eq!(order.into_order(&|| false), Ok(vec![14, 1, 26, 13, 0]));
    }

    #[test]
    fn original_fixture_profile_and_fresh_empty_swaps_are_pinned() {
        let profile = profile();
        for (key, value) in [
            ("GLIBCXX_RELEASE", "14"),
            ("GLIBCXX_DATE", "20250315"),
            ("cplusplus", "201703"),
            ("sizeof_uint16", "2"),
            ("max_load_factor", "1"),
            ("hash_identity_checked_keys", "65536"),
            ("bucket_capacity_bound", "131072"),
            ("maximum_observed_bucket_count", "85229"),
            ("measured_cardinalities", "65537"),
            ("full_permutation_growth_checks", "262144"),
        ] {
            assert_eq!(profile[key], value, "{key}");
        }
        let swaps = fs::read_to_string(fixture("swap_checks.tsv")).unwrap();
        let swaps = rows(&swaps);
        assert_eq!(swaps.len(), number(&profile["cases"]));
        let mut populated_roots = 0;
        for row in swaps {
            assert_eq!(row.len(), 7);
            assert_eq!(row[3], "0", "{} reset size", row[0]);
            assert_eq!(row[4], "1", "{} reset buckets", row[0]);
            assert_eq!(row[5], row[1], "{} displaced size", row[0]);
            assert_eq!(row[6], row[2], "{} displaced buckets", row[0]);
            if number(row[1]) != 0 {
                assert_eq!(number(row[1]), KEY_COUNT);
                populated_roots += 1;
            }
        }
        assert_eq!(populated_roots, 3);
    }

    #[test]
    fn measured_growth_schedule_covers_the_entire_u16_domain() {
        let fixture = fs::read_to_string(fixture("bucket_schedule.tsv")).unwrap();
        let measured: Vec<_> = rows(&fixture)
            .into_iter()
            .map(|row| {
                assert_eq!(row.len(), 2);
                (number(row[0]), number(row[1]))
            })
            .collect();
        assert_eq!(GROWTH, measured);
        let mut state = ReferenceOrder::fresh(&|| false).unwrap();
        let mut observed = vec![(0, state.buckets.len())];
        for key in 0..=u16::MAX {
            let old_bucket_count = state.buckets.len();
            assert_eq!(state.insert(key, &|| false), Ok(true));
            if old_bucket_count != state.buckets.len() {
                observed.push((state.len, state.buckets.len()));
            }
            assert!(state.buckets.len() <= MAX_BUCKETS);
        }
        assert_eq!(observed, measured);
        let next_growth = state.next_growth;
        for key in 0..=u16::MAX {
            assert_eq!(state.insert(key, &|| false), Ok(false));
        }
        assert_eq!(state.next_growth, next_growth);
        assert_eq!(state.into_order(&|| false).unwrap().len(), KEY_COUNT);
    }

    #[test]
    fn duplicate_at_every_growth_boundary_preserves_pending_transition() {
        let mut state = ReferenceOrder::fresh(&|| false).unwrap();
        // The initial 0 -> 1 transition has no existing key to duplicate.
        for (growth_index, &(at, bucket_count)) in GROWTH.iter().enumerate().skip(2) {
            while state.len < at - 1 {
                let key = u16::try_from(state.len).unwrap();
                assert_eq!(state.insert(key, &|| false), Ok(true));
            }
            assert_eq!(state.next_growth, growth_index);
            let next = state.next.clone();
            let buckets = state.buckets.clone();
            let present = state.present;
            let head = state.head;
            let len = state.len;
            for key in [0, u16::try_from(at - 2).unwrap()] {
                assert_eq!(state.insert(key, &|| false), Ok(false));
                assert_eq!(state.next, next, "links before {at}");
                assert_eq!(state.buckets, buckets, "buckets before {at}");
                assert_eq!(state.present, present, "membership before {at}");
                assert_eq!(state.head, head, "head before {at}");
                assert_eq!(state.len, len, "size before {at}");
                assert_eq!(state.next_growth, growth_index, "growth before {at}");
            }
            assert_eq!(
                state.insert(u16::try_from(at - 1).unwrap(), &|| false),
                Ok(true)
            );
            assert_eq!(state.len, at);
            assert_eq!(state.buckets.len(), bucket_count);
            assert_eq!(state.next_growth, growth_index + 1);
        }
    }

    #[test]
    fn original_source_iteration_fixtures_match_every_checkpoint() {
        let cases = fs::read_to_string(fixture("cases.tsv")).unwrap();
        let cases = rows(&cases);
        let checkpoints = fs::read_to_string(fixture("checkpoints.tsv")).unwrap();
        let checkpoints = rows(&checkpoints);
        let insertions = words("insertions.u16le");
        let iterations = words("iterations.u16le");
        let profile = profile();
        assert_eq!(cases.len(), number(&profile["cases"]));
        assert_eq!(checkpoints.len(), number(&profile["checkpoints"]));
        assert_eq!(
            insertions.len(),
            number(&profile["literal_input_u16_count"])
        );
        assert_eq!(
            iterations.len(),
            number(&profile["literal_iteration_u16_count"])
        );
        let mut case_names = BTreeSet::new();
        for case in &cases {
            assert_eq!(case.len(), 4);
            assert!(case_names.insert(case[0]), "duplicate case {}", case[0]);
        }
        for row in &checkpoints {
            assert_eq!(row.len(), 6);
            assert!(case_names.contains(row[0]), "orphan checkpoint {}", row[0]);
        }
        let mut consumed = vec![false; checkpoints.len()];
        let mut checked = 0;
        let mut full_domain_cases = 0;
        for case in cases {
            assert_eq!(case.len(), 4);
            let offset = number(case[1]);
            let count = number(case[2]);
            // The source prepopulates the OLD set before swapping with empty.
            // fresh() intentionally has no way to inherit that capacity.
            assert!(number(case[3]) <= KEY_COUNT);
            let input = &insertions[offset..offset + count];
            let mut expected = checkpoints
                .iter()
                .enumerate()
                .filter(|(_, row)| row[0] == case[0])
                .peekable();
            assert!(expected.peek().is_some(), "{} no checkpoints", case[0]);
            let mut state = ReferenceOrder::fresh(&|| false).unwrap();
            for inserted in 0..=input.len() {
                while expected
                    .peek()
                    .is_some_and(|(_, row)| number(row[1]) == inserted)
                {
                    let (row_index, row) = expected.next().unwrap();
                    assert!(!consumed[row_index]);
                    consumed[row_index] = true;
                    assert_eq!(row.len(), 6);
                    assert_eq!(state.len, number(row[2]), "{} {inserted}", case[0]);
                    assert_eq!(
                        state.buckets.len(),
                        number(row[3]),
                        "{} {inserted}",
                        case[0]
                    );
                    let at = number(row[4]);
                    let len = number(row[5]);
                    let actual = state.collect_order(&|| false).unwrap();
                    assert_eq!(actual, iterations[at..at + len], "{} {inserted}", case[0]);
                    checked += 1;
                }
                if let Some(&key) = input.get(inserted) {
                    state.insert(key, &|| false).unwrap();
                }
            }
            assert!(expected.next().is_none(), "{} missing checkpoint", case[0]);
            if state.len == KEY_COUNT {
                full_domain_cases += 1;
            }
        }
        assert_eq!(checked, checkpoints.len());
        assert!(consumed.into_iter().all(|was_consumed| was_consumed));
        assert_eq!(full_domain_cases, 4);
    }

    #[test]
    fn cancellation_during_construction_returns_no_state() {
        let polls = Cell::new(0);
        let result = ReferenceOrder::fresh(&|| {
            polls.set(polls.get() + 1);
            polls.get() == 4
        });
        assert!(matches!(result, Err(ReferenceOrderError::Cancelled)));
        assert_eq!(polls.get(), 4);
    }

    #[test]
    fn cancellation_during_insertion_invalidates_state() {
        let mut state = populated(4);
        assert_eq!(
            state.insert(4, &|| true),
            Err(ReferenceOrderError::Cancelled)
        );
        assert_eq!(
            state.insert(4, &|| false),
            Err(ReferenceOrderError::InvalidState)
        );
        assert_eq!(
            state.into_order(&|| false),
            Err(ReferenceOrderError::InvalidState)
        );
    }

    #[test]
    fn cancellation_inside_rehash_cannot_expose_partial_order() {
        let mut state = populated(13);
        let polls = Cell::new(0);
        let result = state.insert(13, &|| {
            polls.set(polls.get() + 1);
            polls.get() == 5
        });
        assert_eq!(result, Err(ReferenceOrderError::Cancelled));
        assert_eq!(polls.get(), 5);
        assert!(state.failed);
        assert_eq!(
            state.insert(13, &|| false),
            Err(ReferenceOrderError::InvalidState)
        );
        assert_eq!(
            state.into_order(&|| false),
            Err(ReferenceOrderError::InvalidState)
        );
    }

    #[test]
    fn cancellation_during_large_bucket_initialization_precedes_link_mutation() {
        let mut state = populated(42_043);
        let next = state.next.clone();
        let buckets = state.buckets.clone();
        let head = state.head;
        let present = state.present;
        let next_growth = state.next_growth;
        let polls = Cell::new(0);
        // Growing to 85,229 buckets requires 84 initialization chunks. Poll 10
        // cancels inside that initialization, before any old node is relinked.
        let result = state.insert(42_043, &|| {
            polls.set(polls.get() + 1);
            polls.get() == 10
        });
        assert_eq!(result, Err(ReferenceOrderError::Cancelled));
        assert_eq!(polls.get(), 10);
        assert_eq!(state.next, next);
        assert_eq!(state.buckets, buckets);
        assert_eq!(state.head, head);
        assert_eq!(state.present, present);
        assert_eq!(state.len, 42_043);
        assert_eq!(state.next_growth, next_growth);
        assert!(state.failed);
        assert_eq!(
            state.into_order(&|| false),
            Err(ReferenceOrderError::InvalidState)
        );
    }

    #[test]
    fn cancellation_inside_final_iteration_discards_prefix() {
        let state = populated(13);
        let polls = Cell::new(0);
        let result = state.into_order(&|| {
            polls.set(polls.get() + 1);
            polls.get() == 5
        });
        assert_eq!(result, Err(ReferenceOrderError::Cancelled));
        assert_eq!(polls.get(), 5);
    }
}
