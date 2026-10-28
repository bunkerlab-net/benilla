// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-owned public entries feed the real parser/ItemQuery path, not pixel verification.
use super::*;
fn descriptor(events: &[SessionEvent], player: u64) -> benilla_protocol::ObjectFields {
    let mut out = benilla_protocol::ObjectFields::default();
    for event in events {
        if let SessionEvent::ObjectValues { guid, fields } = event {
            if *guid == player {
                out.merge(fields.clone());
            }
        }
    }
    out
}
fn visible(fields: &benilla_protocol::ObjectFields) -> Vec<(u16, u32)> {
    fields
        .raw_fields()
        .filter(|(field, _)| (258..486).contains(field))
        .collect()
}
#[test]
fn stock_visible_equipment_entry_queries_bound_display_and_survives_save_retry_reopen() {
    let h = Harness::new();
    let mut c = enter(&h, profile(0), false);
    let initial = c.initial_events().unwrap();
    let fields = descriptor(&initial, c.player_guid());
    let entries = visible(&fields);
    assert_eq!(entries.len(), 19);
    assert_eq!(fields.player_visible_item_entry(4), Some(900001));
    for slot in 0..19u8 {
        assert!(entries.contains(&(
            260 + 12 * u16::from(slot),
            if slot == 4 { 900001 } else { 0 }
        )));
    }
    let before = c.world.state_digest();
    let query = c
        .receive(ClientCommand::ItemQuery {
            entry: fields.player_visible_item_entry(4).unwrap(),
            guid: 0,
        })
        .unwrap();
    assert!(
        matches!(&query.events[..],[SessionEvent::ItemTemplate{entry:900001,info:Some(info)}] if info.display_info_id==900002 && **info==LootCatalog.item_metadata(900001).unwrap())
    );
    assert_eq!(c.world.state_digest(), before);
    assert!(c.actions.is_empty());
    c.receive(ClientCommand::SetWatchedFaction { rep_list_id: 0 })
        .unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(visible(&descriptor(&out.events, c.player_guid())), entries);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut reopened = enter(&h, profile(0), true);
    assert_eq!(reopened.world.state_digest(), digest);
    let fields = descriptor(&reopened.initial_events().unwrap(), reopened.player_guid());
    assert_eq!(visible(&fields), entries);
    let query = reopened
        .receive(ClientCommand::ItemQuery {
            entry: 900001,
            guid: 0,
        })
        .unwrap();
    assert!(
        matches!(&query.events[..],[SessionEvent::ItemTemplate{entry:900001,info:Some(info)}] if info.display_info_id==900002)
    );
}
