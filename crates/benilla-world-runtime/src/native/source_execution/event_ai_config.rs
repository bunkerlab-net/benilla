// SPDX-License-Identifier: GPL-3.0-or-later
//! Log.cpp:56,282-286 and CreatureEventAIMgr.cpp:712,919-920 consume configuration.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiLoaderConfig {
    pub sha256: Sha256Id,
    pub event_ai_dev_line: usize,
    pub assistance_radius_line: Option<usize>,
    dev_filter: bool,
    assistance_radius_bits: Option<u32>,
}
impl EventAiLoaderConfig {
    /// The caller supplies the independently approved recorded configuration hash.
    pub fn recorded(path: &Path, approved: &Sha256Id) -> Result<Self> {
        let file = ExecutionFile::open(path, 1_048_576)?;
        let mut bytes = vec![0; file.len() as usize];
        file.read_into(0, &mut bytes)?;
        file.check()?;
        require(
            Sha256Id::of(&bytes) == *approved,
            "execution.event_ai.config_identity",
        )?;
        Self::parse(&bytes, approved.clone())
    }
    fn parse(bytes: &[u8], sha256: Sha256Id) -> Result<Self> {
        let text =
            std::str::from_utf8(bytes).map_err(|_| refuse("execution.event_ai.config_encoding"))?;
        let mut filter = None;
        let mut radius = None;
        for (index, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "LogFilter_EventAiDev" => {
                    require(filter.is_none(), "execution.event_ai.config_duplicate")?;
                    // Config.cpp:117-124 accepts true/1/yes without case.
                    let value = value.trim();
                    let value = value == "1"
                        || value.eq_ignore_ascii_case("true")
                        || value.eq_ignore_ascii_case("yes");
                    filter = Some((index + 1, value));
                }
                "CreatureFamilyAssistanceRadius" => {
                    require(radius.is_none(), "execution.event_ai.config_duplicate")?;
                    // Config.cpp:134-138 uses stof, not double then float.
                    let value = value
                        .trim()
                        .parse::<f32>()
                        .map_err(|_| refuse("execution.event_ai.config_radius_value"))?;
                    radius = Some((index + 1, value.to_bits()));
                }
                _ => {}
            }
        }
        let (event_ai_dev_line, dev_filter) =
            filter.ok_or_else(|| refuse("execution.event_ai.config_filter_missing"))?;
        Ok(Self {
            sha256,
            event_ai_dev_line,
            dev_filter,
            assistance_radius_line: radius.map(|v| v.0),
            assistance_radius_bits: radius.map(|v| v.1),
        })
    }
    pub fn dev_log_filter(&self) -> bool {
        self.dev_filter
    }
    pub fn assistance_radius(&self) -> Result<u32> {
        let bits = self
            .assistance_radius_bits
            .ok_or_else(|| refuse("execution.event_ai.config_assistance_radius_missing"))?;
        let value = f32::from_bits(bits);
        require(
            value.is_finite() && (0.0..4_294_967_296.0).contains(&value),
            "execution.event_ai.loader.radius_conversion_undefined",
        )?;
        Ok(value as u32)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_has_no_invented_filter_or_radius_default() {
        let parse = |bytes: &[u8]| EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes));
        assert_eq!(
            parse(b"# LogFilter_EventAiDev = 1\n").unwrap_err().id,
            "execution.event_ai.config_filter_missing"
        );
        for (raw, expected) in [
            (b"LogFilter_EventAiDev = 0\n".as_slice(), false),
            (b"LogFilter_EventAiDev = 1\n", true),
        ] {
            let config = parse(raw).unwrap();
            assert_eq!(config.dev_log_filter(), expected);
            assert_eq!(config.event_ai_dev_line, 1);
            assert_eq!(
                config.assistance_radius().unwrap_err().id,
                "execution.event_ai.config_assistance_radius_missing"
            );
        }
        let config =
            parse(b"LogFilter_EventAiDev = 1\nCreatureFamilyAssistanceRadius = 5.75\n").unwrap();
        assert_eq!(config.assistance_radius().unwrap(), 5);
        assert_eq!(config.assistance_radius_line, Some(2));
    }
}
