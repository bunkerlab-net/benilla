// SPDX-License-Identifier: GPL-3.0-or-later
//! Initial-query projection only. MC transitions and sealed binding remain unaccepted.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize)]
pub struct Seal8CorpseReport {
    pub records: u64,
    pub query_requests: u64,
    pub query_results: u64,
    pub independently_matched_queries: u64,
    pub joined_snapshot_rows: usize,
    pub raw_snapshot_counts: [usize; 3],
    pub reference_execution_sealed: bool,
    pub whole_execution_accepted: bool,
}
pub fn compare_seal8_unsealed_corpses(
    root: &Path,
    initial: &CorpseInitialInputs<'_>,
) -> Result<Seal8CorpseReport> {
    initial.check()?;
    let expected = initial.rows().count();
    let mut hex = String::with_capacity(CORPSE_LOAD_QUERY.len() * 2);
    for byte in CORPSE_LOAD_QUERY.bytes() {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    let mut current = BTreeMap::<u64, (u64, u64)>::new();
    let mut pending = BTreeSet::new();
    let mut requests = 0;
    let mut results = 0;
    let mut matched = 0;
    let records = super::retained_seal8::scan_seal8(root, |context, kind, payload| {
        if kind == "sql_query_request" {
            let mut fields = payload.split_whitespace();
            let connection = super::retained_seal8::parse(fields.next())?;
            let request = super::retained_seal8::parse(fields.next())?;
            current.insert(context.thread, (connection, request));
        } else if kind == "sql_request_bytes"
            && payload.split_whitespace().nth(5) == Some(hex.as_str())
        {
            let key = *current
                .get(&context.thread)
                .ok_or_else(|| refuse("execution.corpse.seal8_query_request"))?;
            require(
                pending.insert(key),
                "execution.corpse.seal8_duplicate_request",
            )?;
            requests += 1;
        } else if kind == "sql_query_result" {
            let mut fields = payload.split_whitespace();
            let key = (
                super::retained_seal8::parse(fields.next())?,
                super::retained_seal8::parse(fields.next())?,
            );
            if pending.remove(&key) {
                fields.next();
                let rows: u64 = super::retained_seal8::parse(fields.next())?;
                fields.next();
                let status: u32 = super::retained_seal8::parse(fields.next())?;
                results += 1;
                require(
                    status == 0 && rows == expected as u64,
                    "execution.corpse.seal8_query_difference",
                )?;
                matched += 1;
            }
        }
        Ok(())
    })?;
    require(pending.is_empty(), "execution.corpse.seal8_missing_result")?;
    initial.check()?;
    Ok(Seal8CorpseReport {
        records,
        query_requests: requests,
        query_results: results,
        independently_matched_queries: matched,
        joined_snapshot_rows: expected,
        raw_snapshot_counts: initial.raw_counts,
        reference_execution_sealed: false,
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_initial_corpse_query_or_explicit_absence() {
        let Some(snapshot) = std::env::var_os("BENILLA_F1_INITIAL_CHARACTERS") else {
            println!("SKIP unsealed corpse query: set BENILLA_F1_INITIAL_CHARACTERS and BENILLA_F1_SEAL8_FRAMING");
            return;
        };
        let Some(framing) = std::env::var_os("BENILLA_F1_SEAL8_FRAMING") else {
            println!("SKIP unsealed corpse query: set BENILLA_F1_SEAL8_FRAMING");
            return;
        };
        let initial = CorpseInitialInputs::inspect_snapshot(
            Path::new(&snapshot),
            &Sha256Id::parse("cf538fd2b7bbd96fd7d653d8d2b05f19da49d49989e42ac412aee382bee5a95d")
                .unwrap(),
        )
        .unwrap();
        let report = compare_seal8_unsealed_corpses(Path::new(&framing), &initial).unwrap();
        assert_eq!(
            (
                report.query_requests,
                report.query_results,
                report.independently_matched_queries
            ),
            (1, 1, 1)
        );
        assert_eq!(report.joined_snapshot_rows, 0);
        assert!(!report.reference_execution_sealed && !report.whole_execution_accepted);
        println!(
            "SKIP MC constructor/registry/unload transition comparison: no corpse-state projection"
        );
        println!("{}", serde_json::to_string(&report).unwrap());
    }
}
