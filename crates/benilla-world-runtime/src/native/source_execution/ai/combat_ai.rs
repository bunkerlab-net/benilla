// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
use std::collections::BTreeMap;

const ACTION_CASTING_RESTORE: u32 = 1000;
const ACTION_ON_KILL_COOLDOWN: u32 = 1001;

#[derive(Clone, Debug)]
pub struct CombatAi {
    pub evade_check_cooldown_ms: u32,
    pub on_kill_cooldown: bool,
    pub stored_target: Option<SourceGuid>,
    pub stop_targeting: bool,
    custom: [SourceAiTimer; 2],
    actions: BTreeMap<u32, CombatAction>,
}

#[derive(Clone, Debug)]
struct CombatAction {
    timer: SourceAiTimer,
    initial: (u32, u32),
    initial_disabled: bool,
}

// Distinguish the overloads even when a ranged interval has equal bounds.
pub enum CombatActionInitial {
    Disabled(bool),
    Fixed(u32),
    Range(u32, u32),
}

impl CombatAi {
    // CombatAI.cpp:27-35 and sc_creature.cpp:22-25.
    pub(super) fn construct(random: &mut dyn CreatureRandom) -> Result<Self> {
        Ok(Self {
            evade_check_cooldown_ms: SCRIPTED_EVADE_CHECK_COOLDOWN_MS,
            on_kill_cooldown: false,
            stored_target: None,
            stop_targeting: false,
            // TimerAI.cpp:56-59 draws at (0, 0) for both disabled actions.
            custom: [
                SourceAiTimer::construct(ACTION_CASTING_RESTORE, (0, 0), true, random)?,
                SourceAiTimer::construct(ACTION_ON_KILL_COOLDOWN, (0, 0), true, random)?,
            ],
            actions: BTreeMap::new(),
        })
    }

    // TimerAI.cpp:213-227 constructs/emplace before the unchecked ready-bit write.
    pub fn add_combat_action(
        &mut self,
        base: &mut SourceUnitAi,
        id: u32,
        schedule: CombatActionInitial,
        random: &mut dyn CreatureRandom,
    ) -> Result<()> {
        let (initial, initial_disabled, ready, undefined) = match schedule {
            CombatActionInitial::Disabled(disabled) => (
                (0, 0),
                disabled,
                !disabled,
                "execution.ai.TimerAI_215.action_ready_index_undefined",
            ),
            CombatActionInitial::Fixed(ms) => (
                (ms, ms),
                false,
                false,
                "execution.ai.TimerAI_221.action_ready_index_undefined",
            ),
            CombatActionInitial::Range(min, max) => (
                (min, max),
                false,
                false,
                "execution.ai.TimerAI_227.action_ready_index_undefined",
            ),
        };
        let timer = SourceAiTimer::construct(id, initial, initial_disabled, random)?;
        self.actions.entry(id).or_insert(CombatAction {
            timer,
            initial,
            initial_disabled,
        });
        base.set_action_ready(id, ready)
            .map_err(|_| refuse(undefined))
    }

    // TimerAI.cpp:182-194. This class registers no timerless actions.
    // TimerAI.h:114,202 uses std::map: each collection resets in ascending ID order.
    pub(super) fn reset_timers(
        &mut self,
        base: &mut SourceUnitAi,
        random: &mut dyn CreatureRandom,
    ) -> Result<()> {
        base.clear_action_ready();
        for action in self.actions.values_mut() {
            action
                .timer
                .reset_initial(action.initial, action.initial_disabled, random)?;
        }
        // Custom IDs 1000/1001 precede UnitAI.h:112-113's 2000/2001.
        for timer in &mut self.custom {
            timer.reset_initial((0, 0), true, random)?;
        }
        Ok(())
    }

    pub fn custom_timers(&self) -> &[SourceAiTimer; 2] {
        &self.custom
    }

    pub fn handle_target_restoration(&mut self) -> Result<()> {
        Err(refuse(
            "execution.ai.CombatAI_51.HandleTargetRestoration_owner",
        ))
    }

    pub fn clear_on_kill_cooldown(&mut self) -> Result<()> {
        Err(refuse(
            "execution.ai.CombatAI_34.on_kill_cooldown_callback_owner",
        ))
    }

    // CombatAI.cpp:37-42. m_stopTargeting survives Reset.
    pub(super) fn reset(&mut self, base: &mut SourceUnitAi) {
        base.reset_creature_ranged_mode();
        self.on_kill_cooldown = false;
        self.stored_target = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) struct Draws<'a>(pub(super) &'a [(u32, u32, u32)]);

    impl CreatureRandom for Draws<'_> {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("unexpected irand")
        }

        fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
            let (&(want_min, want_max, value), rest) = self.0.split_first().unwrap();
            assert_eq!((min, max), (want_min, want_max));
            self.0 = rest;
            Ok(value)
        }
    }

    #[test]
    fn combat_ai_resets_ready_bits_and_timers_in_id_order() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut random = Draws(&[(0, 0, 0); 4]);
        let mut base = SourceUnitAi::construct(&objects, unit, 2, &mut random).unwrap();
        let mut combat = CombatAi::construct(&mut random).unwrap();
        assert!(random.0.is_empty());
        assert_eq!(combat.evade_check_cooldown_ms, 2500);
        assert_eq!(
            combat.custom.map(|t| (t.id, t.remaining, t.disabled)),
            [(1000, 0, true), (1001, 0, true)]
        );

        let mut random = Draws(&[(20, 20, 20), (0, 0, 0)]);
        combat
            .add_combat_action(&mut base, 1, CombatActionInitial::Fixed(20), &mut random)
            .unwrap();
        combat
            .add_combat_action(
                &mut base,
                0,
                CombatActionInitial::Disabled(false),
                &mut random,
            )
            .unwrap();
        assert!(random.0.is_empty());
        assert!(base.action_ready(0).unwrap());
        assert!(!base.action_ready(1).unwrap());
        combat.actions.get_mut(&1).unwrap().timer.remaining = 1;
        combat.custom[0].disabled = false;
        let mut random = Draws(&[(0, 0, 0), (20, 20, 20), (0, 0, 0), (0, 0, 0)]);
        combat.reset_timers(&mut base, &mut random).unwrap();
        assert!(random.0.is_empty());
        assert!(!base.action_ready(0).unwrap());
        assert_eq!(combat.actions[&1].timer.remaining, 20);
        assert!(combat.custom[0].disabled);

        combat.on_kill_cooldown = true;
        combat.stop_targeting = true;
        combat.stored_target = Some(SourceGuid::with_entry(HighGuid::Creature, 1, 1));
        base.ranged_mode = true;
        base.chase_distance = 30.0;
        combat.reset(&mut base);
        assert!(base.current_ranged_mode);
        assert_eq!(base.attack_distance, 30.0);
        assert!(!combat.on_kill_cooldown);
        assert!(combat.stored_target.is_none());
        assert!(combat.stop_targeting);
    }

    #[test]
    fn combat_ai_refuses_out_of_range_ready_bit_at_each_overload() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut random = Draws(&[(0, 0, 0); 4]);
        let mut base = SourceUnitAi::construct(&objects, unit, 0, &mut random).unwrap();
        let mut combat = CombatAi::construct(&mut random).unwrap();
        assert!(random.0.is_empty());
        let mut random = Draws(&[(0, 0, 0), (20, 20, 20), (20, 20, 20)]);
        for (schedule, id) in [
            (
                CombatActionInitial::Disabled(true),
                "execution.ai.TimerAI_215.action_ready_index_undefined",
            ),
            (
                CombatActionInitial::Fixed(20),
                "execution.ai.TimerAI_221.action_ready_index_undefined",
            ),
            (
                CombatActionInitial::Range(20, 20),
                "execution.ai.TimerAI_227.action_ready_index_undefined",
            ),
        ] {
            assert_eq!(
                combat
                    .add_combat_action(&mut base, 0, schedule, &mut random)
                    .unwrap_err()
                    .id,
                id
            );
        }
        assert!(random.0.is_empty());
    }
}
