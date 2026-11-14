// SPDX-License-Identifier: GPL-3.0-or-later
//! Recorded World.cpp:455,495 configuration, never implicit defaults.
use super::*;

#[derive(Clone, Debug)]
pub struct AutoloadActiveConfig {
    pub sha256: Sha256Id,
    pub line: usize,
    enabled: bool,
}
#[derive(Clone, Debug)]
pub struct InstanceResetConfig {
    pub sha256: Sha256Id,
    pub line: usize,
    rate: f32,
}
pub(super) fn assignment(bytes: &[u8], key: &str) -> Result<(usize, String)> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| refuse("execution.spawn_loader.config_encoding"))?;
    let mut found = None;
    // Config.cpp:60-80 lowercases keys, strips edge quotes, and keeps the last assignment.
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with(['#', '[']) {
            continue;
        }
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| refuse("execution.spawn_loader.config_syntax"))?;
        if name.trim().eq_ignore_ascii_case(key) {
            found = Some((index + 1, value.trim().trim_matches('"').to_owned()));
        }
    }
    found.ok_or_else(|| refuse(&format!("execution.spawn_loader.config_missing.{key}")))
}
fn recorded_bytes(path: &Path, approved: &Sha256Id) -> Result<Vec<u8>> {
    let file = ExecutionFile::open(path, 1_048_576)?;
    let mut bytes = vec![0; file.len() as usize];
    file.read_into(0, &mut bytes)?;
    file.check()?;
    require(
        Sha256Id::of(&bytes) == *approved,
        "execution.spawn_loader.config_identity",
    )?;
    Ok(bytes)
}
impl AutoloadActiveConfig {
    pub fn recorded(path: &Path, approved: &Sha256Id) -> Result<Self> {
        Self::parse(&recorded_bytes(path, approved)?)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let (line, value) = assignment(bytes, "Autoload.Active")?;
        // Config.cpp:117-124: malformed booleans are false, not parse errors.
        let enabled =
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes");
        Ok(Self {
            sha256: Sha256Id::of(bytes),
            line,
            enabled,
        })
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl InstanceResetConfig {
    pub fn recorded(path: &Path, approved: &Sha256Id) -> Result<Self> {
        Self::parse(&recorded_bytes(path, approved)?)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let (line, value) = assignment(bytes, "Rate.InstanceResetTime")?;
        let rate = value
            .parse::<f32>()
            .map_err(|_| refuse("execution.spawn_loader.reset_rate_value"))?;
        // World.cpp:455,2420-2427 corrects an explicitly negative rate to 1.
        let rate = if rate < 0.0 { 1.0 } else { rate };
        Ok(Self {
            sha256: Sha256Id::of(bytes),
            line,
            rate,
        })
    }
    pub(super) fn scale(&self, days: u32) -> Result<u32> {
        let scaled = days as f32 * self.rate;
        require(
            scaled.is_finite() && scaled > -1.0 && f64::from(scaled) < 4_294_967_296.0,
            "execution.instance_templates.reset_delay_conversion_undefined",
        )?;
        Ok((scaled as u32).max(1))
    }
}

/// Borrows the existing F3 RNG/terrain owner without resetting or replacing it.
pub struct ConfiguredEffectiveSpawnOwner<'a, T> {
    pub execution: &'a mut T,
    pub autoload: &'a AutoloadActiveConfig,
}
impl<T: EffectiveSpawnOwner> EffectiveSpawnOwner for ConfiguredEffectiveSpawnOwner<'_, T> {
    fn startup_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        self.execution.startup_random()
    }
    fn original_zone(&mut self, map: u32, pose: [f32; 3]) -> Result<u32> {
        self.execution.original_zone(map, pose)
    }
    fn autoload_active(&mut self) -> Result<bool> {
        Ok(self.autoload.enabled())
    }
}
