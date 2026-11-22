// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

#[derive(Clone, Debug)]
pub struct BossAzuregosAi {
    pub(super) binding: Arc<ScriptDevAiBinding>,
    pub combat: CombatAi,
}

impl BossAzuregosAi {
    pub(super) const ACTION_COUNT: u32 = 6;

    // boss_azuregos.cpp:42-62. GetNewAIInstance constructs this at :148.
    pub(super) fn construct(
        binding: &Arc<ScriptDevAiBinding>,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
    ) -> Result<Self> {
        use CombatActionInitial::{Fixed, Range};
        let mut combat = CombatAi::construct(random)?;
        for (id, schedule) in [
            (0, Range(5000, 17000)),
            (1, Range(10000, 30000)),
            (2, Range(2000, 8000)),
            (3, Fixed(30000)),
            (4, Range(15000, 30000)),
            (5, Fixed(7000)),
        ] {
            combat.add_combat_action(base, id, schedule, random)?;
        }
        Ok(Self {
            binding: Arc::clone(binding),
            combat,
        })
    }

    pub(crate) fn reset_timers(
        &mut self,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
    ) -> Result<()> {
        self.combat.reset_timers(base, random)
    }

    // boss_azuregos.cpp:65-68.
    pub(super) fn reset(&mut self, base: &mut SourceUnitAi) {
        self.combat.reset(base);
    }

    pub fn killed_unit(&mut self, _victim: ObjectHandle) -> Result<()> {
        Err(refuse("execution.ai.boss_azuregos_70.KilledUnit_owner"))
    }

    pub fn aggro(&mut self, _enemy: ObjectHandle) -> Result<()> {
        Err(refuse("execution.ai.boss_azuregos_77.Aggro_owner"))
    }

    pub fn enter_evade_mode(&mut self) -> Result<()> {
        Err(refuse("execution.ai.boss_azuregos_85.EnterEvadeMode_owner"))
    }

    pub fn execute_action(&mut self, _action: u32) -> Result<()> {
        Err(refuse("execution.ai.boss_azuregos_92.ExecuteAction_owner"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::source_execution::creature_tests::{chicken_binding, Random};

    #[test]
    fn azuregos_combat_callbacks_refuse_at_their_entries() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut random = Random::new();
        let mut base =
            SourceUnitAi::construct(&objects, unit, BossAzuregosAi::ACTION_COUNT, &mut random)
                .unwrap();
        let mut binding = chicken_binding();
        binding.name = "boss_azuregos".into();
        let mut ai = BossAzuregosAi::construct(&Arc::new(binding), &mut base, &mut random).unwrap();
        assert_eq!(
            ai.killed_unit(unit).unwrap_err().id,
            "execution.ai.boss_azuregos_70.KilledUnit_owner"
        );
        assert_eq!(
            ai.aggro(unit).unwrap_err().id,
            "execution.ai.boss_azuregos_77.Aggro_owner"
        );
        assert_eq!(
            ai.enter_evade_mode().unwrap_err().id,
            "execution.ai.boss_azuregos_85.EnterEvadeMode_owner"
        );
        assert_eq!(
            ai.execute_action(3).unwrap_err().id,
            "execution.ai.boss_azuregos_92.ExecuteAction_owner"
        );
    }
}
