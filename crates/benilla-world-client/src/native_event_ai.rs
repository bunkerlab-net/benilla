// SPDX-License-Identifier: GPL-3.0-or-later
//! Callback-bound MonsterText SAY uses inline localized text and the currently
//! represented NPC generation, including before same-frame corpse destruction.
use super::*;

pub(super) fn project(
    projection: &NativeProjection,
    world: &n::NativeWorld,
    speech: &n::EventAiSpeech,
    speaker_guid: u64,
) -> Result<SessionEvent, Error> {
    let callback = &speech.callback;
    if callback.world != projection.world
        || callback.tick != world.state().tick
        || callback.now_ms != world.state().now_ms
        || !matches!(callback.moment, n::InteractionMoment::Frame { .. })
        || callback.invoker != projection.player
        || speech.recipient != projection.player
    {
        return Err(Error::StaleProjection);
    }
    let runtime = world
        .profile()
        .source_visitors
        .as_ref()
        .and_then(|visitors| visitors.event_ai())
        .ok_or(Error::StaleProjection)?;
    if callback.catalog != runtime.digest() || speech.locale != runtime.locale() {
        return Err(Error::StaleProjection);
    }
    let catalog = runtime.catalog();
    let broadcast = i32::try_from(speech.broadcast_id).map_err(|_| Error::StaleProjection)?;
    let source = catalog
        .events(callback.npc.spawn)
        .map_err(|_| Error::StaleProjection)?
        .into_iter()
        .find(|event| event.event_id() == speech.event_id)
        .ok_or(Error::StaleProjection)?;
    if source.text_ids().get(usize::from(speech.text_slot)) != Some(&broadcast) {
        return Err(Error::StaleProjection);
    }
    let localized = catalog
        .localized(callback.npc.spawn, broadcast, speech.locale)
        .map_err(|_| Error::StaleProjection)?;
    if localized.speaker_name != speech.speaker_name
        || localized.text != speech.text
        || localized.language != speech.language
    {
        return Err(Error::StaleProjection);
    }
    Ok(SessionEvent::Chat(message(
        speech,
        speaker_guid,
        projection.player_guid,
    )))
}

fn message(speech: &n::EventAiSpeech, speaker_guid: u64, target_guid: u64) -> p::ChatMessage {
    // CMaNGOS ChatHandler::BuildChatPacket MONSTER_SAY: target is the
    // player's macro/addressee GUID; Common (7) remains untranslated on wire.
    p::ChatMessage {
        chat_type: p::CHAT_MSG_MONSTER_SAY,
        language: speech.language,
        sender_guid: speaker_guid,
        target_guid,
        sender_name: Some(speech.speaker_name.clone()),
        channel: None,
        text: speech.text.clone(),
        chat_tag: 0,
    }
}

#[cfg(test)]
#[path = "native_event_ai_tests.rs"]
mod tests;
