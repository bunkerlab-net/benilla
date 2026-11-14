//! Fork-only private tells over the authenticated owner's direct connection.
//! The stock CHAT_MSG_WHISPER / WHISPER_INFORM events retain name links and /reply.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use benilla_p2p::PeerId;
use benilla_profile::Character;
use benilla_protocol::{messages, SessionEvent};
use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use super::{character_guid, character_name, chat, emit};

const MAX_PENDING: usize = 16;
const TIMEOUT: Duration = Duration::from_secs(10);
const BURST: usize = 4;
const WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub(super) enum Packet {
    Tell {
        sender: [u8; 16],
        recipient: [u8; 16],
        sequence: u64,
        text: String,
    },
    Receipt {
        sender: [u8; 16],
        recipient: [u8; 16],
        sequence: u64,
    },
}
impl Packet {
    pub(super) fn validate(&self) -> Result<()> {
        match self {
            Self::Tell { sequence, text, .. } if *sequence != 0 && valid_text(text) => Ok(()),
            Self::Receipt { sequence, .. } if *sequence != 0 => Ok(()),
            _ => bail!("invalid private chat frame"),
        }
    }
}
// CMaNGOS 8ec338a ChatHandler.cpp: normal players cannot whisper across teams
// with AllowTwoSide.Interaction.Chat=0. This P2P mode grants no GM/chat override.
fn same_team(own: &Character, other: &Character) -> bool {
    (1..=8).contains(&own.race)
        && (1..=8).contains(&other.race)
        && crate::ui_unit::race_pvp_team(own.race) == crate::ui_unit::race_pvp_team(other.race)
}
fn valid_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= 255 && !text.chars().any(char::is_control)
}
#[derive(Default)]
struct Rate {
    start: Option<Instant>,
    count: usize,
}
impl Rate {
    fn admit(&mut self, now: Instant) -> bool {
        if self
            .start
            .is_none_or(|start| now.duration_since(start) >= WINDOW)
        {
            self.start = Some(now);
            self.count = 0;
        }
        if self.count >= BURST {
            return false;
        }
        self.count += 1;
        true
    }
}
struct Pending {
    owner: PeerId,
    recipient: Character,
    text: String,
    sent: Instant,
}
#[derive(Default)]
pub(super) struct State {
    next: u64,
    pending: BTreeMap<u64, Pending>,
    incoming: BTreeMap<PeerId, (u64, Rate)>,
    outgoing: Rate,
}
impl State {
    pub(super) fn send(
        &mut self,
        events: &Sender<SessionEvent>,
        own: &Character,
        peers: &BTreeMap<PeerId, Option<Character>>,
        target: Option<&str>,
        text: String,
        mut send: impl FnMut(PeerId, Packet) -> Result<()>,
    ) -> Result<()> {
        let Some(target) = target.filter(|name| {
            !name.is_empty() && name.len() <= 48 && !name.chars().any(char::is_control)
        }) else {
            return emit(
                events,
                chat(
                    0,
                    "Whisper a connected character with /w Name message.".into(),
                ),
            );
        };
        if !valid_text(&text) {
            return emit(
                events,
                chat(
                    0,
                    "P2P whispers accept 1–255 bytes without control characters.".into(),
                ),
            );
        }
        let mut matches = peers
            .iter()
            .filter_map(|(owner, character)| {
                character
                    .as_ref()
                    .filter(|c| c.name.eq_ignore_ascii_case(target))
                    .map(|c| (*owner, c))
            })
            .chain(
                own.name
                    .eq_ignore_ascii_case(target)
                    .then_some((own.owner_id, own)),
            );
        let Some((owner, recipient)) = matches.next() else {
            return emit(
                events,
                SessionEvent::ChatPlayerNotFound {
                    name: target.into(),
                },
            );
        };
        if matches.next().is_some() {
            return emit(
                events,
                chat(
                    0,
                    "More than one connected character has that name; whisper was not sent.".into(),
                ),
            );
        }
        if !same_team(own, recipient) {
            return emit(events, SessionEvent::ChatWrongFaction);
        }
        let now = Instant::now();
        if self.pending.len() >= MAX_PENDING || !self.outgoing.admit(now) {
            return emit(
                events,
                chat(
                    0,
                    "You are sending whispers too quickly. Please wait.".into(),
                ),
            );
        }
        if owner == own.owner_id {
            emit(events, event(own, text.clone(), false))?;
            return emit(events, event(own, text, true));
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("private chat sequence exhausted"))?;
        let sequence = self.next;
        // This callback is backed only by Command::Send, never a broadcast envelope.
        send(
            owner,
            Packet::Tell {
                sender: own.id,
                recipient: recipient.id,
                sequence,
                text: text.clone(),
            },
        )?;
        self.pending.insert(
            sequence,
            Pending {
                owner,
                recipient: recipient.clone(),
                text,
                sent: now,
            },
        );
        Ok(())
    }
    pub(super) fn receive(
        &mut self,
        events: &Sender<SessionEvent>,
        own: &Character,
        peers: &BTreeMap<PeerId, Option<Character>>,
        owner: PeerId,
        packet: Packet,
        mut send: impl FnMut(PeerId, Packet) -> Result<()>,
    ) -> Result<()> {
        packet.validate()?;
        let Some(Some(other)) = peers.get(&owner) else {
            bail!("private chat before avatar admission");
        };
        if other.owner_id != owner {
            bail!("private chat owner mismatch");
        }
        // Check admitted identities again on ingress, including delivery receipts.
        // A modified peer cannot bypass the outgoing refusal or cause a false self-echo.
        if !same_team(own, other) {
            bail!("private chat wrong faction");
        }
        match packet {
            Packet::Tell {
                sender,
                recipient,
                sequence,
                text,
            } => {
                if sender != other.id || recipient != own.id {
                    bail!("private chat character mismatch");
                }
                let (last, rate) = self.incoming.entry(owner).or_default();
                if sequence <= *last {
                    bail!("replayed private chat");
                }
                *last = sequence;
                if !rate.admit(Instant::now()) {
                    bail!("private chat rate exceeded");
                }
                // Names are from the admitted descriptor, including peers on other maps.
                emit(events, character_name(other))?;
                emit(events, event(other, text, false))?;
                send(
                    owner,
                    Packet::Receipt {
                        sender: own.id,
                        recipient: other.id,
                        sequence,
                    },
                )?;
            }
            Packet::Receipt {
                sender,
                recipient,
                sequence,
            } => {
                if sender != other.id || recipient != own.id {
                    bail!("private chat receipt mismatch");
                }
                let Some(pending) = self.pending.get(&sequence) else {
                    return Ok(());
                };
                if pending.owner != owner || pending.recipient.id != sender {
                    bail!("private chat receipt owner mismatch");
                }
                let pending = self
                    .pending
                    .remove(&sequence)
                    .expect("checked pending tell");
                emit(events, character_name(&pending.recipient))?;
                emit(events, event(&pending.recipient, pending.text, true))?;
            }
        }
        Ok(())
    }
    pub(super) fn disconnected(
        &mut self,
        events: &Sender<SessionEvent>,
        owner: PeerId,
    ) -> Result<()> {
        // Transport closes the old authenticated stream and discards its queued frames.
        // A new admission starts a new receive sequence; no pending plaintext is retried.
        self.incoming.remove(&owner);
        let failed = self.pending.values().any(|p| p.owner == owner);
        self.pending.retain(|_, p| p.owner != owner);
        if failed {
            emit(
                events,
                chat(
                    0,
                    "Whisper recipient disconnected before confirming receipt.".into(),
                ),
            )?;
        }
        Ok(())
    }
    pub(super) fn expire(&mut self, events: &Sender<SessionEvent>) -> Result<()> {
        let before = self.pending.len();
        self.pending.retain(|_, p| p.sent.elapsed() < TIMEOUT);
        if before != self.pending.len() {
            emit(
                events,
                chat(
                    0,
                    "Whisper delivery was not confirmed; the recipient may be offline.".into(),
                ),
            )?;
        }
        Ok(())
    }
}
fn event(character: &Character, text: String, outgoing: bool) -> SessionEvent {
    SessionEvent::Chat(messages::ChatMessage {
        chat_type: if outgoing {
            messages::CHAT_MSG_WHISPER_INFORM
        } else {
            messages::CHAT_MSG_WHISPER
        },
        language: 0,
        sender_guid: character_guid(character),
        target_guid: 0,
        sender_name: Some(character.name.clone()),
        channel: None,
        text,
        chat_tag: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_profile::{Appearance, SpawnPolicy};
    fn avatar(owner: u8, name: &str) -> Character {
        Character {
            id: [owner + 10; 16],
            owner_id: [owner; 16],
            name: name.into(),
            race: 1,
            class: 1,
            gender: 0,
            appearance: Appearance::default(),
            level: 1,
            spawn_policy: SpawnPolicy::NorthshireMeetingV1,
            position: SpawnPolicy::NorthshireMeetingV1.position().unwrap(),
        }
    }
    fn peers(characters: &[Character]) -> BTreeMap<PeerId, Option<Character>> {
        characters
            .iter()
            .map(|c| (c.owner_id, Some(c.clone())))
            .collect()
    }
    fn tell(from: &Character, to: &Character, sequence: u64) -> Packet {
        Packet::Tell {
            sender: from.id,
            recipient: to.id,
            sequence,
            text: "private text".into(),
        }
    }
    #[test]
    fn classic_race_pairs_reject_cross_faction_without_sending_or_charging_a_rate_slot() {
        for from_race in 1..=8 {
            for to_race in 1..=8 {
                let mut a = avatar(1, "Human");
                let mut b = avatar(2, "Forsaken");
                a.race = from_race;
                b.race = to_race;
                let alliance = |race| matches!(race, 1 | 3 | 4 | 7);
                let allowed = alliance(from_race) == alliance(to_race);
                let (tx, rx) = crossbeam_channel::bounded(8);
                let mut state = State::default();
                let mut sent = 0;
                state
                    .send(
                        &tx,
                        &a,
                        &peers(std::slice::from_ref(&b)),
                        Some(&b.name),
                        "private".into(),
                        |owner, _| {
                            assert_eq!(owner, b.owner_id);
                            sent += 1;
                            Ok(())
                        },
                    )
                    .unwrap();
                assert_eq!(sent, usize::from(allowed), "races {from_race} -> {to_race}");
                if allowed {
                    assert!(rx.is_empty(), "delivery echo still waits for receipt");
                    assert_eq!(state.pending.len(), 1);
                } else {
                    assert!(matches!(rx.recv().unwrap(), SessionEvent::ChatWrongFaction));
                    assert!(rx.is_empty(), "no whisper echo or chime event");
                    assert!(state.pending.is_empty());
                    assert_eq!(state.next, 0);
                    assert_eq!(state.outgoing.count, 0);
                }
            }
        }
    }

    #[test]
    fn unknown_races_cannot_send_even_to_the_same_unknown_race() {
        for race in [0, 9, 255] {
            let mut own = avatar(1, "Local");
            own.race = race;
            let (tx, rx) = crossbeam_channel::bounded(4);
            let mut state = State::default();
            state
                .send(
                    &tx,
                    &own,
                    &BTreeMap::new(),
                    Some(&own.name),
                    "no".into(),
                    |_, _| panic!("invalid race send"),
                )
                .unwrap();
            assert!(matches!(rx.recv().unwrap(), SessionEvent::ChatWrongFaction));
            assert!(rx.is_empty());
        }
    }

    #[test]
    fn cross_faction_or_unknown_admitted_races_cannot_deliver_tells_or_receipts() {
        for (own_race, other_race) in [(1, 5), (5, 1), (1, 0), (0, 1), (9, 1), (1, 9), (255, 255)] {
            let mut own = avatar(1, "Local");
            let mut other = avatar(2, "Remote");
            own.race = own_race;
            other.race = other_race;
            let (tx, rx) = crossbeam_channel::bounded(8);
            let mut state = State::default();
            state.pending.insert(
                1,
                Pending {
                    owner: other.owner_id,
                    recipient: other.clone(),
                    text: "must not echo".into(),
                    sent: Instant::now(),
                },
            );
            for packet in [
                tell(&other, &own, 1),
                Packet::Receipt {
                    sender: other.id,
                    recipient: own.id,
                    sequence: 1,
                },
            ] {
                assert!(state
                    .receive(
                        &tx,
                        &own,
                        &peers(std::slice::from_ref(&other)),
                        other.owner_id,
                        packet,
                        |_, _| panic!("no receipt to refused tell")
                    )
                    .is_err());
                assert!(rx.is_empty(), "no display/name/sound-producing chat event");
                assert!(state.incoming.is_empty());
                assert_eq!(state.pending.len(), 1, "refusal cannot claim delivery");
            }
        }
    }

    #[test]
    fn different_racial_languages_on_the_same_team_still_deliver_universal_whispers() {
        for (from_race, to_race) in [(1, 4), (3, 7), (2, 5), (6, 8)] {
            let mut a = avatar(1, "Sender");
            let mut b = avatar(2, "Recipient");
            a.race = from_race;
            b.race = to_race;
            let (tx, rx) = crossbeam_channel::bounded(8);
            let mut state = State::default();
            let mut receipts = 0;
            state
                .receive(
                    &tx,
                    &b,
                    &peers(std::slice::from_ref(&a)),
                    a.owner_id,
                    tell(&a, &b, 1),
                    |_, _| {
                        receipts += 1;
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(receipts, 1);
            assert!(matches!(
                rx.recv().unwrap(),
                SessionEvent::PlayerName { .. }
            ));
            assert!(matches!(rx.recv().unwrap(), SessionEvent::Chat(m)
                if m.language == 0 && m.chat_type == messages::CHAT_MSG_WHISPER && m.text == "private text"));
            assert!(rx.is_empty());
        }
    }

    #[test]
    fn private_tell_routes_one_owner_and_echoes_only_after_bound_receipt_across_maps() {
        let a = avatar(1, "Alice");
        let mut b = avatar(2, "Bob");
        let c = avatar(3, "Carol");
        b.position.map_id = 1;
        let (tx, rx) = crossbeam_channel::bounded(16);
        let mut sender = State::default();
        let mut receiver = State::default();
        let mut packets = Vec::new();
        sender
            .send(
                &tx,
                &a,
                &peers(&[b.clone(), c.clone()]),
                Some("bOB"),
                "secret".into(),
                |owner, p| {
                    packets.push((owner, p));
                    Ok(())
                },
            )
            .unwrap();
        assert!(rx.is_empty(), "no false delivery echo");
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, b.owner_id);
        let (_, packet) = packets.pop().unwrap();
        receiver
            .receive(
                &tx,
                &b,
                &peers(&[a.clone(), c]),
                a.owner_id,
                packet,
                |owner, p| {
                    packets.push((owner, p));
                    Ok(())
                },
            )
            .unwrap();
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::PlayerName { name, .. } if name == "Alice")
        );
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::Chat(m) if m.chat_type == messages::CHAT_MSG_WHISPER && m.text == "secret" && m.sender_guid == character_guid(&a))
        );
        let (owner, ack) = packets.pop().unwrap();
        assert_eq!(owner, a.owner_id);
        sender
            .receive(
                &tx,
                &a,
                &peers(std::slice::from_ref(&b)),
                b.owner_id,
                ack.clone(),
                |_, _| panic!("no reply to receipt"),
            )
            .unwrap();
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::PlayerName { name, .. } if name == "Bob")
        );
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::Chat(m) if m.chat_type == messages::CHAT_MSG_WHISPER_INFORM && m.sender_guid == character_guid(&b))
        );
        sender
            .receive(
                &tx,
                &a,
                &peers(std::slice::from_ref(&b)),
                b.owner_id,
                ack,
                |_, _| unreachable!(),
            )
            .unwrap();
        assert!(rx.is_empty(), "duplicate receipt does not echo twice");
    }
    #[test]
    fn rejects_forged_sender_recipient_unadmitted_owner_and_replay() {
        let a = avatar(1, "Alice");
        let b = avatar(2, "Bob");
        let c = avatar(3, "Carol");
        let (tx, rx) = crossbeam_channel::bounded(16);
        let mut state = State::default();
        for packet in [tell(&c, &b, 1), tell(&a, &c, 1)] {
            assert!(state
                .receive(
                    &tx,
                    &b,
                    &peers(std::slice::from_ref(&a)),
                    a.owner_id,
                    packet,
                    |_, _| panic!("forgery ack")
                )
                .is_err());
            assert!(rx.is_empty());
        }
        assert!(state
            .receive(
                &tx,
                &b,
                &BTreeMap::new(),
                a.owner_id,
                tell(&a, &b, 1),
                |_, _| unreachable!()
            )
            .is_err());
        state
            .receive(
                &tx,
                &b,
                &peers(std::slice::from_ref(&a)),
                a.owner_id,
                tell(&a, &b, 1),
                |_, _| Ok(()),
            )
            .unwrap();
        rx.try_iter().for_each(drop);
        assert!(state
            .receive(
                &tx,
                &b,
                &peers(std::slice::from_ref(&a)),
                a.owner_id,
                tell(&a, &b, 1),
                |_, _| unreachable!()
            )
            .is_err());
        assert!(rx.is_empty());
    }
    #[test]
    fn unresolved_ambiguous_and_invalid_tells_never_send() {
        let a = avatar(1, "Alice");
        let b = avatar(2, "Bob");
        let c = avatar(3, "Bob");
        let (tx, rx) = crossbeam_channel::bounded(16);
        let mut state = State::default();
        state
            .send(
                &tx,
                &a,
                &peers(std::slice::from_ref(&b)),
                Some("Offline"),
                "hi".into(),
                |_, _| panic!("offline send"),
            )
            .unwrap();
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::ChatPlayerNotFound { name } if name == "Offline")
        );
        state
            .send(
                &tx,
                &a,
                &peers(&[b.clone(), c]),
                Some("Bob"),
                "hi".into(),
                |_, _| panic!("ambiguous send"),
            )
            .unwrap();
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::Chat(m) if m.text.contains("More than one"))
        );
        for text in [
            String::new(),
            "a".repeat(256),
            "bad\ntext".into(),
            "é".repeat(128),
        ] {
            assert!(!valid_text(&text));
            state
                .send(
                    &tx,
                    &a,
                    &peers(std::slice::from_ref(&b)),
                    Some("Bob"),
                    text,
                    |_, _| panic!("invalid send"),
                )
                .unwrap();
        }
        assert!(valid_text(&"a".repeat(255)));
    }
    #[test]
    fn bounded_spam_disconnect_and_timeout_never_retry_plaintext() {
        let a = avatar(1, "Alice");
        let b = avatar(2, "Bob");
        let (tx, rx) = crossbeam_channel::bounded(32);
        let mut state = State::default();
        let mut sent = 0;
        for _ in 0..6 {
            state
                .send(
                    &tx,
                    &a,
                    &peers(std::slice::from_ref(&b)),
                    Some("Bob"),
                    "hi".into(),
                    |_, _| {
                        sent += 1;
                        Ok(())
                    },
                )
                .unwrap();
        }
        assert_eq!(sent, BURST);
        assert_eq!(state.pending.len(), BURST);
        state.disconnected(&tx, b.owner_id).unwrap();
        assert!(state.pending.is_empty());
        state
            .receive(
                &tx,
                &a,
                &peers(std::slice::from_ref(&b)),
                b.owner_id,
                Packet::Receipt {
                    sender: b.id,
                    recipient: a.id,
                    sequence: 1,
                },
                |_, _| panic!("retry"),
            )
            .unwrap();
        assert!(rx.try_iter().all(
            |e| matches!(e, SessionEvent::Chat(m) if m.chat_type == messages::CHAT_MSG_SYSTEM)
        ));
        state.pending.insert(
            99,
            Pending {
                owner: b.owner_id,
                recipient: b,
                text: "expired".into(),
                sent: Instant::now() - TIMEOUT,
            },
        );
        state.expire(&tx).unwrap();
        assert!(state.pending.is_empty());
        assert!(
            matches!(rx.recv().unwrap(), SessionEvent::Chat(m) if m.text.contains("not confirmed"))
        );
    }
    #[test]
    fn incoming_flood_is_bounded_and_new_admission_resets_only_receive_state() {
        let a = avatar(1, "Alice");
        let b = avatar(2, "Bob");
        let (tx, rx) = crossbeam_channel::bounded(32);
        let mut state = State::default();
        for sequence in 1..=BURST as u64 {
            state
                .receive(
                    &tx,
                    &b,
                    &peers(std::slice::from_ref(&a)),
                    a.owner_id,
                    tell(&a, &b, sequence),
                    |_, _| Ok(()),
                )
                .unwrap();
        }
        assert!(state
            .receive(
                &tx,
                &b,
                &peers(std::slice::from_ref(&a)),
                a.owner_id,
                tell(&a, &b, 5),
                |_, _| unreachable!()
            )
            .is_err());
        rx.try_iter().for_each(drop);
        state.disconnected(&tx, a.owner_id).unwrap();
        state
            .receive(
                &tx,
                &b,
                &peers(std::slice::from_ref(&a)),
                a.owner_id,
                tell(&a, &b, 1),
                |_, _| Ok(()),
            )
            .unwrap();
        assert_eq!(rx.len(), 2);
    }
}
