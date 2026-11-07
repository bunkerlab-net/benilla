//! Fork-only local recovery requests: stable owner/identity targets, never roster indices.
use benilla_profile::Character;
use std::sync::mpsc::Sender;

pub(crate) type Reply = Result<Vec<Character>, String>;
pub(crate) enum Request {
    List(Sender<Reply>),
    Restore {
        target: Character,
        reply: Sender<Reply>,
    },
}
impl Request {
    pub(crate) fn reject(self) {
        let reply = match self {
            Self::List(reply) | Self::Restore { reply, .. } => reply,
        };
        let _ = reply.send(Err(
            "Recovery is available only for local characters.".into()
        ));
    }
}

/// A second click confirms the exact record shown. Taking it makes repeated clicks inert.
#[derive(Default)]
pub(crate) struct Confirmation(pub(crate) Option<Character>);
impl Confirmation {
    pub(crate) fn confirm(&mut self, rows: &[Character]) -> Option<Character> {
        self.0.take().filter(|target| rows.contains(target))
    }
}
