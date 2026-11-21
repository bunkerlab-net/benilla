// SPDX-License-Identifier: GPL-3.0-or-later
#[test]
fn retained_original_f1_job_projection_or_clean_absence() {
    let explicit = std::env::var_os("BENILLA_F1_CLOCK_TRACE");
    let path = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("benilla-config/full-server/f1/run/startup-through-third-map0.tsv")
    });
    if explicit.is_none() && !path.try_exists().unwrap() {
        eprintln!("SKIP private F1 clock/job projection: set BENILLA_F1_CLOCK_TRACE to retained original prefix");
        return;
    }
    let report = compare_original_unsealed_scheduler(&path).unwrap();
    assert_eq!(report.matched_records, 62);
    assert_eq!(report.observed["worker_submit"], 52);
    assert_eq!(report.observed["worker_dispatch"], 52);
    assert_eq!(report.observed["worker_complete"], 51);
    assert_eq!(report.observed["map_begin"], 52);
    assert_eq!(report.observed["map_end"], 52);
    assert!(!report.observed.contains_key("world_tick"));
    assert_eq!(report.first_unproved_record, Some(15_310_184));
    let difference = report.first_difference.as_ref().unwrap();
    assert_eq!(
        difference.reason.id,
        "execution.scheduler_diagnostic_mismatch"
    );
    assert_eq!(difference.expected.job, 22);
    assert_eq!(difference.actual.job, 21);
    assert_eq!(difference.expected.thread, 13);
    assert_eq!(difference.expected.observed_occurrence, 7223);
    assert_eq!(report.outstanding_jobs.len(), 1);
    assert!(!report.whole_execution_accepted);
    assert!(report
        .pending
        .contains(&ClockSchedulerPending::QueuePopLinearization));
    println!("{}", serde_json::to_string(&report).unwrap());
}
