//! Movement cancellation of persistent exploration emotes, independent of presentation/transport.

/// CMaNGOS Classic 8ec338a MovementHandler.cpp:823 and Object.h:730: movement/turn
/// clears the persistent emote, but idle heartbeats and walk/run mode toggles do not.
/// The mask includes direction/pitch/turn, falling-far and spline-elevation flags.
pub(super) fn cancel_loop(state_emote: &mut Option<u32>, flags: u32) -> bool {
    const MOVING_OR_TURN: u32 = 0x0400_40ff;
    flags & MOVING_OR_TURN != 0 && state_emote.take().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_clears_loop_once_and_keeps_late_join_state_clear() {
        for flags in [1, 2, 4, 8, 16, 32, 64, 128, 0x4000, 0x0400_0000] {
            let mut state = Some(9002);
            assert!(cancel_loop(&mut state, flags), "flags={flags:#x}");
            assert_eq!(state, None, "admission snapshot must not revive the loop");
            assert!(
                !cancel_loop(&mut state, flags),
                "no repeated state broadcasts"
            );
        }
    }

    #[test]
    fn stationary_heartbeat_and_mode_changes_preserve_loop() {
        for flags in [0, 0x100, 0x400, 0x0020_0000, 0x0200_0000] {
            let mut state = Some(9002);
            assert!(!cancel_loop(&mut state, flags));
            assert_eq!(state, Some(9002));
        }
    }

    #[test]
    fn one_shot_is_not_a_persistent_state_to_cancel() {
        let mut state = None;
        assert!(!cancel_loop(&mut state, 1));
        assert_eq!(state, None);
    }
}
