// SPDX-License-Identifier: GPL-3.0-or-later
//! Retained calendar operands are labelled numeric fixtures, not loader inputs.
use super::retained_seal8::{parse, scan_seal8, SEAL8_TRACE_HASH};
use super::*;
use benilla_sim::source_calendar::{self, Tm};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize)]
pub struct Seal8EventCalendarDifference {
    pub event: u64,
    pub kind: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Seal8EventCalendarReport {
    pub adapter: &'static str,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub localtime_records: u64,
    pub mktime_input_records: u64,
    pub mktime_result_records: u64,
    pub independently_matched_localtime: u64,
    pub independently_matched_mktime: u64,
    pub p0_calendar_records_observed: u64,
    pub independently_matched_event_definitions: u64,
    pub independently_matched_me_transitions: u64,
    pub first_difference: Option<Seal8EventCalendarDifference>,
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
    pub pending: &'static [&'static str],
}
fn tuple(fields: &[&str]) -> Result<Tm> {
    require(fields.len() == 11, "execution.event.seal8_calendar_fields")?;
    Ok(Tm {
        sec: parse(fields.get(1).copied())?,
        min: parse(fields.get(2).copied())?,
        hour: parse(fields.get(3).copied())?,
        mday: parse(fields.get(4).copied())?,
        mon: parse(fields.get(5).copied())?,
        year: parse(fields.get(6).copied())?,
        wday: parse(fields.get(7).copied())?,
        yday: parse(fields.get(8).copied())?,
        isdst: parse(fields.get(9).copied())?,
    })
}
pub fn compare_seal8_unsealed_event_calendar(root: &Path) -> Result<Seal8EventCalendarReport> {
    let mut report = Seal8EventCalendarReport {
        adapter: "benilla-seal8-08-unsealed-calendar-fixtures-v1",
        trace_sha256: Sha256Id::parse(SEAL8_TRACE_HASH)?,
        records: 0,
        localtime_records: 0,
        mktime_input_records: 0,
        mktime_result_records: 0,
        independently_matched_localtime: 0,
        independently_matched_mktime: 0,
        p0_calendar_records_observed: 0,
        independently_matched_event_definitions: 0,
        independently_matched_me_transitions: 0,
        first_difference: None,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
        pending: &[
            "SealedExecutionAndEnvironmentBinding",
            "ActualDefinitionAndSqlOccurrences",
            "LunarHistoricalLibmDispatch",
            "METransitionsAndEffects",
            "F7Continuation",
        ],
    };
    let mut pending = BTreeMap::<u64, Tm>::new();
    report.records = scan_seal8(root, |context, kind, payload| {
        if !matches!(
            kind,
            "clock_localtime" | "clock_localtime_r" | "clock_mktime_input" | "clock_mktime_result"
        ) {
            return Ok(());
        }
        if context.world == 0 {
            report.p0_calendar_records_observed += 1;
        }
        match kind {
            "clock_mktime_input" => report.mktime_input_records += 1,
            "clock_mktime_result" => report.mktime_result_records += 1,
            _ => report.localtime_records += 1,
        }
        if report.first_difference.is_some() {
            return Ok(());
        }
        let fields: Vec<_> = payload.split_whitespace().collect();
        let observed = tuple(&fields)?;
        let epoch: i64 = parse(fields.first().copied())?;
        let outcome = if kind == "clock_mktime_input" {
            // F0:646-654 approves only seven consumed pre-mktime fields.
            // Incoming wday/yday and gmtoff are not inputs, even in old captures.
            require(
                pending.insert(context.thread, observed).is_none(),
                "execution.event.seal8_calendar_input_overlap",
            )?;
            return Ok(());
        } else if kind == "clock_mktime_result" {
            let mut input = pending
                .remove(&context.thread)
                .ok_or_else(|| refuse("execution.event.seal8_calendar_result_without_input"))?;
            source_calendar::mktime(&mut input).map(|actual| (actual, input))
        } else {
            source_calendar::localtime(epoch).map(|value| (epoch, value))
        };
        if !outcome.is_ok_and(|(actual, value)| actual == epoch && value == observed)
            || fields[10] != "0"
        {
            report.first_difference = Some(Seal8EventCalendarDifference {
                event: context.event,
                kind: kind.into(),
                reason: "UTC calendar conversion differs; no resynchronization".into(),
            });
        } else if kind == "clock_mktime_result" {
            report.independently_matched_mktime += 1;
        } else {
            report.independently_matched_localtime += 1;
        }
        Ok(())
    })?;
    require(
        pending.is_empty(),
        "execution.event.seal8_calendar_unfinished_conversion",
    )?;
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_event_calendar_fixture_or_explicit_absence() {
        let explicit = std::env::var_os("BENILLA_F1_SEAL8_FRAMING");
        let path=explicit.as_ref().map(std::path::PathBuf::from).unwrap_or_else(||std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benilla-config/full-server/f1/sealing/source-choice/seal8-full-framing-08"));
        if explicit.is_none() && !path.try_exists().unwrap() {
            println!("SKIP unsealed P0 calendar projection: set BENILLA_F1_SEAL8_FRAMING");
            return;
        }
        let report = compare_seal8_unsealed_event_calendar(&path).unwrap();
        assert!(report.localtime_records > 0 && report.mktime_input_records > 0);
        assert!(report.first_difference.is_none(), "{report:?}");
        assert_eq!(report.independently_matched_event_definitions, 0);
        assert_eq!(report.independently_matched_me_transitions, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!("SKIP ME/complete P0 event comparison: no bound definition, manager or transition occurrences");
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
