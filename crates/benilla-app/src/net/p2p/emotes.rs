//! Exploration emotes use the install's DBC mapping and the existing realm presentation events.
use super::*;

pub(super) fn emit_text_emote(
    events: &Sender<SessionEvent>,
    performer: u64,
    text_id: u32,
    target: u64,
    own: &Character,
    peers: &BTreeMap<PeerId, Option<Character>>,
    catalog: &AvatarCatalog,
) -> Result<()> {
    // A peer may have a different patch chain. Never accept an animation or arbitrary name
    // from the wire, and never substitute a guessed row for an unavailable text emote.
    if !catalog.1.contains_text(text_id) {
        return Ok(());
    }
    let target_name = std::iter::once(own)
        .chain(peers.values().flatten())
        .filter(|c| c.position.map_id == own.position.map_id)
        .find(|c| character_guid(c) == target)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    if let Some(emote_id) = catalog.1.text_emote(text_id) {
        // Emotes.dbc EmoteSpecProc: 0 is a one-shot; 2 owns UNIT_NPC_EMOTESTATE.
        // Posture (1) travels separately through StandStateChange, including movement's stand-up.
        match catalog.1.spec_proc(emote_id) {
            Some(0) => emit(
                events,
                SessionEvent::Emote {
                    guid: performer,
                    emote_id,
                },
            )?,
            Some(2) => emit(
                events,
                SessionEvent::ObjectValues {
                    guid: performer,
                    fields: ObjectFields::from_pairs(&[(148, emote_id)]),
                },
            )?,
            _ => {}
        }
    }
    emit(
        events,
        SessionEvent::TextEmote {
            guid: performer,
            text_emote: text_id,
            target_name,
        },
    )
}

/// Only player-controlled standing, sitting, sleeping and kneeling exist in exploration.
pub(super) fn valid_stand_state(state: u32) -> bool {
    matches!(state, 0 | 1 | 3 | 8)
}

pub(super) fn state_emote_id(
    catalog: &benilla_formats::EmoteSoundCatalog,
    text_id: u32,
) -> Option<u32> {
    let id = catalog.text_emote(text_id)?;
    (catalog.spec_proc(id) == Some(2)).then_some(id)
}

/// Admission snapshots restore a pose without replaying old chat or voice.
pub(super) fn emit_presentation(
    events: &Sender<SessionEvent>,
    character: &Character,
    stand_state: u8,
    state_emote: Option<u32>,
    catalog: &AvatarCatalog,
) -> Result<()> {
    use benilla_protocol::field::FIELD_UNIT_BYTES_1;
    // Exploration owns this descriptor; preserve every non-posture bit from its avatar
    // baseline. No exploration command changes the other bytes of UNIT_FIELD_BYTES_1.
    let SessionEvent::ObjectCreate { fields, .. } = character_avatar(character, catalog)? else {
        unreachable!("character_avatar creates an avatar")
    };
    let packed = fields
        .raw_fields()
        .find_map(|(index, value)| (index == FIELD_UNIT_BYTES_1).then_some(value))
        .unwrap_or(0);
    let mut fields = vec![(
        FIELD_UNIT_BYTES_1,
        (packed & !0xff) | u32::from(stand_state),
    )];
    match state_emote {
        Some(text_id) => {
            if let Some(id) = state_emote_id(&catalog.1, text_id) {
                fields.push((148, id));
            }
        }
        None => fields.push((148, 0)),
    }
    emit(
        events,
        SessionEvent::ObjectValues {
            guid: character_guid(character),
            fields: ObjectFields::from_pairs(&fields),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> AvatarCatalog {
        let mut catalog = AvatarCatalog::fixture();
        // Original synthetic rows; the production mapping comes only from the install.
        catalog.1 = benilla_formats::EmoteSoundCatalog::default()
            .with_text_row(9001, 8001, 0)
            .with_text_row(9002, 8002, 2)
            .with_text_row(9003, 8003, 1)
            .with_text_row(9004, 0, 0);
        catalog
    }

    #[test]
    fn off_map_emote_targets_do_not_reveal_names() {
        let own = super::super::tests::fixture(1, 1, "Local");
        let mut other = super::super::tests::fixture(2, 2, "Hidden");
        other.position.map_id = 1;
        let peers = [(other.owner_id, Some(other.clone()))].into();
        let (tx, rx) = crossbeam_channel::unbounded();
        emit_text_emote(
            &tx,
            character_guid(&own),
            9001,
            character_guid(&other),
            &own,
            &peers,
            &catalog(),
        )
        .unwrap();
        assert!(rx.try_iter().any(|event| matches!(event, SessionEvent::TextEmote { target_name, .. } if target_name.is_empty())));
    }

    #[test]
    fn text_emotes_project_one_shot_loop_posture_and_chat_only_separately() {
        let own = super::super::tests::fixture(1, 1, "Local");
        let other = super::super::tests::fixture(2, 2, "Friend");
        let peers = [(other.owner_id, Some(other.clone()))].into();
        let (tx, rx) = crossbeam_channel::unbounded();
        let catalog = catalog();
        let performer = character_guid(&own);
        for text_id in 9001..=9004 {
            emit_text_emote(
                &tx,
                performer,
                text_id,
                character_guid(&other),
                &own,
                &peers,
                &catalog,
            )
            .unwrap();
            if text_id == 9001 {
                assert!(
                    matches!(rx.try_recv().unwrap(), SessionEvent::Emote { guid, emote_id: 8001 } if guid == performer)
                );
            } else if text_id == 9002 {
                assert!(
                    matches!(rx.try_recv().unwrap(), SessionEvent::ObjectValues { guid, fields } if guid == performer && fields.unit_emote_state() == 8002)
                );
            }
            assert!(
                matches!(rx.try_recv().unwrap(), SessionEvent::TextEmote { guid, text_emote, target_name } if guid == performer && text_emote == text_id && target_name == "Friend")
            );
            assert!(rx.is_empty());
        }
        emit_text_emote(&tx, performer, u32::MAX, 0, &own, &peers, &catalog).unwrap();
        assert!(rx.is_empty(), "unknown rows do not invent presentation");
    }

    #[test]
    fn peer_emote_uses_authenticated_character_and_never_target_as_performer() {
        let own = super::super::tests::fixture(1, 1, "Local");
        let other = super::super::tests::fixture(2, 2, "Friend");
        let mut peers = BTreeMap::new();
        let (tx, rx) = crossbeam_channel::unbounded();
        let payload = Frame::TextEmote {
            text_id: 9001,
            target: character_guid(&own),
        }
        .encode()
        .unwrap();
        let catalog = catalog();
        let spoofed = format!(
            "{}\nguid = {}\n",
            String::from_utf8(payload.clone()).unwrap(),
            character_guid(&own)
        );
        assert!(
            Frame::decode(spoofed.as_bytes()).is_err(),
            "the wire cannot choose a performer"
        );
        apply_peer_frame(&tx, other.owner_id, &payload, &mut peers, &own, &catalog).unwrap();
        peers.insert(other.owner_id, None);
        apply_peer_frame(&tx, other.owner_id, &payload, &mut peers, &own, &catalog).unwrap();
        assert!(
            rx.is_empty(),
            "unknown and pre-introduction peers cannot emote"
        );
        peers.insert(other.owner_id, Some(other.clone()));
        apply_peer_frame(&tx, other.owner_id, &payload, &mut peers, &own, &catalog).unwrap();
        assert!(
            matches!(rx.try_recv().unwrap(), SessionEvent::Emote { guid, .. } if guid == character_guid(&other))
        );
        assert!(
            matches!(rx.try_recv().unwrap(), SessionEvent::TextEmote { guid, target_name, .. } if guid == character_guid(&other) && target_name == "Local")
        );
    }
    #[test]
    fn late_join_pose_and_stand_up_do_not_replay_text_or_voice() {
        let own = super::super::tests::fixture(1, 1, "Local");
        let other = super::super::tests::fixture(2, 2, "Friend");
        let mut peers = [(other.owner_id, Some(other.clone()))].into();
        let (tx, rx) = crossbeam_channel::unbounded();
        let catalog = catalog();
        for stand_state in [1, 3, 8, 0] {
            let payload = Frame::Presentation {
                stand_state,
                state_emote: Some(9002),
            }
            .encode()
            .unwrap();
            apply_peer_frame(&tx, other.owner_id, &payload, &mut peers, &own, &catalog).unwrap();
            assert!(
                matches!(rx.try_recv().unwrap(), SessionEvent::ObjectValues { guid, fields }
                if guid == character_guid(&other) && fields.unit_stand_state() == stand_state && fields.unit_emote_state() == 8002)
            );
            assert!(
                rx.is_empty(),
                "a pose snapshot has no text, voice or one-shot event"
            );
        }
        let payload = Frame::Presentation {
            stand_state: 0,
            state_emote: None,
        }
        .encode()
        .unwrap();
        apply_peer_frame(&tx, other.owner_id, &payload, &mut peers, &own, &catalog).unwrap();
        assert!(
            matches!(rx.try_recv().unwrap(), SessionEvent::ObjectValues { fields, .. } if fields.unit_emote_state() == 0)
        );
        for invalid in [2, 7, 255] {
            let payload = Frame::Presentation {
                stand_state: invalid,
                state_emote: None,
            }
            .encode()
            .unwrap();
            assert!(Frame::decode(&payload).is_err());
        }
        assert!(
            !valid_stand_state(256),
            "large local commands cannot truncate to standing"
        );
    }
}
