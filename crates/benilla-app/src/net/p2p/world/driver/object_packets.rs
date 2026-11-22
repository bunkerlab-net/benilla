// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit object-manager packet boundaries for native projections, which produce events
//! directly rather than decoding wire packets. Adjacent object blocks form one packet; an
//! intervening non-object event ends it, so its observers see the preceding committed fields.
use benilla_protocol::{guid, NotifyBlock, SessionEvent};

/// Preserve the projection's order between packets and gameplay events. Within each object
/// packet, first-pass GUID writes precede the notify pass, just as protocol::decode does.
pub(super) fn frame(batch: Vec<SessionEvent>) -> Vec<SessionEvent> {
    let mut out = Vec::new();
    let mut first = Vec::new();
    let mut second = Vec::new();
    for event in batch {
        // A removal must lead its packet: never discard a native projection's removal because
        // it followed another block (the wire decoder deliberately rejects that wire shape).
        if matches!(event, SessionEvent::ObjectsRemoved(_)) {
            flush(&mut out, &mut first, &mut second);
        }
        match event {
            SessionEvent::ObjectCreate { guid: id, kind, .. } if guid::first_pass(id) => {
                second.push(SessionEvent::ObjectNotify {
                    guid: id,
                    block: NotifyBlock::Create(kind),
                });
                first.push(event);
            }
            SessionEvent::ItemCreate { guid: id, .. } if guid::first_pass(id) => {
                second.push(SessionEvent::ObjectNotify {
                    guid: id,
                    block: NotifyBlock::ItemCreate,
                });
                first.push(event);
            }
            SessionEvent::ObjectValues {
                guid: id,
                ref fields,
            } if guid::first_pass(id) => {
                second.push(SessionEvent::ObjectNotify {
                    guid: id,
                    block: NotifyBlock::Values(
                        fields.raw_fields().map(|(index, _)| index).collect(),
                    ),
                });
                first.push(event);
            }
            SessionEvent::ObjectsNear(ids) => {
                let (ahead, rest): (Vec<_>, Vec<_>) =
                    ids.into_iter().partition(|id| guid::first_pass(*id));
                if !ahead.is_empty() {
                    first.push(SessionEvent::ObjectsNear(ahead));
                }
                second.push(SessionEvent::ObjectsNear(rest));
            }
            SessionEvent::ObjectsRemoved(_) => first.push(event),
            SessionEvent::ObjectCreate { .. }
            | SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectValues { .. }
            | SessionEvent::ObjectMove { .. } => second.push(event),
            _ => {
                flush(&mut out, &mut first, &mut second);
                out.push(event);
            }
        }
    }
    flush(&mut out, &mut first, &mut second);
    out
}

fn flush(
    out: &mut Vec<SessionEvent>,
    first: &mut Vec<SessionEvent>,
    second: &mut Vec<SessionEvent>,
) {
    if first.is_empty() && second.is_empty() {
        return;
    }
    out.append(first);
    out.append(second);
    out.push(SessionEvent::ObjectUpdateEnd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::ObjectFields;
    fn values(guid: u64) -> SessionEvent {
        SessionEvent::ObjectValues {
            guid,
            fields: ObjectFields::from_pairs(&[(0, 7)]),
        }
    }
    #[test]
    fn ordinary_native_blocks_end_before_observers_and_at_batch_end() {
        let out = frame(vec![
            values(1),
            values(2),
            SessionEvent::Notification {
                text: "observer".into(),
            },
            values(3),
        ]);
        assert!(matches!(
            out.as_slice(),
            [
                SessionEvent::ObjectValues { guid: 1, .. },
                SessionEvent::ObjectValues { guid: 2, .. },
                SessionEvent::ObjectUpdateEnd,
                SessionEvent::Notification { .. },
                SessionEvent::ObjectValues { guid: 3, .. },
                SessionEvent::ObjectUpdateEnd,
            ]
        ));
    }
    #[test]
    fn first_pass_writes_precede_notifications_in_original_block_order() {
        let ahead = 0x6000_0000_0000_0001;
        let out = frame(vec![values(1), values(ahead), values(2)]);
        assert!(matches!(out.as_slice(), [
            SessionEvent::ObjectValues { guid, .. },
            SessionEvent::ObjectValues { guid: 1, .. },
            SessionEvent::ObjectNotify { guid: notified, block: NotifyBlock::Values(sent) },
            SessionEvent::ObjectValues { guid: 2, .. },
            SessionEvent::ObjectUpdateEnd,
        ] if *guid == ahead && *notified == ahead && sent == &[0]));
    }
    #[test]
    fn removal_starts_a_new_packet_and_empty_batches_add_nothing() {
        assert!(frame(vec![]).is_empty());
        let out = frame(vec![
            values(1),
            SessionEvent::ObjectsRemoved(vec![1]),
            values(2),
        ]);
        assert!(matches!(out.as_slice(), [
            SessionEvent::ObjectValues { guid: 1, .. }, SessionEvent::ObjectUpdateEnd,
            SessionEvent::ObjectsRemoved(ids), SessionEvent::ObjectValues { guid: 2, .. }, SessionEvent::ObjectUpdateEnd,
        ] if ids == &[1]));
    }
    #[test]
    fn first_pass_item_create_and_near_are_framed_without_losing_identity() {
        let ahead = 0x8000_0000_0000_0001;
        let out = frame(vec![
            SessionEvent::ItemCreate {
                guid: ahead,
                container: true,
                fields: ObjectFields::default(),
            },
            SessionEvent::ObjectsNear(vec![1, ahead]),
        ]);
        assert!(matches!(out.as_slice(), [
            SessionEvent::ItemCreate { guid: item, container: true, .. },
            SessionEvent::ObjectsNear(first),
            SessionEvent::ObjectNotify { guid: notified, block: NotifyBlock::ItemCreate },
            SessionEvent::ObjectsNear(second),
            SessionEvent::ObjectUpdateEnd,
        ] if *item == ahead && *notified == ahead && first == &[ahead] && second == &[1]));
    }
}
