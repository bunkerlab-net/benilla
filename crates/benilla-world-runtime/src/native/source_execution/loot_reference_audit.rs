// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Eq, PartialEq)]
pub enum LootReferenceNotice {
    MissingId { id: u32 },
    Cycle { id: u32, active: Vec<u32> },
    TemplateIssue { kind: LootStoreKind, entry: u32 },
    UnusedId { id: u32 },
}

// LootMgr.cpp:2764-2779,2585-2599 walks flat rows, then explicit and equal rows in each group.
#[derive(Clone, Copy, Default)]
struct RowPosition {
    group: usize,
    equal: bool,
    offset: usize,
}

impl RowPosition {
    fn next_reference(&mut self, template: &SourceLootTemplate) -> Option<(Self, i32)> {
        loop {
            let rows = if self.group == 0 {
                &template.entries
            } else {
                let group = template.groups.get(self.group - 1)?;
                if self.equal {
                    &group.equal
                } else {
                    &group.explicit
                }
            };
            while let Some(row) = rows.get(self.offset) {
                let position = *self;
                self.offset += 1;
                if row.min_count_or_ref < 0 {
                    return Some((position, row.min_count_or_ref));
                }
            }
            if self.group != 0 && !self.equal {
                self.equal = true;
            } else {
                self.group += 1;
                self.equal = false;
            }
            self.offset = 0;
        }
    }

    fn disable(self, template: &mut SourceLootTemplate) {
        // The cursor already read this position. The audit never changes row or group counts.
        let rows = if self.group == 0 {
            &mut template.entries
        } else {
            let group = &mut template.groups[self.group - 1];
            if self.equal {
                &mut group.equal
            } else {
                &mut group.explicit
            }
        };
        rows[self.offset].min_count_or_ref = 0;
    }
}

struct Frame {
    kind: LootStoreKind,
    entry: u32,
    next: RowPosition,
}

impl Frame {
    fn new(kind: LootStoreKind, entry: u32) -> Self {
        Self {
            kind,
            entry,
            next: RowPosition::default(),
        }
    }
}

#[derive(Default)]
struct Traversal {
    active: BTreeSet<u32>,
    parents: Vec<Frame>,
}

impl SourceLootStores {
    // LootMgr.cpp:2977-2995 prunes reference IDs in source store order.
    pub fn check_reference_templates(
        &mut self,
        battleground_ids: &BTreeSet<u32>,
        mut notice: impl FnMut(LootReferenceNotice),
    ) -> Result<()> {
        use LootStoreKind::*;
        let mut path = Traversal::default();
        for kind in [
            Creature,
            Fishing,
            Gameobject,
            Item,
            Pickpocketing,
            Skinning,
            Disenchant,
            Mail,
            Reference,
        ] {
            self.check_store_refs(kind, &mut path, &mut notice)?;
        }
        for id in battleground_ids {
            self.unused_reference_ids.remove(id);
        }
        // LootMgr.cpp:289-298 reports the remaining IDs without removing templates.
        for &id in &self.unused_reference_ids {
            notice(LootReferenceNotice::UnusedId { id });
        }
        Ok(())
    }

    // LootMgr.cpp:271-286 starts a fresh path for each template in native hash iteration order.
    fn check_store_refs(
        &mut self,
        kind: LootStoreKind,
        path: &mut Traversal,
        notice: &mut impl FnMut(LootReferenceNotice),
    ) -> Result<()> {
        let mut next = self.store(kind).templates.first_key();
        while let Some(key) = next {
            let entry = key as u32;
            path.active.clear();
            path.parents.clear();
            if !self.check_template_refs(Frame::new(kind, entry), path, notice)? {
                notice(LootReferenceNotice::TemplateIssue { kind, entry });
            }
            next = self.store(kind).templates.next_key(key);
        }
        Ok(())
    }

    // LootMgr.cpp:74-121 descends through references and unwinds before marking a use.
    fn check_template_refs(
        &mut self,
        mut frame: Frame,
        path: &mut Traversal,
        notice: &mut impl FnMut(LootReferenceNotice),
    ) -> Result<bool> {
        loop {
            let template = self
                .store(frame.kind)
                .get_loot_for(frame.entry)
                .expect("frames only name loaded templates");
            let Some((position, operand)) = frame.next.next_reference(template) else {
                let Some(parent) = path.parents.pop() else {
                    return Ok(true);
                };
                path.active.remove(&frame.entry);
                self.unused_reference_ids.remove(&frame.entry);
                frame = parent;
                continue;
            };
            let id = operand
                .checked_neg()
                .ok_or_else(|| refuse("execution.loot.LootMgr_78.reference_negation_undefined"))?
                as u32;
            if !self.store(LootStoreKind::Reference).have_loot_for(id) {
                // LootMgr.cpp:300-303 logs missing references without changing the row.
                notice(LootReferenceNotice::MissingId { id });
                return Ok(false);
            }
            if !path.active.insert(id) {
                notice(LootReferenceNotice::Cycle {
                    id,
                    active: path.active.iter().copied().collect(),
                });
                self.unused_reference_ids.remove(&id);
                let template = self.stores[frame.kind as usize]
                    .templates
                    .get_mut(u64::from(frame.entry))
                    .expect("the audit does not erase templates");
                position.disable(template);
                return Ok(false);
            }
            path.parents.push(frame);
            frame = Frame::new(LootStoreKind::Reference, id);
        }
    }
}
