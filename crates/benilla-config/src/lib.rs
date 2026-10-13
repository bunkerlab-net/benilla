//! Fork-specific startup configuration, separate from the stock client's persisted CVars.
//! Never display TOML parser errors directly: they can contain the source and hence the PSK.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::File,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
pub const RULESET_VERSION: u32 = 1;

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
    pub network: String,
    pub listen: SocketAddr,
    pub peers: Vec<SocketAddr>,
    pub max_peers: u16,
    pub timeout_ms: u64,
    psk: SecretPsk,
}

impl P2pConfig {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawP2p {
    psk: String,
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
        let mut input = String::new();
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

fn validate_p2p(mut raw: RawP2p) -> Result<P2pConfig, ConfigError> {
    let parsed_psk = parse_psk(&raw.psk);
    raw.psk.zeroize();
    let psk = parsed_psk?;
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
    Ok(P2pConfig {
        network: raw.network,
        listen: raw.listen,
        peers: raw.peers,
        max_peers: raw.max_peers,
        timeout_ms: raw.timeout_ms,
        psk,
    })
}

fn parse_psk(input: &str) -> Result<SecretPsk, ConfigError> {
    if input.len() != 64 || !input.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("p2p.psk must be exactly 64 hexadecimal characters (32 random bytes); replace the sample placeholder"));
    }
    let mut bytes = [0; 32];
    for (byte, hex) in bytes.iter_mut().zip(input.as_bytes().as_chunks::<2>().0) {
        let digit = |b: u8| {
            if b.is_ascii_digit() {
                b - b'0'
            } else {
                b.to_ascii_lowercase() - b'a' + 10
            }
        };
        *byte = (digit(hex[0]) << 4) | digit(hex[1]);
    }
    if bytes.iter().all(|byte| *byte == bytes[0]) {
        return Err(invalid(
            "p2p.psk must not be a constant-byte placeholder key",
        ));
    }
    Ok(SecretPsk(bytes))
}

#[cfg(test)]
mod tests;
