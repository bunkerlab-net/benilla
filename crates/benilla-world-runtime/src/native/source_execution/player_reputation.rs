// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::source_rows::{RepRow, RowDelete, RowInsert, RowOperation};
use super::*;

// ReputationMgr.h:39-52. Player's map key is the reputation list ID.
pub(super) struct ReputationFactionState {
    pub(super) faction: u32,
    pub(super) standing: i32,
    pub(super) flags: u32,
    pub(super) need_save: bool,
}

impl SourcePlayer {
    // ReputationMgr.cpp:517-535. Stage DELETE then INSERT without clearing needSave.
    pub(super) fn save_reputation(
        &self,
        guid: u32,
        observe: &mut impl FnMut(SourcePlayerSaveObservation) -> Result<()>,
    ) -> Result<()> {
        for state in self.reputation.values().filter(|state| state.need_save) {
            observe(SourcePlayerSaveObservation::Row(RowOperation::Delete(
                RowDelete::Reputation {
                    guid,
                    faction: state.faction,
                },
            )))?;
            observe(SourcePlayerSaveObservation::Row(RowOperation::Insert(
                RowInsert::Reputation(RepRow {
                    guid,
                    faction: state.faction,
                    standing: state.standing,
                    flags: i32::try_from(state.flags).map_err(|_| {
                        refuse("execution.player_save.ReputationMgr_531.flags_width")
                    })?,
                }),
            )))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_reputation_projection_uses_list_order_and_preserves_signed_delta() {
        let mut objects = SourceObjects::default();
        let mut units = SourceUnits::default();
        let mut player = super::super::tests::construct(&mut objects, &mut units);
        // ReputationMgr.cpp:525-532. The clean faction contributes no writes.
        player.reputation.extend(
            [
                (4, 21, 28, 1, true),
                (1, 40, -67, 2, true),
                (3, 59, 17, 0, false),
            ]
            .map(|(key, faction, standing, flags, need_save)| {
                (
                    key,
                    ReputationFactionState {
                        faction,
                        standing,
                        flags,
                        need_save,
                    },
                )
            }),
        );
        let mut rows = Vec::new();
        player
            .save_reputation(9, &mut |op| {
                rows.push(op);
                Ok(())
            })
            .unwrap();
        use SourcePlayerSaveObservation::Row;
        assert_eq!(
            rows,
            [
                Row(RowOperation::Delete(RowDelete::Reputation {
                    guid: 9,
                    faction: 40
                })),
                Row(RowOperation::Insert(RowInsert::Reputation(RepRow {
                    guid: 9,
                    faction: 40,
                    standing: -67,
                    flags: 2
                }))),
                Row(RowOperation::Delete(RowDelete::Reputation {
                    guid: 9,
                    faction: 21
                })),
                Row(RowOperation::Insert(RowInsert::Reputation(RepRow {
                    guid: 9,
                    faction: 21,
                    standing: 28,
                    flags: 1
                }))),
            ]
        );
        assert!(player.reputation[&1].need_save);
        assert!(!player.reputation[&3].need_save);
        assert!(player.reputation[&4].need_save);
    }
}
