// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::player_progression;

/// ObjectMgr.cpp:4239-4272. Keep the source query's whitespace.
pub const QUEST_TEMPLATE_QUERY: &str = concat!(
    "SELECT entry, Method, ZoneOrSort, MinLevel, MaxLevel, QuestLevel, Type, RequiredClasses, RequiredRaces, RequiredSkill, RequiredSkillValue,",
    "RepObjectiveFaction, RepObjectiveValue, RequiredMinRepFaction, RequiredMinRepValue, RequiredMaxRepFaction, RequiredMaxRepValue, SuggestedPlayers, LimitTime,",
    "QuestFlags, SpecialFlags, PrevQuestId, NextQuestId, ExclusiveGroup, NextQuestInChain, SrcItemId, SrcItemCount, SrcSpell,",
    "Title, Details, Objectives, OfferRewardText, RequestItemsText, EndText, ObjectiveText1, ObjectiveText2, ObjectiveText3, ObjectiveText4,",
    "ReqItemId1, ReqItemId2, ReqItemId3, ReqItemId4, ReqItemCount1, ReqItemCount2, ReqItemCount3, ReqItemCount4,",
    "ReqSourceId1, ReqSourceId2, ReqSourceId3, ReqSourceId4, ReqSourceCount1, ReqSourceCount2, ReqSourceCount3, ReqSourceCount4,",
    "ReqCreatureOrGOId1, ReqCreatureOrGOId2, ReqCreatureOrGOId3, ReqCreatureOrGOId4, ReqCreatureOrGOCount1, ReqCreatureOrGOCount2, ReqCreatureOrGOCount3, ReqCreatureOrGOCount4,",
    "ReqSpellCast1, ReqSpellCast2, ReqSpellCast3, ReqSpellCast4,",
    "RewChoiceItemId1, RewChoiceItemId2, RewChoiceItemId3, RewChoiceItemId4, RewChoiceItemId5, RewChoiceItemId6,",
    "RewChoiceItemCount1, RewChoiceItemCount2, RewChoiceItemCount3, RewChoiceItemCount4, RewChoiceItemCount5, RewChoiceItemCount6,",
    "RewItemId1, RewItemId2, RewItemId3, RewItemId4, RewItemCount1, RewItemCount2, RewItemCount3, RewItemCount4,",
    "RewRepFaction1, RewRepFaction2, RewRepFaction3, RewRepFaction4, RewRepFaction5, RewRepValue1, RewRepValue2, RewRepValue3, RewRepValue4, RewRepValue5,",
    "RewOrReqMoney, RewMoneyMaxLevel, RewSpell, RewSpellCast, RewMailTemplateId, RewMailDelaySecs, PointMapId, PointX, PointY, PointOpt,",
    "DetailsEmote1, DetailsEmote2, DetailsEmote3, DetailsEmote4, DetailsEmoteDelay1, DetailsEmoteDelay2, DetailsEmoteDelay3, DetailsEmoteDelay4,",
    "IncompleteEmote, IncompleteEmoteDelay, CompleteEmote, CompleteEmoteDelay,  OfferRewardEmote1, OfferRewardEmote2, OfferRewardEmote3, OfferRewardEmote4,",
    "OfferRewardEmoteDelay1, OfferRewardEmoteDelay2, OfferRewardEmoteDelay3, OfferRewardEmoteDelay4,",
    "StartScript, CompleteScript, RequiredCondition, BreadcrumbForQuestId, RewFactionFlags",
    " FROM quest_template",
);

/// Loaded startup owners. None means the source call cannot read that owner.
#[derive(Default)]
pub struct QuestCatalogInputs<'a> {
    pub creatures: Option<&'a LoadedCreatureTemplates>,
    pub gameobjects: Option<&'a LoadedGameObjectTemplates>,
    pub items: Option<&'a SourceStartupItems>,
    pub spells: Option<&'a SourceSpellStores>,
    pub progression: Option<&'a player_progression::Rules>,
}

impl QuestLoaderOwner for QuestCatalogInputs<'_> {
    fn contains(&mut self, kind: QuestLookup, id: u32) -> Result<bool> {
        match kind {
            QuestLookup::Creature => Ok(self.creatures.ok_or_else(unbound)?.get(id).is_some()),
            QuestLookup::Item => Ok(self
                .items
                .ok_or_else(unbound)?
                .item_prototype(id)
                .map_err(|_| unbound())?
                .is_some()),
            QuestLookup::Talent => Ok(self.spells.ok_or_else(unbound)?.is_talent(id)),
            _ => Err(unbound()),
        }
    }
    fn gameobject_type(&mut self, id: u32) -> Result<Option<u32>> {
        Ok(self
            .gameobjects
            .ok_or_else(unbound)?
            .get(id)
            .map(|row| row.words[1]))
    }
    fn spell_effects(&mut self, id: u32) -> Result<Option<[(u32, i32); 3]>> {
        Ok(self.spells.ok_or_else(unbound)?.template(id).map(|spell| {
            [
                (spell.effect1, spell.effectmiscvalue1),
                (spell.effect2, spell.effectmiscvalue2),
                (spell.effect3, spell.effectmiscvalue3),
            ]
        }))
    }
    fn spell_valid(&mut self, id: u32, notices: &mut Vec<SpellValidityNotice>) -> Result<bool> {
        let spells = self.spells.ok_or_else(unbound)?;
        spells.is_spell_valid(spells.template(id), self.items, true, &mut |notice| {
            notices.push(notice)
        })
    }
    fn max_skill(&mut self) -> Result<u16> {
        Ok(self.progression.ok_or_else(unbound)?.global_skill_max)
    }
}

impl SourceQuestMap {
    /// World.cpp:1101 and :1104 read templates, then the four relation tables.
    /// The catalog preserves its retained occurrence order.
    pub fn load_catalog(
        catalog: &mut VerifiedCatalog,
        mut inputs: QuestCatalogInputs<'_>,
    ) -> Result<Self> {
        let mut map = Self::default();
        catalog.scan_execution_quest_templates(|quest| map.insert_quest(quest))?;
        map.validate_templates(&mut CatalogValidation {
            catalog,
            inputs: &mut inputs,
            dbcs: BTreeMap::new(),
        })?;
        map.load_quest_relations(
            |kind| Ok(catalog.execution_quest_relations(kind)?),
            &mut inputs,
        )?;
        Ok(map)
    }
}

struct CatalogValidation<'a, 'b> {
    catalog: &'a mut VerifiedCatalog,
    inputs: &'a mut QuestCatalogInputs<'b>,
    dbcs: BTreeMap<QuestLookup, BTreeMap<u32, u32>>,
}

impl QuestLoaderOwner for CatalogValidation<'_, '_> {
    fn contains(&mut self, kind: QuestLookup, id: u32) -> Result<bool> {
        use QuestLookup::*;
        let file = match kind {
            Area => "AreaTable.dbc",
            QuestSort => "QuestSort.dbc",
            SkillLine => "SkillLine.dbc",
            Faction => "Faction.dbc",
            MailTemplate => "MailTemplate.dbc",
            Creature | Item | Talent => return self.inputs.contains(kind, id),
        };
        let index =
            match self.dbcs.entry(kind) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    // DBCStores.cpp:671-674 uses AreaTable ID, like the other indexed DBCs.
                    entry.insert(self.catalog.execution_dbc_lookup_index(file).map_err(
                        |error| {
                            if error.id == "dbc.mandatory_file_missing" {
                                unbound()
                            } else {
                                error.into()
                            }
                        },
                    )?)
                }
            };
        Ok(index.contains_key(&id))
    }
    fn gameobject_type(&mut self, id: u32) -> Result<Option<u32>> {
        self.inputs.gameobject_type(id)
    }
    fn spell_effects(&mut self, id: u32) -> Result<Option<[(u32, i32); 3]>> {
        self.inputs.spell_effects(id)
    }
    fn spell_valid(&mut self, id: u32, notices: &mut Vec<SpellValidityNotice>) -> Result<bool> {
        self.inputs.spell_valid(id, notices)
    }
    fn max_skill(&mut self) -> Result<u16> {
        self.inputs.max_skill()
    }
}
