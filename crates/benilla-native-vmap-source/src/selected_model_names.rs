// SPDX-License-Identifier: GPL-3.0-or-later
//! Private, bounded selected-model `std::set<std::string>` byte ordering.
//!
//! The full source name is the key: case, path, extension and an optional single
//! terminal NUL all participate in identity and ordering. Empty names and NULs
//! anywhere except that terminal position are an explicit narrower envelope.
//! Names are borrowed, never normalized, decoded, opened or copied into strings.
//!
//! Stable LSD radix sorting uses 257 bins: missing bytes precede actual NULs,
//! which precede bytes 1..=255. At most 500 passes visit at most 32,768 names.
//! Every input is cancellation-polled; sorting and scratch initialization poll
//! at most 1,024 scalar steps apart. Container allocation/hash operations retain
//! the fixed input bounds but cannot be interrupted inside the standard library.

use std::collections::HashSet;
use std::fmt;

const MAX_NAMES: usize = 32_768;
const MAX_NAME_BYTES: usize = 500;
const RADIX_BINS: usize = 257;
const POLL_STEPS: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectedModelNamesError {
    Cancelled,
    Limit,
    Allocation,
    Name,
}

impl fmt::Display for SelectedModelNamesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.selected_model_names_cancelled",
            Self::Limit => "vmap.selected_model_names_limit",
            Self::Allocation => "vmap.selected_model_names_allocation",
            Self::Name => "vmap.selected_model_names_name",
        })
    }
}

impl std::error::Error for SelectedModelNamesError {}

/// Deduplicates exact full byte names, retaining the first borrowed slice for
/// each key, then returns the source's lexicographic unsigned-byte order.
/// Limits apply to distinct names and full name lengths, including terminal NUL.
/// An error returns no partial selection. The caller bounds input occurrences;
/// repeated names do not consume the distinct-name budget.
pub(crate) fn ordered_unique<'a>(
    names: impl IntoIterator<Item = &'a [u8]>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<&'a [u8]>, SelectedModelNamesError> {
    ordered_unique_bounded(names, cancelled, MAX_NAMES)
}

fn ordered_unique_bounded<'a>(
    names: impl IntoIterator<Item = &'a [u8]>,
    cancelled: &dyn Fn() -> bool,
    limit: usize,
) -> Result<Vec<&'a [u8]>, SelectedModelNamesError> {
    check_cancel(cancelled)?;
    let limit = limit.min(MAX_NAMES);
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    let mut width = 0;
    for name in names {
        check_cancel(cancelled)?;
        validate_name(name)?;
        if seen.contains(name) {
            continue;
        }
        if selected.len() >= limit {
            return Err(SelectedModelNamesError::Limit);
        }
        seen.try_reserve(1)
            .map_err(|_| SelectedModelNamesError::Allocation)?;
        selected
            .try_reserve(1)
            .map_err(|_| SelectedModelNamesError::Allocation)?;
        seen.insert(name);
        selected.push(name);
        width = width.max(name.len());
    }
    drop(seen);
    let mut work = SortWork::new(cancelled);
    radix_order(&mut selected, width, &mut work)?;
    Ok(selected)
}

fn validate_name(name: &[u8]) -> Result<(), SelectedModelNamesError> {
    if name.len() > MAX_NAME_BYTES {
        return Err(SelectedModelNamesError::Limit);
    }
    let body = name.strip_suffix(&[0]).unwrap_or(name);
    if body.is_empty() || body.contains(&0) {
        return Err(SelectedModelNamesError::Name);
    }
    Ok(())
}

fn radix_order(
    selected: &mut Vec<&[u8]>,
    width: usize,
    work: &mut SortWork<'_>,
) -> Result<(), SelectedModelNamesError> {
    let mut offsets = [0_usize; RADIX_BINS];
    check_cancel(work.cancelled)?;
    if selected.len() < 2 {
        return Ok(());
    }
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(selected.len())
        .map_err(|_| SelectedModelNamesError::Allocation)?;
    for _ in 0..selected.len() {
        work.step()?;
        scratch.push(&[][..]);
    }
    for position in (0..width).rev() {
        for offset in &mut offsets {
            work.step()?;
            *offset = 0;
        }
        for name in selected.iter() {
            work.step()?;
            offsets[digit(name, position)] += 1;
        }
        let mut start = 0;
        for offset in &mut offsets {
            work.step()?;
            let count = *offset;
            *offset = start;
            start += count;
        }
        for &name in selected.iter() {
            work.step()?;
            let offset = &mut offsets[digit(name, position)];
            scratch[*offset] = name;
            *offset += 1;
        }
        work.step()?;
        std::mem::swap(selected, &mut scratch);
    }
    check_cancel(work.cancelled)
}

fn digit(name: &[u8], position: usize) -> usize {
    name.get(position).map_or(0, |byte| usize::from(*byte) + 1)
}

struct SortWork<'a> {
    cancelled: &'a dyn Fn() -> bool,
    since_poll: usize,
    #[cfg(test)]
    steps: usize,
}

impl<'a> SortWork<'a> {
    fn new(cancelled: &'a dyn Fn() -> bool) -> Self {
        Self {
            cancelled,
            since_poll: 0,
            #[cfg(test)]
            steps: 0,
        }
    }

    fn step(&mut self) -> Result<(), SelectedModelNamesError> {
        if self.since_poll == POLL_STEPS {
            check_cancel(self.cancelled)?;
            self.since_poll = 0;
        }
        self.since_poll += 1;
        #[cfg(test)]
        {
            self.steps += 1;
        }
        Ok(())
    }
}

fn check_cancel(cancelled: &dyn Fn() -> bool) -> Result<(), SelectedModelNamesError> {
    if cancelled() {
        Err(SelectedModelNamesError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeSet;

    fn source_set<'a>(names: impl IntoIterator<Item = &'a [u8]>) -> Vec<&'a [u8]> {
        names
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[test]
    fn full_source_bytes_survive_without_case_path_or_extension_normalization() {
        let names: &[&[u8]] = &[
            b"World\\Tree.M2",
            b"world\\tree.m2",
            b"World/Tree.M2",
            b"World\\Tree.mdx",
            b"World\\Tree.M2\0",
            b"World\\Tree.M2",
            b"\xff.m2",
            b"\x80.m2\0",
        ];
        let actual = ordered_unique(names.iter().copied(), &|| false).unwrap();
        assert_eq!(actual, source_set(names.iter().copied()));
        assert_eq!(actual.len(), 7);
    }

    #[test]
    fn absent_byte_sorts_before_retained_terminal_nul_and_other_bytes() {
        let names: &[&[u8]] = &[b"ab", b"a\0", b"a\x01", b"a", b"a\xff", b"A\0"];
        assert_eq!(
            ordered_unique(names.iter().copied(), &|| false).unwrap(),
            [
                &b"A\0"[..],
                &b"a"[..],
                &b"a\0"[..],
                &b"a\x01"[..],
                &b"ab"[..],
                &b"a\xff"[..],
            ]
        );
        assert_eq!(digit(b"a", 1), 0);
        assert_eq!(digit(b"a\0", 1), 1);
        assert_eq!(digit(b"a\xff", 1), 256);
    }

    #[test]
    fn duplicate_retains_first_borrow_and_does_not_consume_limit() {
        let first = b"same.m2".to_vec();
        let duplicate = first.clone();
        let names = [first.as_slice(), duplicate.as_slice(), first.as_slice()];
        let actual = ordered_unique_bounded(names, &|| false, 1).unwrap();
        assert_eq!(actual.len(), 1);
        assert!(std::ptr::eq(actual[0], first.as_slice()));
        assert_eq!(
            ordered_unique_bounded([first.as_slice(), b"other.m2"], &|| false, 1),
            Err(SelectedModelNamesError::Limit)
        );
        assert_eq!(
            ordered_unique_bounded([first.as_slice(), b"same.m2\0"], &|| false, 1),
            Err(SelectedModelNamesError::Limit)
        );
    }

    #[test]
    fn empty_selection_is_valid_but_empty_or_embedded_nul_names_are_rejected() {
        assert!(ordered_unique([], &|| false).unwrap().is_empty());
        for name in [&b""[..], b"\0", b"a\0b", b"a\0\0", b"\0a", b"\0\0"] {
            assert_eq!(
                ordered_unique([name], &|| false),
                Err(SelectedModelNamesError::Name),
                "{name:?}"
            );
        }
        assert!(ordered_unique_bounded([], &|| false, 0).unwrap().is_empty());
        assert_eq!(
            ordered_unique_bounded([&b"a"[..]], &|| false, 0),
            Err(SelectedModelNamesError::Limit)
        );
    }

    #[test]
    fn byte_length_limit_includes_terminal_nul() {
        let plain = vec![b'a'; MAX_NAME_BYTES];
        let mut terminated = plain.clone();
        terminated[MAX_NAME_BYTES - 1] = 0;
        assert_eq!(
            ordered_unique([plain.as_slice(), terminated.as_slice()], &|| false).unwrap(),
            [terminated.as_slice(), plain.as_slice()]
        );
        let mut too_long = plain.clone();
        too_long.push(0);
        assert_eq!(
            ordered_unique([too_long.as_slice()], &|| false),
            Err(SelectedModelNamesError::Limit)
        );
        too_long[MAX_NAME_BYTES] = b'a';
        assert_eq!(
            ordered_unique([too_long.as_slice()], &|| false),
            Err(SelectedModelNamesError::Limit)
        );
    }

    fn numbered_names(count: usize, width: usize) -> Vec<Vec<u8>> {
        assert!(width >= 3 && count <= 255 * 255 * 255);
        (0..count)
            .map(|index| {
                let mut name = vec![b'x'; width];
                name[width - 3] = 1 + (index / (255 * 255)) as u8;
                name[width - 2] = 1 + ((index / 255) % 255) as u8;
                name[width - 1] = 1 + (index % 255) as u8;
                name
            })
            .collect()
    }

    #[test]
    fn actual_distinct_count_limit_allows_duplicates_but_not_one_more_key() {
        let names = numbered_names(MAX_NAMES + 1, 3);
        let at_limit = names[..MAX_NAMES].iter().map(Vec::as_slice);
        let actual =
            ordered_unique(at_limit.clone().rev().chain(at_limit.clone()), &|| false).unwrap();
        assert_eq!(actual.len(), MAX_NAMES);
        assert_eq!(actual, source_set(at_limit));
        assert_eq!(
            ordered_unique(names.iter().map(Vec::as_slice), &|| false),
            Err(SelectedModelNamesError::Limit)
        );
        assert_eq!(
            ordered_unique_bounded(names.iter().map(Vec::as_slice), &|| false, usize::MAX),
            Err(SelectedModelNamesError::Limit)
        );
    }

    #[test]
    fn every_nonzero_byte_and_terminal_nul_match_source_btree_set() {
        let names: Vec<Vec<u8>> = (1..=255)
            .flat_map(|byte| [vec![byte], vec![byte, 0], vec![b'p', byte]])
            .collect();
        for reverse in [false, true] {
            let mut input: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
            if reverse {
                input.reverse();
            }
            assert_eq!(
                ordered_unique(input.iter().copied(), &|| false).unwrap(),
                source_set(input.iter().copied())
            );
        }
    }

    #[test]
    fn varied_lengths_prefixes_duplicates_and_byte_values_match_source_btree_set() {
        let mut state = 0x6e61_6d65_7365_7431_u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 32) as u32
        };
        let mut names = Vec::new();
        for index in 0..3_000 {
            let len = 1 + next() as usize % 64;
            let mut name: Vec<u8> = (0..len).map(|_| 1 + (next() % 255) as u8).collect();
            if index % 2 == 0 {
                name[0] = b'P';
            }
            if index % 3 == 0 {
                name.push(0);
            }
            names.push(name);
        }
        let input = names.iter().rev().chain(names.iter()).map(Vec::as_slice);
        assert_eq!(
            ordered_unique(input.clone(), &|| false).unwrap(),
            source_set(input)
        );
    }

    fn assert_every_observed_poll_can_cancel(names: &[&[u8]]) {
        let polls = Cell::new(0);
        let actual = ordered_unique(names.iter().copied(), &|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        assert_eq!(actual, source_set(names.iter().copied()));
        let total = polls.get();
        for stop in 1..=total {
            polls.set(0);
            assert_eq!(
                ordered_unique(names.iter().copied(), &|| {
                    polls.set(polls.get() + 1);
                    polls.get() == stop
                }),
                Err(SelectedModelNamesError::Cancelled),
                "cancellation poll {stop} of {total}"
            );
            assert_eq!(polls.get(), stop);
        }
        polls.set(0);
        assert_eq!(
            ordered_unique(names.iter().copied(), &|| {
                polls.set(polls.get() + 1);
                polls.get() == total + 1
            })
            .unwrap(),
            actual
        );
        assert_eq!(polls.get(), total);
    }

    #[test]
    fn cancellation_at_every_observed_input_sort_and_final_poll_returns_no_selection() {
        assert_every_observed_poll_can_cancel(&[]);
        assert_every_observed_poll_can_cancel(&[b"one"]);
        assert_every_observed_poll_can_cancel(&[b"one", b"one", b"one\0"]);
        let many = numbered_names(POLL_STEPS + 1, 3);
        let input: Vec<_> = many.iter().rev().map(Vec::as_slice).collect();
        assert_every_observed_poll_can_cancel(&input);
        let longest = numbered_names(3, MAX_NAME_BYTES);
        let input: Vec<_> = longest.iter().rev().map(Vec::as_slice).collect();
        assert_every_observed_poll_can_cancel(&input);
    }

    #[test]
    fn duplicate_inputs_are_polled_individually_before_sorting() {
        let yielded = Cell::new(0);
        let input = std::iter::repeat(&b"duplicate"[..]).inspect(|_| {
            yielded.set(yielded.get() + 1);
        });
        assert_eq!(
            ordered_unique(input, &|| yielded.get() == 7),
            Err(SelectedModelNamesError::Cancelled)
        );
        assert_eq!(yielded.get(), 7);
    }

    #[test]
    fn maximum_count_and_width_have_fixed_work_and_poll_bounds() {
        let names = numbered_names(MAX_NAMES, MAX_NAME_BYTES);
        let mut selected: Vec<_> = names.iter().rev().map(Vec::as_slice).collect();
        let polls = Cell::new(0);
        let cancelled = || {
            polls.set(polls.get() + 1);
            false
        };
        let mut work = SortWork::new(&cancelled);
        radix_order(&mut selected, MAX_NAME_BYTES, &mut work).unwrap();
        // One scratch initialization; per pass: clear bins, count names,
        // prefix bins, scatter names, and swap the two borrowed-slice buffers.
        let expected_steps = MAX_NAMES + MAX_NAME_BYTES * (2 * MAX_NAMES + 2 * RADIX_BINS + 1);
        assert_eq!(work.steps, expected_steps);
        assert_eq!(polls.get(), 2 + (expected_steps - 1) / POLL_STEPS);
        assert!(work.since_poll <= POLL_STEPS);
        assert_eq!(selected.len(), MAX_NAMES);
        assert!(selected.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(selected, source_set(names.iter().map(Vec::as_slice)));
    }

    #[test]
    fn sorting_poll_interval_never_exceeds_1024_steps() {
        let polls = Cell::new(0);
        let cancelled = || {
            polls.set(polls.get() + 1);
            false
        };
        let mut work = SortWork::new(&cancelled);
        for step in 1..=3 * POLL_STEPS + 1 {
            work.step().unwrap();
            assert_eq!(polls.get(), (step - 1) / POLL_STEPS);
            assert_eq!(work.since_poll, (step - 1) % POLL_STEPS + 1);
        }
    }
}
