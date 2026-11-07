// SPDX-License-Identifier: GPL-3.0-or-later
//! Borrowed observations from genuine immutable capture, never authority from metadata.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceOutcome<'a> {
    Present {
        winning_archive: &'a str,
        sha256: &'a str,
        bytes: usize,
    },
    Absent,
    Dummy {
        winning_archive: &'a str,
        bytes: usize,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceReceipt<'a> {
    name: &'a str,
    outcome: SourceOutcome<'a>,
}
impl<'a> SourceReceipt<'a> {
    pub(crate) fn new(name: &'a str, outcome: SourceOutcome<'a>) -> Self {
        Self { name, outcome }
    }
    pub fn name(self) -> &'a str {
        self.name
    }
    pub fn outcome(self) -> SourceOutcome<'a> {
        self.outcome
    }
}
