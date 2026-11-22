// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

// WorldSession.h:128-134.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWorldSessionState {
    Created,
    CharSelection,
    Ready,
    Offline,
}

// WorldSession.h:121-126,860-861.
#[derive(Clone, Copy)]
pub(super) enum TutorialState {
    Unchanged,
    Changed,
    New,
}

pub struct SourceWorldSession {
    // WorldSession.cpp:102 binds the optional constructor socket.
    socket_present: bool,
    // WorldSession.cpp:100-103 binds the account supplied by the caller.
    account_id: u32,
    // WorldSession.cpp:102 initializes m_sessionState to CREATED.
    state: SourceWorldSessionState,
    // WorldSession.cpp:105 initializes m_playerLoading to false.
    pub(super) player_loading: bool,
    pub(super) tutorial_words: [u32; 8],
    pub(super) tutorial_state: TutorialState,
}

impl SourceWorldSession {
    pub fn construct(account_id: u32, socket_present: bool) -> Self {
        Self {
            socket_present,
            account_id,
            state: SourceWorldSessionState::Created,
            player_loading: false,
            tutorial_words: [0; 8],
            tutorial_state: TutorialState::Unchanged, // WorldSession.cpp:107.
        }
    }

    // WorldSession.h:244.
    pub fn account_id(&self) -> u32 {
        self.account_id
    }

    /// WorldSession.cpp:1117-1139. The account query owner supplies the result.
    pub fn bind_tutorials(&mut self, row: Option<[u32; 8]>) {
        self.tutorial_words = row.unwrap_or_default();
        self.tutorial_state = if row.is_some() {
            TutorialState::Unchanged
        } else {
            TutorialState::New
        };
    }

    /// WorldSession.h:357-365. A new row keeps its insert state.
    pub fn set_tutorial_word(&mut self, index: usize, value: u32) -> Result<()> {
        let word = self
            .tutorial_words
            .get_mut(index)
            .ok_or_else(|| refuse("execution.session.WorldSession_359.tutorial_index_undefined"))?;
        if *word != value {
            *word = value;
            if matches!(self.tutorial_state, TutorialState::Unchanged) {
                self.tutorial_state = TutorialState::Changed;
            }
        }
        Ok(())
    }

    // WorldSession.cpp:160-164. The kick deadline is not a packet-send operand.
    pub fn set_in_char_selection(&mut self) {
        self.state = SourceWorldSessionState::CharSelection;
    }

    // WorldSession.cpp:197-213. The pinned build disables both playerbot options.
    pub fn send_packet(
        &self,
        opcode: u16,
        payload: &[u8],
        forced: bool,
        output: impl FnOnce(u16, &[u8]) -> Result<()>,
    ) -> Result<()> {
        if !self.socket_present || (self.state != SourceWorldSessionState::Ready && !forced) {
            return Ok(());
        }
        output(opcode, payload)
    }
}

#[test]
fn session_retains_the_supplied_account() {
    let session = SourceWorldSession::construct(u32::MAX, false);
    assert_eq!(session.account_id(), u32::MAX);
}

#[test]
fn session_send_checks_socket_before_ready_and_force() {
    let refuse_output = |opcode, body: &[u8]| {
        assert_eq!((opcode, body), (0x130, &[204, 0, 0, 0, 0][..]));
        Err(refuse(
            "execution.session.WorldSession_252.packet_output_owner",
        ))
    };
    let body = [204, 0, 0, 0, 0];
    let expected = "execution.session.WorldSession_252.packet_output_owner";
    let mut live = SourceWorldSession::construct(19, true);
    live.set_in_char_selection();
    live.send_packet(0x130, &body, false, refuse_output)
        .unwrap();
    assert_eq!(
        live.send_packet(0x130, &body, true, refuse_output)
            .unwrap_err()
            .id,
        expected
    );
    live.state = SourceWorldSessionState::Ready;
    assert_eq!(
        live.send_packet(0x130, &body, false, refuse_output)
            .unwrap_err()
            .id,
        expected
    );
    let mut no_socket = SourceWorldSession::construct(19, false);
    no_socket.state = SourceWorldSessionState::Ready;
    no_socket
        .send_packet(0x130, &body, true, refuse_output)
        .unwrap();
}
