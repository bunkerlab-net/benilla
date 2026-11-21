// SPDX-License-Identifier: GPL-3.0-or-later
//! World.cpp:743-744 loads the two Unit::OnRelocated settings.
use super::*;

impl SourceUnitRelocationConfig {
    pub fn from_recorded_bytes(
        bytes: &[u8],
        approved: &Sha256Id,
        map_distance_bits: u32,
    ) -> Result<Self> {
        require(
            Sha256Id::of(bytes) == *approved,
            "execution.relocation.World_743.config_identity",
        )?;
        let value = |key| match spawn_loader_config::assignment(bytes, key) {
            Ok((_, value)) => Ok(Some(value)),
            Err(error) if error.id == format!("execution.spawn_loader.config_missing.{key}") => {
                Ok(None)
            }
            Err(error) => Err(error),
        };
        let delay = value("Visibility.AIRelocationNotifyDelay")?
            .as_deref()
            .unwrap_or("1000")
            .parse::<i32>()
            .map_err(|_| refuse("execution.relocation.World_743.config_delay_value"))?
            as u32;
        let limit = value("Visibility.RelocationLowerLimit")?
            .as_deref()
            .unwrap_or("10")
            .parse::<f32>()
            .map_err(|_| refuse("execution.relocation.World_744.config_limit_value"))?;
        // pow(float, 2) promotes the operands to double before assignment to float.
        let limit = f64::from(limit);
        let lower_limit_squared = (limit * limit) as f32;
        require(
            lower_limit_squared.is_finite(),
            "execution.relocation.World_744.config_squared_limit_nonfinite",
        )?;
        Ok(Self {
            lower_limit_squared,
            ai_notify_delay_ms: delay,
            map_distance_bits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_config_uses_source_defaults_and_refuses_nonfinite_squared_limit() {
        let parse = |bytes: &[u8]| {
            SourceUnitRelocationConfig::from_recorded_bytes(bytes, &Sha256Id::of(bytes), 0)
        };
        let defaults = parse(b"").unwrap();
        assert_eq!(
            (defaults.ai_notify_delay_ms, defaults.lower_limit_squared),
            (1000, 100.)
        );
        let parsed = parse(
            b"Visibility.AIRelocationNotifyDelay = -1\nVisibility.RelocationLowerLimit = -2.5",
        )
        .unwrap();
        assert_eq!(
            (parsed.ai_notify_delay_ms, parsed.lower_limit_squared),
            (u32::MAX, 6.25)
        );
        for (bytes, id) in [
            (
                &b"Visibility.RelocationLowerLimit = 3e20"[..],
                "execution.relocation.World_744.config_squared_limit_nonfinite",
            ),
            (
                &b"Visibility.RelocationLowerLimit = invalid"[..],
                "execution.relocation.World_744.config_limit_value",
            ),
            (
                &b"Visibility.AIRelocationNotifyDelay = invalid"[..],
                "execution.relocation.World_743.config_delay_value",
            ),
        ] {
            assert_eq!(parse(bytes).err().unwrap().id, id);
        }
    }
}
