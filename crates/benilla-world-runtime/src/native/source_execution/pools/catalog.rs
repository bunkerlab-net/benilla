// SPDX-License-Identifier: GPL-3.0-or-later
//! Reference-environment pool query order, separate from export occurrence order.
use super::*;
use std::collections::VecDeque;

// db-snapshot.py:9 restores --order-by-primary; pool-startup/order-analysis.json:5-45 matches all 28,865 rows.
// [INFERENCE] MyISAM ALL scan. EXPLAIN blocked by tool policy; spawn-row-order/pool_*-status.tsv:2 has Data_free=0.

pub(super) struct StartupPoolRows {
    rows: PoolCatalogRows,
    templates: Option<VecDeque<(u32, PoolTemplateRow)>>,
    members: VecDeque<(Option<u32>, PoolMembershipRow)>,
    slot: usize,
}
impl StartupPoolRows {
    pub(super) fn new(rows: PoolCatalogRows) -> Self {
        Self {
            rows,
            templates: None,
            members: VecDeque::new(),
            slot: 0,
        }
    }
}
impl PoolRows for StartupPoolRows {
    fn maximum(&mut self) -> Result<Option<u16>> {
        Ok(self.rows.maximum()?)
    }
    fn template(&mut self) -> Result<Option<PoolTemplateRow>> {
        if self.templates.is_none() {
            let mut rows = Vec::new();
            while let Some(row) = self.rows.template()? {
                rows.push((
                    row.id,
                    PoolTemplateRow {
                        ordinal: row.ordinal,
                        id: row.id as u16,
                        limit: row.limit,
                        description: row.description,
                    },
                ));
            }
            rows.sort_unstable_by_key(|(entry, _)| *entry);
            self.templates = Some(rows.into());
        }
        Ok(self
            .templates
            .as_mut()
            .unwrap()
            .pop_front()
            .map(|(_, row)| row))
    }
    fn member(&mut self, slot: usize) -> Result<Option<PoolMembershipRow>> {
        if slot != self.slot {
            require(self.members.is_empty(), "execution.pool.loader_phase")?;
            let mut rows = Vec::new();
            while let Some(row) = self.rows.member(slot)? {
                rows.push((
                    row.template_id,
                    PoolMembershipRow {
                        ordinal: row.ordinal,
                        guid: row.guid,
                        pool: row.pool,
                        chance: row.chance,
                    },
                ));
            }
            rows.sort_unstable_by_key(|(entry, row)| (*entry, row.guid));
            self.members = rows.into();
            self.slot = slot;
        }
        Ok(self.members.pop_front().map(|(_, row)| row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_pool_queries_match_independent_recorded_order() {
        let (Some(root), Some(occurrences)) = (
            std::env::var_os("BENILLA_F2_CATALOG"),
            std::env::var_os("BENILLA_F2_POOL_OCCURRENCES"),
        ) else {
            println!("SKIP pool startup query order: bind catalog and pool occurrences");
            return;
        };
        let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
        let mut rows = StartupPoolRows::new(
            catalog
                .execution_pool_rows(Path::new(&occurrences))
                .unwrap(),
        );
        assert_eq!(rows.maximum().unwrap(), Some(50003));
        let mut bytes = Vec::new();
        let mut count = 0;
        while let Some(row) = rows.template().unwrap() {
            bytes.extend_from_slice(&row.id.to_le_bytes());
            bytes.extend_from_slice(&row.limit.to_le_bytes());
            bytes.extend_from_slice(&(row.description.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&row.description);
            count += 1;
        }
        assert_eq!(count, 4096);
        // Frozen trace:21239822 onward; pool-startup/typed-row-digests.json documents encoding.
        assert_eq!(
            Sha256Id::of(&bytes),
            Sha256Id::parse("ffa7d7afe6a94102313ca2a8000f800526f91131813e1be1a958ffa3def31692")
                .unwrap(),
        );
        // Expected digests come from recorded SQL fields, never from the sorted adapter.
        for (slot, expected_count, expected) in [
            (
                2,
                156,
                "3e0892626e489beaa7711b4bed6c088db026ff00e53921c63db1ad277aebfa33",
            ),
            (
                3,
                722,
                "c3191e04d4bab091a4bf267f3e28d9654f3466c7711bc6d317cb187cfa5ef268",
            ),
            (
                4,
                20081,
                "42bd8b48632e9f166c933ebd6c4335fe32a2287094d8953432cf06f95f264837",
            ),
            (
                5,
                824,
                "89b173ff3f23c70ab2ff716d6f44a493dedb623ad9a56fd7a6b74618702dada9",
            ),
            (
                6,
                2985,
                "d0af906edbcc750d94beb95c1c6e4f7ceba3c68742a5f52957ee45c4e9b28e53",
            ),
        ] {
            bytes.clear();
            count = 0;
            while let Some(row) = rows.member(slot).unwrap() {
                bytes.extend_from_slice(&row.guid.to_le_bytes());
                bytes.extend_from_slice(&row.pool.to_le_bytes());
                bytes.extend_from_slice(&row.chance.to_bits().to_le_bytes());
                count += 1;
            }
            assert_eq!(count, expected_count, "query slot {slot}");
            assert_eq!(
                Sha256Id::of(&bytes),
                Sha256Id::parse(expected).unwrap(),
                "query slot {slot}"
            );
        }
    }
}
