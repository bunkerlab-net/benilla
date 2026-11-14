//! Owner authentication layered on the existing PSK-encrypted stream. The transcript uses
//! fixed-width integers and length-prefixed strings, independent of JSON serialization.
use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub(super) const PROLOGUE: &[u8] = b"benilla-p2p-v2-owned-v1";
const OWNER_DOMAIN: &[u8] = b"benilla-owner-id-v1\0";
const BINDING_DOMAIN: &[u8] = b"benilla-owner-connection-v1\0";
const SIGNING_DOMAIN: &[u8] = b"benilla-transport-identity-v1\0";
const MAX_HELLO: usize = 4096;
type Signer = dyn Fn(&[u8; 32]) -> [u8; 64] + Send + Sync + 'static;

/// Immutable public identity and a narrowly scoped signer. The profile retains its secret;
/// this handle never serializes or prints it. Clones share the callback.
///
/// `signer` must return an Ed25519 signature of the exact bytes
/// `b"benilla-transport-identity-v1\0" || challenge`, using the corresponding private key.
/// Only transport-built, domain-separated connection transcript digests reach this callback.
#[derive(Clone)]
pub struct OwnerIdentity {
    public_key: [u8; 32],
    signer: Arc<Signer>,
}
impl OwnerIdentity {
    pub fn new(
        public_key: [u8; 32],
        signer: impl Fn(&[u8; 32]) -> [u8; 64] + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        verifying_key(&public_key)?;
        Ok(Self {
            public_key,
            signer: Arc::new(signer),
        })
    }
    pub fn owner_id(&self) -> PeerId {
        owner_id_from_public_key(&self.public_key)
    }
    pub(super) fn validate_hello(&self, hello: &Hello) -> Result<(), Error> {
        hello.validate()?;
        if hello.peer_id != self.owner_id() {
            return Err(Error::Protocol(
                "local peer ID does not match owner public key",
            ));
        }
        Ok(())
    }
}

/// Canonical persistent owner ID, also used by the local character profile format.
/// Authentication still requires a valid Ed25519 signature, not just this public hash.
pub fn owner_id_from_public_key(public_key: &[u8; 32]) -> PeerId {
    let mut hash = Sha256::new();
    hash.update(OWNER_DOMAIN);
    hash.update(public_key);
    hash.finalize()[..16].try_into().unwrap()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedHello {
    hello: Hello,
    public_key: [u8; 32],
}
impl OwnedHello {
    fn validate(&self) -> Result<(), Error> {
        self.hello.validate()?;
        verifying_key(&self.public_key)?;
        if self.hello.peer_id != owner_id_from_public_key(&self.public_key) {
            return Err(Error::Protocol("peer ID does not match owner public key"));
        }
        Ok(())
    }
    fn bind(&self, hash: &mut Sha256) {
        let hello = &self.hello;
        hash.update(hello.protocol_version.to_be_bytes());
        hash.update((hello.network.len() as u32).to_be_bytes());
        hash.update(hello.network.as_bytes());
        hash.update(hello.peer_id);
        hash.update(hello.ruleset_hash);
        hash.update((hello.content_version.len() as u32).to_be_bytes());
        hash.update(hello.content_version.as_bytes());
        hash.update(self.public_key);
    }
}
fn verifying_key(public_key: &[u8; 32]) -> Result<VerifyingKey, Error> {
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| Error::Crypto)?;
    if key.is_weak() {
        return Err(Error::Crypto);
    }
    Ok(key)
}
fn challenge(
    binding: &[u8; 32],
    initiator: &OwnedHello,
    responder: &OwnedHello,
    signer_is_initiator: bool,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(BINDING_DOMAIN);
    hash.update(binding);
    initiator.bind(&mut hash);
    responder.bind(&mut hash);
    // Role separation prevents reflecting a peer's proof back as the other endpoint.
    hash.update([u8::from(signer_is_initiator)]);
    hash.finalize().into()
}
fn verify(public_key: &[u8; 32], challenge: &[u8; 32], signature: &[u8]) -> Result<(), Error> {
    let signature = Signature::from_slice(signature).map_err(|_| Error::Crypto)?;
    let mut message = Vec::with_capacity(SIGNING_DOMAIN.len() + challenge.len());
    message.extend_from_slice(SIGNING_DOMAIN);
    message.extend_from_slice(challenge);
    verifying_key(public_key)?
        .verify_strict(&message, &signature)
        .map_err(|_| Error::Crypto)
}
async fn send<S: tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    noise: &mut snow::TransportState,
    clear: &[u8],
) -> Result<(), Error> {
    let mut wire = vec![0; clear.len() + 16];
    let size = noise.write_message(clear, &mut wire)?;
    write_wire(stream, &wire[..size]).await
}
async fn receive<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
    noise: &mut snow::TransportState,
    max_clear: usize,
) -> Result<Vec<u8>, Error> {
    // Validate before allocating; owned authentication has smaller bounds than game frames.
    let size = stream.read_u32().await? as usize;
    if size < 16 || size > max_clear + 16 {
        return Err(Error::Protocol("invalid owner authentication frame length"));
    }
    let mut wire = vec![0; size];
    stream.read_exact(&mut wire).await?;
    let mut clear = vec![0; max_clear];
    let size = noise.read_message(&wire, &mut clear)?;
    clear.truncate(size);
    Ok(clear)
}

pub(super) async fn authenticate<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    noise: &mut snow::TransportState,
    binding: &[u8; 32],
    hello: &Hello,
    identity: &OwnerIdentity,
    is_initiator: bool,
) -> Result<Hello, Error> {
    identity.validate_hello(hello)?;
    let local = OwnedHello {
        hello: hello.clone(),
        public_key: identity.public_key,
    };
    let bytes = serde_json::to_vec(&local).map_err(|_| Error::Protocol("invalid owner hello"))?;
    if bytes.len() > MAX_HELLO {
        return Err(Error::Protocol("owner hello too large"));
    }
    send(stream, noise, &bytes).await?;
    let bytes = receive(stream, noise, MAX_HELLO).await?;
    let remote: OwnedHello =
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol("invalid owner hello"))?;
    remote.validate()?;
    hello.compatible(&remote.hello)?;
    let (initiator, responder) = if is_initiator {
        (&local, &remote)
    } else {
        (&remote, &local)
    };
    let local_challenge = challenge(binding, initiator, responder, is_initiator);
    let signature = (identity.signer)(&local_challenge);
    // Fail locally before exposing a broken signer to a remote as authenticated identity.
    verify(&local.public_key, &local_challenge, &signature)?;
    send(stream, noise, &signature).await?;
    let signature = receive(stream, noise, 64).await?;
    verify(
        &remote.public_key,
        &challenge(binding, initiator, responder, !is_initiator),
        &signature,
    )?;
    Ok(remote.hello)
}

#[cfg(test)]
mod tests;
