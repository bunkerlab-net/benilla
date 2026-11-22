//! The saved exploration character's racial language baseline, not a class spell book.
//! CMaNGOS ChatHandler validates the speaker's language skill before routing speech;
//! the 1.12 client then substitutes unknown language using its own LanguageWords.dbc.
use anyhow::{bail, Result};
use benilla_protocol::{messages, SessionEvent};

#[derive(Clone, Copy)]
struct Language {
    id: u32,
    spell: u32,
    skill: u32,
}
const COMMON: Language = Language {
    id: 7,
    spell: 668,
    skill: 98,
};
const ORCISH: Language = Language {
    id: 1,
    spell: 669,
    skill: 109,
};

// CMaNGOS classic-db 603ece109099ce44a5447a1b1cffd65c7272d71c:
// Full_DB/ClassicDB_1_12_1_z2815.sql.gz, playercreateinfo_spell, all valid racial classes.
// Spell→language→skill joins are pinned against 5875 DBCs in formats::spells::catalog_tests.
fn known(race: u8) -> &'static [Language] {
    match race {
        1 => &[COMMON],
        2 => &[ORCISH],
        3 => &[
            COMMON,
            Language {
                id: 6,
                spell: 672,
                skill: 111,
            },
        ],
        4 => &[
            COMMON,
            Language {
                id: 2,
                spell: 671,
                skill: 113,
            },
        ],
        5 => &[
            ORCISH,
            Language {
                id: 33,
                spell: 17737,
                skill: 673,
            },
        ],
        6 => &[
            ORCISH,
            Language {
                id: 3,
                spell: 670,
                skill: 115,
            },
        ],
        7 => &[
            COMMON,
            Language {
                id: 13,
                spell: 7340,
                skill: 313,
            },
        ],
        8 => &[
            ORCISH,
            Language {
                id: 14,
                spell: 7341,
                skill: 315,
            },
        ],
        _ => &[],
    }
}

/// Only admitted, immutable race data grants languages; a frame cannot grant fluency.
pub(super) fn language(race: u8, requested: Option<u32>) -> Result<u32> {
    let languages = known(race);
    let selected = requested.or_else(|| languages.first().map(|language| language.id));
    if let Some(selected) =
        selected.filter(|id| languages.iter().any(|language| language.id == *id))
    {
        return Ok(selected);
    }
    bail!("P2P character does not know that language")
}

pub(super) fn message(
    sender_guid: u64,
    race: u8,
    requested: u32,
    text: String,
) -> Result<SessionEvent> {
    let language = language(race, Some(requested))?;
    if text.is_empty() || text.len() > super::MAX_CHAT || text.chars().any(char::is_control) {
        bail!("invalid P2P chat");
    }
    Ok(SessionEvent::Chat(messages::ChatMessage {
        chat_type: messages::CHAT_MSG_SAY,
        language,
        sender_guid,
        target_guid: 0,
        sender_name: None,
        channel: None,
        text,
        chat_tag: 0,
    }))
}

/// Feed the existing client spell→language→skill lookup, including after reconnect.
pub(super) fn spell_book(race: u8) -> SessionEvent {
    SessionEvent::SpellBook {
        spell_ids: known(race).iter().map(|language| language.spell).collect(),
        cooldowns: Vec::new(),
    }
}

pub(super) fn append_skills(race: u8, fields: &mut Vec<(u16, u32)>) {
    // CMaNGOS core 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Player.cpp:5748–5751:
    // SKILL_RANGE_LANGUAGE uses SetSkill(skillId, 300, 300).
    for (slot, language) in known(race).iter().enumerate() {
        let field = messages::FIELD_PLAYER_SKILL_INFO_1_1 + slot as u16 * 3;
        fields.extend([
            (field, language.skill),
            (field + 1, 300 | (300 << 16)),
            (field + 2, 0),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_classic_races_speak_only_their_starter_languages() {
        let rows: &[(u8, &[u32])] = &[
            (1, &[7]),
            (2, &[1]),
            (3, &[7, 6]),
            (4, &[7, 2]),
            (5, &[1, 33]),
            (6, &[1, 3]),
            (7, &[7, 13]),
            (8, &[1, 14]),
        ];
        for &(race, expected) in rows {
            assert_eq!(language(race, None).unwrap(), expected[0]);
            for id in [0, 1, 2, 3, 6, 7, 13, 14, 33, 999, u32::MAX] {
                assert_eq!(
                    language(race, Some(id)).is_ok(),
                    expected.contains(&id),
                    "race={race} language={id}"
                );
            }
        }
        for race in [0, 9, 10, 255] {
            assert!(language(race, None).is_err());
            assert!(language(race, Some(7)).is_err());
        }
    }

    #[test]
    fn human_and_forsaken_speech_keeps_its_language_and_rejects_universal_forgery() {
        for (race, id) in [(1, 7), (5, 1), (5, 33)] {
            let SessionEvent::Chat(chat) = message(123, race, id, "hello".into()).unwrap() else {
                panic!("chat")
            };
            assert_eq!(
                (chat.sender_guid, chat.language, chat.chat_type),
                (123, id, messages::CHAT_MSG_SAY)
            );
            assert_eq!(chat.text, "hello");
            assert!(message(123, race, 0, "forged universal".into()).is_err());
            assert!(message(123, race, u32::MAX, "forged addon".into()).is_err());
        }
        assert!(message(123, 1, 1, "forged Orcish".into()).is_err());
        assert!(message(123, 5, 7, "forged Common".into()).is_err());
    }

    #[test]
    fn language_spell_and_skill_projection_is_complete_and_reconnect_stable() {
        for race in 1..=8 {
            let mut pairs = Vec::new();
            append_skills(race, &mut pairs);
            let fields = benilla_protocol::ObjectFields::from_pairs(&pairs);
            let SessionEvent::SpellBook {
                spell_ids,
                cooldowns,
            } = spell_book(race)
            else {
                panic!("spell book")
            };
            assert!(cooldowns.is_empty());
            assert_eq!(spell_ids.len(), known(race).len());
            for (slot, language) in known(race).iter().enumerate() {
                assert_eq!(spell_ids[slot], language.spell);
                let skill = fields.player_skill(slot as u8).unwrap();
                assert_eq!(u32::from(skill.skill_id), language.skill);
                assert_eq!((skill.value, skill.max), (300, 300));
            }
            let mut reconnect = Vec::new();
            append_skills(race, &mut reconnect);
            assert_eq!(pairs, reconnect);
        }
    }
}
