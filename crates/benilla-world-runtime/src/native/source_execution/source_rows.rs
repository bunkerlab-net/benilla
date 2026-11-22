// SPDX-License-Identifier: GPL-3.0-or-later
//! Canonical source database rows and statements in execution order.

use serde::{Deserialize, Serialize};

/// A prepared FLOAT value, before any text-protocol read conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FloatWord(u32);

impl FloatWord {
    pub fn from_float(value: f32) -> Self {
        Self(value.to_bits())
    }

    pub fn as_float(self) -> f32 {
        f32::from_bits(self.0)
    }
}

/// `initial-characters.sql:599-664`, in `Player.cpp:15624-15758` bind order.
/// Arrays group adjacent columns or encode the words in source text columns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterRow {
    pub guid: u32,
    pub account: u32,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub level: u8,
    pub xp: u32,
    pub money: u32,
    pub player_bytes: u32,
    pub player_bytes2: u32,
    pub player_flags: u32,
    pub map: u32,
    pub xyz: [FloatWord; 3],
    pub orientation: FloatWord,
    pub taxi_mask: [u32; 8],
    pub online: bool,
    pub cinematic: u8,
    pub total_time: u32,
    pub level_time: u32,
    pub rest_bonus: FloatWord,
    pub logout_time: u64,
    pub is_logout_resting: bool,
    pub reset_talents_cost: u32,
    pub reset_talents_time: u64,
    pub transport_pose: [FloatWord; 4],
    pub transport_guid: u64,
    pub extra_flags: u32,
    pub stable_slots: u8,
    pub at_login: u32,
    pub zone: u32,
    pub death_expire_time: u64,
    pub taxi_path: Vec<u32>,
    pub honor_highest_rank: u32,
    pub honor_standing: u32,
    pub stored_honor_rating: FloatWord,
    pub stored_dishonorable_kills: i32,
    pub stored_honorable_kills: i32,
    pub watched_faction: u32,
    pub drunk: u16,
    pub health: u32,
    pub powers: [u32; 5],
    #[serde(with = "exploration_words")]
    pub explored_zones: [u32; 64],
    pub equipment_cache: [(u32, u32); 20],
    pub ammo_id: u32,
    pub action_bars: u8,
    pub fishing_steps: u8,
}

mod exploration_words {
    use serde::de::{self, SeqAccess, Visitor};
    use serde::{Deserializer, Serialize, Serializer};
    use std::fmt;

    pub fn serialize<S: Serializer>(words: &[u32; 64], serializer: S) -> Result<S::Ok, S::Error> {
        words.as_slice().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u32; 64], D::Error> {
        struct Words;
        impl<'de> Visitor<'de> for Words {
            type Value = [u32; 64];

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("64 exploration words")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut words = [0; 64];
                for (i, word) in words.iter_mut().enumerate() {
                    *word = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(i, &self))?;
                }
                Ok(words)
            }
        }
        deserializer.deserialize_tuple(64, Words)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryRow {
    pub guid: u32,
    pub bag_guid: u32,
    pub slot: u8,
    pub item_guid: u32,
    pub item_template: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInstanceRow {
    pub guid: u32,
    pub owner_guid: u32,
    pub entry: u32,
    pub creator: u32,
    pub gift_creator: u32,
    pub count: u32,
    pub duration: u32,
    pub charges: [i32; 5],
    pub flags: u32,
    pub enchantments: [[u32; 3]; 7],
    pub random_property: i16,
    pub durability: u16,
    pub text_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillRow {
    pub guid: u32,
    pub skill: u32,
    pub value: u16,
    pub max: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRow {
    pub guid: u32,
    pub button: u8,
    pub action: u32,
    pub kind: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellRow {
    pub guid: u32,
    pub spell: u32,
    pub active: bool,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestRow {
    pub guid: u32,
    pub quest: u32,
    pub status: u8,
    pub rewarded: bool,
    pub explored: bool,
    pub timer_seconds: u64,
    pub mob: [u32; 4],
    pub item: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepRow {
    pub guid: u32,
    pub faction: u32,
    pub standing: i32,
    pub flags: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TutorialRow {
    pub account: u64,
    pub words: [u32; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomebindRow {
    pub guid: u32,
    pub map: u32,
    pub zone: u32,
    pub xyz: [FloatWord; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RespawnKind {
    Creature,
    GameObject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RespawnRow {
    pub kind: RespawnKind,
    pub guid: u32,
    pub instance: u32,
    pub deadline: u64,
}

/// `world_state` ID 0, written by `WorldState.cpp:225-244`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DragonStateRow {
    pub state: u8,
    pub deadline: u64,
    pub positions: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CooldownRow {
    pub guid: u32,
    pub spell: u32,
    pub spell_expire_time: u64,
    pub category: u32,
    pub category_expire_time: u64,
    pub item_id: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuraRow {
    pub guid: u32,
    pub caster_guid: u64,
    pub item_guid: u32,
    pub spell: u32,
    pub stack_count: u32,
    pub remaining_charges: u8,
    pub base_points: [i32; 3],
    pub periodic_time: [u32; 3],
    pub max_duration: i32,
    pub remaining_time: i32,
    pub effect_index_mask: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnteredInstanceRow {
    pub account: u32,
    pub expire_time: i64,
    pub instance: u32,
}

/// Keep statement order in the batch, including deletes that match no rows.
#[derive(Debug, PartialEq, Eq)]
pub enum RowOperation {
    Insert(RowInsert),
    ReplaceItemInstance(ItemInstanceRow),
    Update(RowUpdate),
    Delete(RowDelete),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RowInsert {
    Character(Box<CharacterRow>),
    Inventory(InventoryRow),
    Skill(SkillRow),
    Action(ActionRow),
    Spell(SpellRow),
    Quest(QuestRow),
    WeeklyQuest { guid: u32, quest: u32 },
    Reputation(RepRow),
    Tutorial(TutorialRow),
    Homebind(HomebindRow),
    Respawn(RespawnRow),
    DragonState(DragonStateRow),
    Cooldown(CooldownRow),
    Aura(AuraRow),
    EnteredInstance(EnteredInstanceRow),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RowUpdate {
    OnlineByGuid { guid: u32, online: bool },
    OnlineByAccount { account: u32, online: bool },
    ClearOnline,
    Inventory(InventoryRow),
    ItemInstance(ItemInstanceRow),
    Skill(SkillRow),
    Action(ActionRow),
    Quest(QuestRow),
    Tutorial(TutorialRow),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowDelete {
    Character(u32),
    Inventory {
        item_guid: u32,
    },
    ItemInstance(u32),
    Skill {
        guid: u32,
        skill: u32,
    },
    Action {
        guid: u32,
        button: u8,
    },
    Spell {
        guid: u32,
        spell: u32,
    },
    WeeklyQuestsByGuid(u32),
    Reputation {
        guid: u32,
        faction: u32,
    },
    Respawn {
        kind: RespawnKind,
        guid: u32,
        instance: u32,
    },
    DragonState,
    CooldownsByGuid(u32),
    AurasByGuid(u32),
    EnteredInstancesByAccount(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_words_round_trip_without_text_conversion() {
        for bits in [0x8000_0000, 0x0000_0001, 0xc320_4991, 0x7f7f_ffff] {
            let word = FloatWord::from_float(f32::from_bits(bits));
            let text = serde_json::to_string(&word).unwrap();
            assert_eq!(text, bits.to_string());
            assert_eq!(
                serde_json::from_str::<FloatWord>(&text)
                    .unwrap()
                    .as_float()
                    .to_bits(),
                bits
            );
        }
    }

    #[test]
    fn fixed_words_and_integer_widths_refuse_bad_columns() {
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct Explored(#[serde(with = "exploration_words")] [u32; 64]);

        let words = Explored([u32::MAX; 64]);
        let mut encoded = serde_json::to_value(&words).unwrap();
        assert_eq!(
            serde_json::from_value::<Explored>(encoded.clone()).unwrap(),
            words
        );
        encoded.as_array_mut().unwrap().pop();
        assert!(serde_json::from_value::<Explored>(encoded.clone()).is_err());
        encoded.as_array_mut().unwrap().extend([0.into(), 0.into()]);
        assert!(serde_json::from_value::<Explored>(encoded).is_err());

        let json =
            r#"{"guid":1,"bag_guid":0,"slot":256,"item_guid":4294967295,"item_template":35}"#;
        assert!(serde_json::from_str::<InventoryRow>(json).is_err());
        let row = serde_json::from_str::<InventoryRow>(&json.replace("256", "255")).unwrap();
        assert_eq!((row.slot, row.item_guid), (u8::MAX, u32::MAX));
    }

    #[test]
    fn transcript_keeps_distinct_scopes_and_repeated_keys_in_order() {
        let delete = RowOperation::Delete(RowDelete::Character(7));
        let offline = RowOperation::Update(RowUpdate::OnlineByAccount {
            account: 19,
            online: false,
        });
        let last_delete = RowOperation::Delete(RowDelete::Character(7));
        let batch = Vec::from([delete, offline, last_delete]);
        assert!(matches!(
            batch.as_slice(),
            [
                RowOperation::Delete(RowDelete::Character(7)),
                RowOperation::Update(RowUpdate::OnlineByAccount {
                    account: 19,
                    online: false
                }),
                RowOperation::Delete(RowDelete::Character(7)),
            ]
        ));
    }
}
