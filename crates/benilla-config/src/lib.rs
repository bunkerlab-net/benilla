//! Fork-specific startup configuration, separate from the stock client's persisted CVars.
//! Never display TOML parser errors directly: they can contain the source and hence the PSK.

pub mod editor;

use argon2::{Algorithm, Argon2, Block, Params, Version};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::File,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use zeroize::{Zeroize, Zeroizing};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
pub const RULESET_VERSION: u32 = 1;
/// This version fixes passphrase encoding, salt construction and Argon2id parameters together.
/// Changing any of them requires a new version and a peer compatibility boundary.
pub const PASSPHRASE_KDF_VERSION: u32 = 1;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_ITERATIONS: u32 = 3;
const KDF_LANES: u32 = 4;
const MAX_PASSPHRASE_BYTES: usize = 1024;

#[derive(Clone, Debug, Default)]
pub struct AppConfig {
    pub p2p: Option<P2pConfig>,
    pub wow: WowConfig,
    pub ruleset: Ruleset,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WowConfig {
    /// Absolute after loading. A relative path is anchored at the executable, never the cwd.
    pub data: Option<PathBuf>,
}

#[derive(Clone)]
pub struct SecretPsk([u8; 32]);

impl fmt::Debug for SecretPsk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Drop for SecretPsk {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Debug)]
pub struct P2pConfig {
    pub encounter: Option<EncounterConfig>,
    pub internet: Option<benilla_p2p::InternetConfig>,
    pub network: String,
    pub listen: SocketAddr,
    pub peers: Vec<SocketAddr>,
    pub max_peers: u16,
    pub timeout_ms: u64,
    psk: SecretPsk,
}

/// Explicit opt-in synthetic encounter. IDs are public identifiers, never credentials.
#[derive(Clone, Debug)]
pub struct EncounterConfig {
    pub local_id: [u8; 16],
    pub epoch: [u8; 32],
    /// Sorted by ID: its index plus one is the deterministic simulation player ID.
    pub roster: Vec<EncounterMember>,
    pub tick_limit: u64,
}
#[derive(Clone, Debug)]
pub struct EncounterMember {
    pub id: [u8; 16],
    pub address: SocketAddr,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEncounter {
    local_id: String,
    epoch: String,
    roster: Vec<RawEncounterMember>,
    #[serde(default = "default_tick_limit")]
    tick_limit: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEncounterMember {
    id: String,
    address: SocketAddr,
}
fn default_tick_limit() -> u64 {
    1200
}
fn identifier<const N: usize>(s: &str) -> Result<[u8; N], ConfigError> {
    if s.len() != N * 2 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid(
            "encounter IDs/epoch require exact-length hexadecimal values",
        ));
    }
    let mut result = [0; N];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
    }
    if result == [0; N] {
        return Err(invalid("encounter IDs/epoch must be nonzero"));
    }
    Ok(result)
}
fn validate_encounter(
    raw: RawEncounter,
    listen: SocketAddr,
    peers: &[SocketAddr],
    max_peers: u16,
) -> Result<EncounterConfig, ConfigError> {
    if !peers.is_empty() {
        return Err(invalid("encounter mode uses its roster; omit p2p.peers"));
    }
    if !(2..=16).contains(&raw.roster.len()) || usize::from(max_peers) < raw.roster.len() - 1 {
        return Err(invalid(
            "encounter requires 2..16 members and enough p2p.max_peers",
        ));
    }
    if !(1..=100_000).contains(&raw.tick_limit) {
        return Err(invalid("encounter.tick_limit must be 1..100000"));
    }
    let local_id = identifier(&raw.local_id)?;
    let epoch = identifier(&raw.epoch)?;
    let mut roster = Vec::<EncounterMember>::new();
    for m in raw.roster {
        let id = identifier(&m.id)?;
        if m.address.port() == 0
            || m.address.ip().is_unspecified()
            || m.address.ip().is_multicast()
            || roster.iter().any(|r| r.id == id || r.address == m.address)
        {
            return Err(invalid(
                "encounter roster requires unique IDs and unique unicast nonzero-port addresses",
            ));
        }
        roster.push(EncounterMember {
            id,
            address: m.address,
        });
    }
    let local = roster
        .iter()
        .find(|m| m.id == local_id)
        .ok_or_else(|| invalid("encounter local_id must be in roster"))?;
    if local.address.port() != listen.port()
        || (!listen.ip().is_unspecified() && local.address.ip() != listen.ip())
    {
        return Err(invalid("encounter local roster address must match p2p.listen (or its port for wildcard listen)"));
    }
    roster.sort_by_key(|m| m.id);
    Ok(EncounterConfig {
        local_id,
        epoch,
        roster,
        tick_limit: raw.tick_limit,
    })
}

impl P2pConfig {
    /// The startup-derived Noise key. Reading or cloning config never reruns the KDF.
    pub fn psk_bytes(&self) -> &[u8; 32] {
        &self.psk.0
    }
}

/// Rates are represented in thousandths: equivalent TOML numbers produce identical hashes.
/// `drop_rate` scales drop chance (capped at 100%), not item quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ruleset {
    xp_milli: u32,
    drop_milli: u32,
}

impl Default for Ruleset {
    fn default() -> Self {
        Self {
            xp_milli: 1000,
            drop_milli: 1000,
        }
    }
}

impl Ruleset {
    pub fn xp_rate(self) -> f64 {
        f64::from(self.xp_milli) / 1000.0
    }
    pub fn drop_rate(self) -> f64 {
        f64::from(self.drop_milli) / 1000.0
    }
    pub fn xp_milli(self) -> u32 {
        self.xp_milli
    }
    pub fn drop_milli(self) -> u32 {
        self.drop_milli
    }

    /// Stable domain-separated SHA-256, independent of local paths, PSK, endpoint order and TOML
    /// spelling. A change in rule meanings or canonical encoding must bump RULESET_VERSION.
    pub fn fingerprint(self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"benilla-ruleset\0");
        hash.update(RULESET_VERSION.to_be_bytes());
        hash.update(self.xp_milli.to_be_bytes());
        hash.update(self.drop_milli.to_be_bytes());
        hash.finalize().into()
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    p2p: Option<RawP2p>,
    #[serde(default)]
    wow: WowConfig,
    #[serde(default)]
    ruleset: RawRuleset,
}

// No Debug/Serialize: the passphrase is only held while parsing and deriving the key.
// RAII also erases it when a later field or validation step fails.
struct Passphrase(Zeroizing<String>);
impl<'de> Deserialize<'de> for Passphrase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|value| Self(Zeroizing::new(value)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawP2p {
    encounter: Option<RawEncounter>,
    #[serde(default)]
    internet: Option<benilla_p2p::InternetConfig>,
    psk: Passphrase,
    #[serde(default = "default_network")]
    network: String,
    #[serde(default = "default_listen")]
    listen: SocketAddr,
    #[serde(default)]
    peers: Vec<SocketAddr>,
    #[serde(default = "default_max_peers")]
    max_peers: u16,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
}
fn default_network() -> String {
    "benilla".to_owned()
}
fn default_listen() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 7474))
}
fn default_max_peers() -> u16 {
    16
}
fn default_timeout() -> u64 {
    5000
}
fn default_rate() -> f64 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRuleset {
    #[serde(default = "default_rate")]
    xp_rate: f64,
    #[serde(default = "default_rate")]
    drop_rate: f64,
}
impl Default for RawRuleset {
    fn default() -> Self {
        Self {
            xp_rate: 1.0,
            drop_rate: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ConfigError {}
fn invalid(message: &str) -> ConfigError {
    ConfigError(message.to_owned())
}

impl AppConfig {
    /// Missing config preserves the upstream realm client. Any existing invalid config fails
    /// closed; the caller must not fall back to realm networking after an error.
    pub fn load() -> Result<Self, ConfigError> {
        let executable = std::env::current_exe()
            .map_err(|_| invalid("cannot locate the executable for config.toml"))?;
        Self::load_for_executable(&executable)
    }

    pub fn load_for_executable(executable: &Path) -> Result<Self, ConfigError> {
        let directory = executable
            .parent()
            .filter(|p| p.is_absolute())
            .ok_or_else(|| invalid("config discovery requires an absolute executable path"))?;
        let path = directory.join("config.toml");
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A broken config symlink is a configuration error, not legacy-mode selection.
                return match std::fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Self::default())
                    }
                    _ => Err(invalid("config.toml exists but cannot be opened")),
                };
            }
            Err(_) => return Err(invalid("cannot read config.toml beside the executable")),
        };
        let mut input = Zeroizing::new(String::new());
        if file
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut input)
            .is_err()
        {
            input.zeroize();
            return Err(invalid("config.toml must be readable UTF-8"));
        }
        let result = if input.len() as u64 > MAX_CONFIG_BYTES {
            Err(invalid("config.toml exceeds the 64 KiB limit"))
        } else {
            Self::parse(&input, directory)
        };
        input.zeroize();
        result
    }

    pub fn parse(input: &str, directory: &Path) -> Result<Self, ConfigError> {
        if !directory.is_absolute() {
            return Err(invalid("config directory must be absolute"));
        }
        let raw: RawConfig = toml::from_str(input).map_err(|error: toml::de::Error| {
            // Neither Display, Debug nor message() is safe: all can quote secret input.
            let location = error.span().map(|span| {
                let before = &input[..span.start.min(input.len())];
                let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
                let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                format!(" near line {line}, column {column}")
            }).unwrap_or_default();
            invalid(&format!("invalid config.toml schema or TOML syntax{location}; check documented fields and types (source withheld to protect the PSK)"))
        })?;
        let mut wow = raw.wow;
        if let Some(path) = &mut wow.data {
            if path.as_os_str().is_empty() {
                return Err(invalid(
                    "wow.data must not be empty; omit it to use normal discovery",
                ));
            }
            if path.is_relative() {
                *path = directory.join(&*path);
            }
        }
        let ruleset = Ruleset {
            xp_milli: rate(raw.ruleset.xp_rate, "ruleset.xp_rate")?,
            drop_milli: rate(raw.ruleset.drop_rate, "ruleset.drop_rate")?,
        };
        let p2p = raw.p2p.map(validate_p2p).transpose()?;
        Ok(Self { p2p, wow, ruleset })
    }
}

fn rate(value: f64, field: &str) -> Result<u32, ConfigError> {
    if !value.is_finite() || !(0.001..=1000.0).contains(&value) {
        return Err(invalid(&format!(
            "{field} must be finite and between 0.001 and 1000"
        )));
    }
    let scaled = value * 1000.0;
    if (scaled - scaled.round()).abs() > 1e-7 {
        return Err(invalid(&format!(
            "{field} supports at most three decimal places"
        )));
    }
    Ok(scaled.round() as u32)
}

fn validate_p2p(raw: RawP2p) -> Result<P2pConfig, ConfigError> {
    if raw.internet.is_some() && raw.encounter.is_some() {
        return Err(invalid("internet and synthetic encounter modes cannot be combined; encounter currently requires its fixed TCP roster"));
    }
    if raw.network.is_empty()
        || raw.network.len() > 64
        || !raw
            .network
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(invalid(
            "p2p.network must contain 1..64 ASCII letters, digits, hyphens, underscores or dots",
        ));
    }
    if raw.listen.port() == 0 || raw.listen.ip().is_multicast() {
        return Err(invalid(
            "p2p.listen requires a nonzero port and a non-multicast address",
        ));
    }
    if !(1..=64).contains(&raw.max_peers) || raw.peers.len() > usize::from(raw.max_peers) {
        return Err(invalid(
            "p2p.max_peers must be 1..64 and at least the number of peers",
        ));
    }
    if !(100..=60_000).contains(&raw.timeout_ms) {
        return Err(invalid("p2p.timeout_ms must be 100..60000"));
    }
    for (index, peer) in raw.peers.iter().enumerate() {
        if peer.port() == 0
            || peer.ip().is_unspecified()
            || peer.ip().is_multicast()
            || *peer == raw.listen
            || raw.peers[..index].contains(peer)
        {
            return Err(invalid("p2p.peers must contain unique unicast addresses with nonzero ports, excluding p2p.listen"));
        }
    }
    if let Some(internet) = &raw.internet {
        if !raw.peers.is_empty() || internet.peer_tickets.len() > usize::from(raw.max_peers) {
            return Err(invalid(
                "internet mode requires empty p2p.peers and at most max_peers endpoint tickets",
            ));
        }
        internet
            .validate()
            .map_err(|error| invalid(&error.to_string()))?;
    }
    // Only trusted local configuration selects this work, once, before networking starts.
    // Validate every cheap field before allocating the fixed 64 MiB KDF workspace.
    let psk = derive_psk(&raw.psk.0, &raw.network)?;
    Ok(P2pConfig {
        encounter: raw
            .encounter
            .map(|encounter| validate_encounter(encounter, raw.listen, &raw.peers, raw.max_peers))
            .transpose()?,
        internet: raw.internet,
        network: raw.network,
        listen: raw.listen,
        peers: raw.peers,
        max_peers: raw.max_peers,
        timeout_ms: raw.timeout_ms,
        psk,
    })
}

fn derive_psk(passphrase: &str, network: &str) -> Result<SecretPsk, ConfigError> {
    if passphrase.len() > MAX_PASSPHRASE_BYTES {
        return Err(invalid("p2p.psk must be at most 1024 UTF-8 bytes"));
    }
    // Deliberately accept every exact string, including empty, short, repeated and hexadecimal
    // strings. Strength advice belongs in the UI/docs; the only bound is resource protection.
    // RFC 9106's second recommended Argon2id profile. These are protocol constants, not
    // peer-controlled or config-tunable values; no negotiation or legacy raw-hex fallback.
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_LANES, Some(32))
        .map_err(|_| invalid("invalid built-in passphrase KDF parameters"))?;
    let mut memory = Zeroizing::new(Vec::<Block>::new());
    memory
        .try_reserve_exact(params.block_count())
        .map_err(|_| invalid("cannot allocate 64 MiB for passphrase key derivation"))?;
    memory.resize(params.block_count(), Block::default());
    let mut output = SecretPsk([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into_with_memory(
            passphrase.as_bytes(),
            &passphrase_salt(network),
            &mut output.0,
            &mut *memory,
        )
        .map_err(|_| invalid("passphrase key derivation failed"))?;
    Ok(output)
}

fn passphrase_salt(network: &str) -> [u8; 16] {
    // A reproducible group salt (not a secret): domain || u32BE version || network UTF-8.
    // No trimming, case folding, Unicode normalization or platform-dependent encoding.
    let mut salt = Sha256::new();
    salt.update(b"benilla-p2p-passphrase\0");
    salt.update(PASSPHRASE_KDF_VERSION.to_be_bytes());
    salt.update(network.as_bytes());
    let digest = salt.finalize();
    digest[..16].try_into().expect("SHA-256 contains 16 bytes")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod encounter_tests;
