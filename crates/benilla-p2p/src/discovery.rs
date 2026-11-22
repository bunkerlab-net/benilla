//! Discovery primitives and opt-in candidate input; no publisher or lookup runs here.
//!
//! Discovery only supplies untrusted candidates. Admission must still complete the existing
//! PSK and persistent owner handshake while the caller retains the exclusive profile lock.
//! A published password-derived topic or encrypted announcement permits offline guessing.
//! Short and empty passphrases remain accepted by config; HKDF does not add password entropy.

use crate::{internet::parse_ticket, Error};
use iroh::{EndpointAddr, EndpointId, RelayUrl};
use ring::hkdf;
use std::{
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const SALT: &[u8] = b"benilla-discovery-v1\0";
const CANDIDATE_TTL: Duration = Duration::from_secs(120);
const MIN_RESIDENCE: Duration = Duration::from_secs(30);
const EVICTION_INTERVAL: Duration = Duration::from_secs(5);
const INPUT_QUEUE: usize = 32;
const MAX_RETRY: Duration = Duration::from_secs(60);

/// Separate discovery outputs from the existing Argon2id-derived transport PSK.
/// Intentionally neither Debug nor Serialize: these values are not diagnostics.
pub struct DiscoveryKeys {
    topic: Zeroizing<[u8; 32]>,
    announcement: Zeroizing<[u8; 32]>,
}

impl DiscoveryKeys {
    /// `psk` must be the config's derived key, never raw passphrase bytes. Network encoding
    /// matches Hello: exact UTF-8, 1..=128 bytes, without normalization or case folding.
    pub fn derive(psk: &[u8; 32], network: &str) -> Result<Self, Error> {
        if network.is_empty() || network.len() > 128 {
            return Err(Error::Protocol("invalid discovery network"));
        }
        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, SALT).extract(psk);
        let length = (network.len() as u32).to_be_bytes();
        let expand = |role: &[u8]| -> Result<Zeroizing<[u8; 32]>, Error> {
            let mut output = Zeroizing::new([0; 32]);
            prk.expand(&[role, &length, network.as_bytes()], hkdf::HKDF_SHA256)
                .map_err(|_| Error::Crypto)?
                .fill(&mut *output)
                .map_err(|_| Error::Crypto)?;
            Ok(output)
        };
        Ok(Self {
            topic: expand(b"topic\0")?,
            announcement: expand(b"announcement-encryption\0")?,
        })
    }

    /// Rendezvous identifier, not authentication or an owner identity.
    pub fn topic(&self) -> &[u8; 32] {
        &self.topic
    }

    /// For a future announcement codec only; never use as an owner or endpoint private key.
    pub fn announcement_key(&self) -> &[u8; 32] {
        &self.announcement
    }
}

struct Candidate {
    address: EndpointAddr,
    expires: Instant,
    next_attempt: Instant,
    retry: Duration,
    admitted: Instant,
    attempted: bool,
}

/// Bounded, expiring candidates for an eventual discovery adapter. This does not connect,
/// authenticate, authorize, or modify a running Network. All times are caller-supplied
/// monotonic times, permitting deterministic tests without sockets or sleeping.
/// The live adapter can admit newcomers by evicting old, attempted idle candidates at a
/// bounded rate. Refreshes never reset residence or backoff. This limits churn and permanent
/// incumbent pinning, but cannot guarantee admission against an unlimited Sybil attacker.
pub struct CandidatePool {
    local: EndpointId,
    limit: usize,
    allowed_relays: Vec<RelayUrl>,
    candidates: BTreeMap<EndpointId, Candidate>,
    last_eviction: Option<Instant>,
}

impl CandidatePool {
    pub fn new(
        local: EndpointId,
        limit: usize,
        allowed_relays: Vec<RelayUrl>,
    ) -> Result<Self, Error> {
        if limit == 0 || limit > 64 || allowed_relays.len() > 8 {
            return Err(Error::Protocol("invalid discovery limits"));
        }
        // Reuse the transport's HTTPS/credential/duplicate validation before accepting peers.
        crate::InternetConfig {
            relay_urls: allowed_relays.iter().map(ToString::to_string).collect(),
            ..Default::default()
        }
        .validate()?;
        Ok(Self {
            local,
            limit,
            allowed_relays,
            candidates: BTreeMap::new(),
            last_eviction: None,
        })
    }

    /// Returns false for self or a full pool. Refreshes never bypass a candidate's backoff.
    /// Unknown relays and malformed addresses fail closed, as with manual endpoint tickets.
    pub fn observe(&mut self, ticket: &str, now: Instant) -> Result<bool, Error> {
        self.observe_inner(ticket, now, None)
    }

    pub(super) fn observe_protected(
        &mut self,
        ticket: &str,
        now: Instant,
        protected: &HashSet<EndpointId>,
    ) -> Result<bool, Error> {
        self.observe_inner(ticket, now, Some(protected))
    }

    fn observe_inner(
        &mut self,
        ticket: &str,
        now: Instant,
        protected: Option<&HashSet<EndpointId>>,
    ) -> Result<bool, Error> {
        let address = parse_ticket(ticket)?;
        if address
            .relay_urls()
            .any(|relay| !self.allowed_relays.contains(relay))
        {
            return Err(Error::Protocol(
                "discovered relay missing from explicit allowlist",
            ));
        }
        if address.id == self.local {
            return Ok(false);
        }
        let expires = now
            .checked_add(CANDIDATE_TTL)
            .ok_or(Error::Protocol("invalid discovery time"))?;
        self.expire(now);
        if let Some(candidate) = self.candidates.get_mut(&address.id) {
            candidate.address = address;
            candidate.expires = expires;
            return Ok(true);
        }
        if self.candidates.len() >= self.limit {
            let Some(protected) = protected else {
                return Ok(false);
            };
            if self
                .last_eviction
                .is_some_and(|last| now.saturating_duration_since(last) < EVICTION_INTERVAL)
            {
                return Ok(false);
            }
            let victim = self
                .candidates
                .iter()
                .filter(|(id, c)| {
                    !protected.contains(*id)
                        && c.attempted
                        && now.saturating_duration_since(c.admitted) >= MIN_RESIDENCE
                })
                .min_by_key(|(id, c)| (c.admitted, **id))
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                return Ok(false);
            };
            self.candidates.remove(&victim);
            self.last_eviction = Some(now);
        }
        self.candidates.insert(
            address.id,
            Candidate {
                address,
                expires,
                next_attempt: now,
                retry: Duration::from_secs(1),
                admitted: now,
                attempted: false,
            },
        );
        Ok(true)
    }

    /// Reserves at most one attempt and schedules its next retry. The caller must apply
    /// existing handshake permits and suppress dialing already connected/in-flight IDs.
    /// Choose earliest due time rather than key order, so a noisy low-ID peer cannot starve
    /// other due candidates. A future live adapter can add bounded jitter to its wake time.
    pub fn next_due(&mut self, now: Instant) -> Option<EndpointAddr> {
        self.next_due_excluding(now, &HashSet::new())
    }

    pub(super) fn next_due_excluding(
        &mut self,
        now: Instant,
        protected: &HashSet<EndpointId>,
    ) -> Option<EndpointAddr> {
        self.expire(now);
        let id = self
            .candidates
            .iter()
            .filter(|(id, candidate)| candidate.next_attempt <= now && !protected.contains(*id))
            .min_by_key(|(id, candidate)| (candidate.next_attempt, **id))
            .map(|(id, _)| *id)?;
        let candidate = self.candidates.get_mut(&id)?;
        candidate.next_attempt = now.checked_add(candidate.retry)?;
        candidate.retry = (candidate.retry * 2).min(MAX_RETRY);
        candidate.attempted = true;
        Some(candidate.address.clone())
    }

    pub fn remove(&mut self, id: &EndpointId) {
        self.candidates.remove(id);
    }

    pub fn expire(&mut self, now: Instant) {
        self.candidates
            .retain(|_, candidate| candidate.expires > now);
    }

    pub(super) fn is_due(&self, id: &EndpointId, now: Instant) -> bool {
        self.candidates
            .get(id)
            .is_some_and(|candidate| candidate.expires > now && candidate.next_attempt <= now)
    }

    pub(super) fn contains(&self, id: &EndpointId) -> bool {
        self.candidates.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// Bounded local injection seam; constructing this starts no tasks or discovery traffic.
pub(super) struct CandidateInput {
    sender: tokio::sync::mpsc::Sender<String>,
    allowed_relays: Vec<RelayUrl>,
}
impl CandidateInput {
    pub(super) fn submit(&self, ticket: &str) -> Result<(), Error> {
        let address = parse_ticket(ticket)?;
        if address
            .relay_urls()
            .any(|relay| !self.allowed_relays.contains(relay))
        {
            return Err(Error::Protocol(
                "discovered relay missing from explicit allowlist",
            ));
        }
        self.sender
            .try_send(ticket.to_owned())
            .map_err(|_| Error::Protocol("candidate input full or closed"))
    }
}
pub(super) struct Adapter {
    pub(super) pool: CandidatePool,
    receiver: tokio::sync::mpsc::Receiver<String>,
}
impl Adapter {
    pub(super) fn new(
        local: EndpointId,
        limit: usize,
        allowed_relays: Vec<RelayUrl>,
    ) -> Result<(CandidateInput, Self), Error> {
        let pool = CandidatePool::new(local, limit, allowed_relays.clone())?;
        let (sender, receiver) = tokio::sync::mpsc::channel(INPUT_QUEUE);
        Ok((
            CandidateInput {
                sender,
                allowed_relays,
            },
            Self { pool, receiver },
        ))
    }
}
pub(super) async fn receive(adapter: &mut Option<Adapter>) -> String {
    if let Some(adapter) = adapter {
        if let Some(ticket) = adapter.receiver.recv().await {
            return ticket;
        }
    }
    std::future::pending().await
}

#[cfg(test)]
mod tests;
