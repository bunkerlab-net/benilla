// SPDX-License-Identifier: GPL-3.0-or-later
//! Unsealed first-packet projection only, never a weather transition certificate.
use super::*;
#[derive(Debug, Serialize)]
pub struct Seal8WeatherReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub weather_packets: u64,
    pub matched_initial_packet: u64,
    pub first_difference_event: Option<u64>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
pub fn compare_seal8_unsealed_weather(root: &Path) -> Result<Seal8WeatherReport> {
    let mut packets = 0;
    let mut matched = 0;
    let mut difference = None;
    let expected = benilla_sim::source_weather::Weather::default().packet();
    let records = super::retained_seal8::scan_seal8(root, |context, kind, payload| {
        if kind != "session_packet_output" {
            return Ok(());
        }
        let mut fields = payload.split_whitespace();
        fields.next();
        if fields.next() != Some("756") {
            return Ok(());
        }
        packets += 1;
        fields.next();
        let length: usize = super::retained_seal8::parse(fields.next())?;
        fields.next();
        let hex = fields
            .next()
            .ok_or_else(|| refuse("execution.weather.packet_fields"))?;
        require(fields.next().is_none(), "execution.weather.packet_fields")?;
        require(
            length == 13 && hex.len() == 26 && hex.is_ascii(),
            "execution.weather.packet_width",
        )?;
        let mut bytes = [0; 13];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| refuse("execution.weather.packet_hex"))?;
        }
        // Only first lazy fine/zero packet is represented by this projection; Weather.cpp:49-57,207-215.
        if packets == 1 {
            if bytes == expected {
                matched = 1;
            } else {
                difference = Some(context.event);
            }
        }
        Ok(())
    })?;
    Ok(Seal8WeatherReport {
        adapter: "benilla-seal8-08-unsealed-weather-first-packet-v1",
        trace_sha256: Sha256Id::parse(super::retained_seal8::SEAL8_TRACE_HASH)?,
        records,
        weather_packets: packets,
        matched_initial_packet: matched,
        first_difference_event: difference,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_weather_initial_packet_or_explicit_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path=explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(||std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08"));
        if explicit.is_none() && !path.try_exists().unwrap() {
            println!("SKIP unsealed weather projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_weather(&path).unwrap();
        assert_eq!(report.first_difference_event, None);
        if report.weather_packets == 0 {
            println!("SKIP unsealed P1 weather packet: projection absent");
        } else {
            assert_eq!(report.matched_initial_packet, 1);
        }
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("SKIP unsealed MW regeneration/container/calendar comparison: no bound weather transition projection");
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
