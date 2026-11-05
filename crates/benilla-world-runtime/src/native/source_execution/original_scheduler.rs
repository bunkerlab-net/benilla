// SPDX-License-Identifier: GPL-3.0-or-later
//! Original TSV job projection, deliberately separate from F0 envelopes.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize)]
pub struct OriginalSchedulerReport {
    pub adapter: &'static str,
    pub projection: Projection,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub matched_records: u64,
    pub observed: BTreeMap<String, u64>,
    pub outstanding_jobs: Vec<SchedulerOperation>,
    pub first_difference: Option<SchedulerDivergence>,
    pub first_unproved_record: Option<u64>,
    pub pending: Vec<ClockSchedulerPending>,
    pub f1_pending: Vec<PendingF1Obligation>,
    pub whole_execution_accepted: bool,
}
#[derive(Default)]
struct Jobs {
    queue: super::scheduler::FifoJobs<u64>,
    submitted: BTreeMap<u64, u64>,
    running: BTreeMap<u64, SchedulerOperation>,
    observed_running: BTreeMap<u64, SchedulerOperation>,
    difference: Option<SchedulerDivergence>,
    workers: BTreeSet<u64>,
    sequences: BTreeMap<u64, u64>,
    next_job: u64,
    matched: u64,
    observed: BTreeMap<String, u64>,
}
fn number<T: std::str::FromStr>(text: Option<&str>) -> Result<T> {
    text.ok_or_else(|| refuse("execution.scheduler_fields"))?
        .parse()
        .map_err(|_| refuse("execution.scheduler_number"))
}
impl Jobs {
    fn line(&mut self, raw: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(raw).map_err(|_| refuse("execution.scheduler_utf8"))?;
        let mut fields = text.splitn(4, '\t');
        let thread: u64 = number(fields.next())?;
        let occurrence: u64 = number(fields.next())?;
        require(
            thread > 0
                && self
                    .sequences
                    .get(&thread)
                    .is_none_or(|prior| occurrence == prior + 1),
            "execution.scheduler_occurrence",
        )?;
        self.sequences.insert(thread, occurrence);
        let name = fields
            .next()
            .ok_or_else(|| refuse("execution.scheduler_fields"))?;
        let payload = fields
            .next()
            .ok_or_else(|| refuse("execution.scheduler_fields"))?;
        if matches!(
            name,
            "map_begin" | "map_end" | "world_tick" | "worker_birth"
        ) {
            *self.observed.entry(name.into()).or_default() += 1;
            if name == "worker_birth" {
                require(
                    self.workers.insert(thread),
                    "execution.scheduler_worker_birth",
                )?;
            }
        }
        if !matches!(
            name,
            "worker_submit" | "worker_dispatch" | "worker_complete"
        ) {
            return Ok(());
        }
        *self.observed.entry(name.into()).or_default() += 1;
        let address = u64::from_str_radix(
            payload
                .strip_prefix("0x")
                .ok_or_else(|| refuse("execution.scheduler_address"))?,
            16,
        )
        .map_err(|_| refuse("execution.scheduler_address"))?;
        let (kind, reference_job) = match name {
            "worker_submit" => {
                require(
                    !self.submitted.contains_key(&address),
                    "execution.scheduler_job_lifetime",
                )?;
                self.next_job += 1;
                self.submitted.insert(address, self.next_job);
                (SchedulerOperationKind::Submit, self.next_job)
            }
            "worker_dispatch" => {
                require(
                    self.workers.contains(&thread) && !self.observed_running.contains_key(&thread),
                    "execution.scheduler_worker_owner",
                )?;
                let job = *self
                    .submitted
                    .get(&address)
                    .ok_or_else(|| refuse("execution.scheduler_dispatch_without_submit"))?;
                self.observed_running.insert(
                    thread,
                    SchedulerOperation {
                        kind: SchedulerOperationKind::Dispatch,
                        thread,
                        observed_occurrence: occurrence,
                        job,
                    },
                );
                (SchedulerOperationKind::Dispatch, job)
            }
            "worker_complete" => {
                let job = self
                    .submitted
                    .remove(&address)
                    .ok_or_else(|| refuse("execution.scheduler_completion_without_submit"))?;
                let observed = self
                    .observed_running
                    .remove(&thread)
                    .ok_or_else(|| refuse("execution.scheduler_completion_without_dispatch"))?;
                require(
                    observed.job == job,
                    "execution.scheduler_observed_completion_owner",
                )?;
                (SchedulerOperationKind::Complete, job)
            }
            _ => unreachable!(),
        };
        // Continue counting raw observations, never accepted native work, after divergence; MapUpdater.cpp:88-98.
        if self.difference.is_some() {
            return Ok(());
        }
        let native_job = match kind {
            SchedulerOperationKind::Submit => {
                self.queue.push(reference_job, reference_job);
                reference_job
            }
            SchedulerOperationKind::Dispatch => {
                let job = self.queue.pop_next()?;
                self.running.insert(
                    thread,
                    SchedulerOperation {
                        kind,
                        thread,
                        observed_occurrence: occurrence,
                        job,
                    },
                );
                job
            }
            SchedulerOperationKind::Complete => {
                self.running
                    .remove(&thread)
                    .ok_or_else(|| refuse("execution.scheduler_completion_without_dispatch"))?
                    .job
            }
        };
        let expected = SchedulerOperation {
            kind,
            thread,
            observed_occurrence: occurrence,
            job: reference_job,
        };
        let actual = SchedulerOperation {
            job: native_job,
            ..expected
        };
        if let Err(mismatch) =
            DifferentialOracle::compare_scheduler_operation(expected, actual, self.matched)
        {
            let reason = mismatch.reason.clone();
            self.difference = Some(mismatch);
            return Err(reason);
        }
        self.matched += 1;
        Ok(())
    }
}
// Recompute queue membership from publication and recipient choices; MapUpdater.cpp:74-98.
pub fn compare_original_unsealed_scheduler(path: &Path) -> Result<OriginalSchedulerReport> {
    let file = ExecutionFile::open(path, 1_073_741_824)?;
    let mut buffer = vec![0; 1_048_576];
    let mut line = Vec::with_capacity(1024);
    let mut hash = Sha256::new();
    let mut at = 0;
    let mut records = 0;
    let mut jobs = Jobs::default();
    let mut first_unproved_record = None;
    while at < file.len() {
        let size = (file.len() - at).min(buffer.len() as u64) as usize;
        file.read_into(at, &mut buffer[..size])?;
        hash.update(&buffer[..size]);
        let mut start = 0;
        while start < size {
            let end = buffer[start..size]
                .iter()
                .position(|b| *b == b'\n')
                .map(|p| start + p);
            let stop = end.unwrap_or(size);
            require(
                line.len() + stop - start <= 65_536,
                "execution.scheduler_line_limit",
            )?;
            line.extend_from_slice(&buffer[start..stop]);
            start = stop + usize::from(end.is_some());
            if end.is_none() {
                break;
            }
            records += 1;
            if let Err(mut reason) = jobs.line(&line) {
                if reason.id == "execution.scheduler_diagnostic_mismatch"
                    && first_unproved_record.is_none()
                {
                    first_unproved_record = Some(records);
                } else {
                    reason.id.push_str(&format!(".record_{records}"));
                    return Err(reason);
                }
            }
            line.clear();
        }
        at += size as u64;
    }
    file.check()?;
    require(line.is_empty(), "execution.scheduler_truncated")?;
    let trace_sha256 = Sha256Id(format!("{:x}", hash.finalize()));
    require(
        trace_sha256.as_str() == "574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b"
            && records == 15_371_848,
        "execution.scheduler_prefix_identity",
    )?;
    require(
        jobs.matched == 62
            && jobs.workers.len() == 3
            && jobs.observed_running.len() == 1
            && jobs.submitted.len() == 1
            && first_unproved_record == Some(15_310_184),
        "execution.scheduler_prefix_counts",
    )?;
    Ok(OriginalSchedulerReport {
        adapter: "benilla-original-unsealed-tsv-scheduler-v1",
        projection: Projection::OriginalUnsealedSchedulerV1,
        trace_sha256,
        records,
        matched_records: jobs.matched,
        observed: jobs.observed,
        outstanding_jobs: jobs.observed_running.into_values().collect(),
        first_difference: jobs.difference,
        first_unproved_record,
        pending: CLOCK_SCHEDULER_PENDING.into(),
        f1_pending: vec![
            PendingF1Obligation::SealedReferenceExecution,
            PendingF1Obligation::CompleteCausalCoverage,
            PendingF1Obligation::UnsupportedSemanticOwners,
        ],
        whole_execution_accepted: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_recomputes_fifo_and_refuses_wrong_recipient_or_completion() {
        let mut jobs = Jobs::default();
        jobs.line(b"2\t0\tworker_birth\t0xab").unwrap();
        jobs.line(b"1\t0\tworker_submit\t0x1").unwrap();
        jobs.line(b"1\t1\tworker_submit\t0x2").unwrap();
        assert_eq!(
            jobs.line(b"2\t1\tworker_dispatch\t0x2").unwrap_err().id,
            "execution.scheduler_diagnostic_mismatch"
        );
        let mut jobs = Jobs::default();
        assert_eq!(
            jobs.line(b"2\t0\tworker_complete\t0x1").unwrap_err().id,
            "execution.scheduler_completion_without_submit"
        );
    }
    #[test]
    fn diagnostic_comparator_reports_the_exact_first_difference() {
        let expected = SchedulerOperation {
            kind: SchedulerOperationKind::Dispatch,
            thread: 2,
            observed_occurrence: 7,
            job: 2,
        };
        let actual = SchedulerOperation { job: 1, ..expected };
        let error =
            DifferentialOracle::compare_scheduler_operation(expected, actual, 6).unwrap_err();
        assert_eq!(error.expected, expected);
        assert_eq!(error.actual, actual);
        assert_eq!(error.matched_records, 6);
    }
}
