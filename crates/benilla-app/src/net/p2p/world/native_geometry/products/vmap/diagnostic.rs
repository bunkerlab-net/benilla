// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only, bounded and redacted collision inspection. No gameplay admission.
use super::super::{
    source_height::{CompositionError, HeightRequest as StaticHeightRequest, PreparedSourceHeight},
    TileId,
};
use super::{prepared::PrepareError, tile_prepared::PreparedTileCollision, HeightRequest};
use benilla_native_terrain as terrain_source;
use benilla_native_vmap_source::{self as source, DoodadProjectionProfile};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

const FORMAT: &str = "benilla-collision-diagnostics-v2";
const CODES: &str = include_str!("diagnostic_codes.json");
const MAX_LINE: usize = 16 * 1024;
const MAX_OUTPUT: usize = 256 * 1024;
#[derive(Clone)]
struct Selection {
    data: PathBuf,
    map: u32,
    x: u8,
    y: u8,
    seconds: u64,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            data: PathBuf::new(),
            map: 0,
            x: 0,
            y: 0,
            seconds: 120,
        }
    }
}
fn selection(args: &[OsString]) -> Option<Selection> {
    if args.len() != 8 {
        return None;
    }
    let mut result = Selection::default();
    let mut seen = std::collections::BTreeSet::new();
    for pair in args.as_chunks::<2>().0 {
        let flag = pair[0].to_str()?;
        if !seen.insert(flag) {
            return None;
        }
        match flag {
            "--data" => {
                if pair[1].is_empty() {
                    return None;
                }
                result.data = PathBuf::from(&pair[1]);
            }
            "--map" => {
                result.map = pair[1].to_str()?.parse().ok()?;
                if result.map > 999 {
                    return None;
                }
            }
            "--tile" => {
                let (x, y) = pair[1].to_str()?.split_once(',')?;
                result.x = x.parse().ok()?;
                result.y = y.parse().ok()?;
                if result.x >= 64 || result.y >= 64 {
                    return None;
                }
            }
            "--timeout-seconds" => {
                result.seconds = pair[1].to_str()?.parse().ok()?;
                if !(1..=600).contains(&result.seconds) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    (!result.data.as_os_str().is_empty()).then_some(result)
}
struct Report<'a, W> {
    output: &'a mut W,
    selection: Selection,
    start: Instant,
    bytes: usize,
    lines: usize,
    failed: usize,
}
impl<W: Write> Report<'_, W> {
    fn record(
        &mut self,
        stage: &str,
        status: &str,
        code: Option<&str>,
        metrics: Value,
        hashes: Value,
        probe: Value,
        source_stage: Value,
    ) -> io::Result<()> {
        if status == "failed" {
            self.failed += 1;
        }
        let code = code.map(safe_code);
        let record = json!({"format": FORMAT, "stage": stage, "status": status,
            "map": self.selection.map, "tile_x": self.selection.x, "tile_y": self.selection.y,
            "elapsed_ms": self.start.elapsed().as_millis().min(i64::MAX as u128) as u64,
            "code":code, "source_stage":source_stage, "metrics":metrics, "hashes":hashes,
            "probe":probe, "gameplay_admitted":false});
        let line = serde_json::to_vec(&record)?;
        if self.lines >= 32 || line.len() > MAX_LINE || self.bytes + line.len() + 1 > MAX_OUTPUT {
            return Err(io::Error::other("diagnostic output limit"));
        }
        self.output.write_all(&line)?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        self.lines += 1;
        self.bytes += line.len() + 1;
        Ok(())
    }
    fn simple(&mut self, stage: &str, status: &str, code: Option<&str>) -> io::Result<()> {
        self.record(
            stage,
            status,
            code,
            json!({}),
            json!({}),
            Value::Null,
            Value::Null,
        )
    }
}
fn safe_code(code: &str) -> &str {
    // A closed list, rather than formatting arbitrary errors into the report.
    if CODES
        .lines()
        .any(|line| line.trim().trim_end_matches(',').trim_matches('"') == code)
    {
        code
    } else {
        "collision.unknown_error"
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Called only after exact subcommand selection, before any app/profile setup.
/// The soft deadline is cooperative; the collector independently kills a stalled
/// child. A successful exit means a report was emitted, not that stages passed.
pub(crate) fn run(args: &[OsString], output: &mut impl Write) -> io::Result<()> {
    run_with_budget(args, output, None)
}

fn run_with_budget(
    args: &[OsString],
    output: &mut impl Write,
    budget: Option<Duration>,
) -> io::Result<()> {
    let selected = selection(args);
    let mut report = Report {
        output,
        selection: selected.clone().unwrap_or_default(),
        start: Instant::now(),
        bytes: 0,
        lines: 0,
        failed: 0,
    };
    if selected.is_none() {
        report.simple("selection", "failed", Some("collision.arguments"))?;
        return report.record(
            "complete",
            "failed",
            Some("collision.arguments"),
            json!({"failed_stages":1}),
            json!({}),
            Value::Null,
            Value::Null,
        );
    }
    let selected = selected.expect("checked selection");
    let configured = Duration::from_secs(selected.seconds);
    let deadline = report.start + budget.unwrap_or(configured).min(configured);
    let cancelled = || Instant::now() >= deadline;
    report.simple("selection", "passed", None)?;
    // Terrain preparation is bounded but synchronous. The external collector's
    // hard process timeout is authoritative if a filesystem operation stalls.
    let terrain = if cancelled() {
        report.simple("terrain_capture", "failed", Some("collision.timeout"))?;
        None
    } else {
        match terrain_source::prepare(
            &selected.data,
            selected.map,
            &[terrain_source::Tile {
                x: selected.x,
                y: selected.y,
            }],
        ) {
            Ok(terrain) => {
                report.record("terrain_capture", "passed", None,
                    json!({"terrain_tiles":terrain.tiles().count(),"terrain_bytes":terrain.tiles().map(|(_,b)|b.len()).sum::<usize>()}),
                    json!({"terrain_source_epoch":terrain.source_epoch()}), Value::Null, Value::Null)?;
                Some(terrain)
            }
            Err(error) => {
                report.simple(
                    "terrain_capture",
                    "failed",
                    Some(terrain_source::diagnostic_id(&error)),
                )?;
                None
            }
        }
    };
    let capture = match source::prepare(&selected.data, selected.map, cancelled) {
        Ok(capture) => {
            let s = capture.summary();
            report.record("capture","passed",None,json!({"archive_count":s.archive_count,"member_lookups":s.member_lookups,"read_work_bytes":s.read_work_bytes,"adt_attempts":s.adt_attempts,"adts_present":s.adts_present,"adts_absent":s.adts_absent,"adts_dummy":s.adts_dummy,"products":s.products,"placements":s.placements,"doodad_candidates":capture.doodad_candidates().len(),"raw_bytes":s.retained_raw_bytes}),json!({"source_epoch":capture.source_epoch()}),Value::Null,Value::Null)?;
            Some(capture)
        }
        Err(error) => {
            let d = error.diagnostic();
            let mut metrics = json!({});
            if let Some(p) = d.progress {
                metrics = json!({"member_lookups":p.member_lookups,"read_work_bytes":p.read_work_bytes,"complete_raw_products":p.complete_raw_products,"adt_attempts":p.adt_attempts});
            }
            if let Some(ordinal) = d.global_root_ordinal {
                metrics["global_root_ordinal"] = json!(ordinal);
            }
            report.record(
                "capture",
                "failed",
                Some(if cancelled() {
                    "collision.timeout"
                } else {
                    &d.category
                }),
                metrics,
                json!({}),
                Value::Null,
                json!(d.stage),
            )?;
            None
        }
    };
    let profile = DoodadProjectionProfile::from_policy_id("g3d-portable-libm-0.2.16-v1");
    let mut placed = false;
    if let (Some(capture), Ok(profile)) = (&capture, &profile) {
        match capture.placement_records(*profile, cancelled) {
            Ok(records) => {
                report.record("placement","passed",None,json!({"placement_bytes":records.bytes().len(),"placement_records":records.record_count()}),json!({"preprocessing":hex(records.preprocessing_binding())}),Value::Null,Value::Null)?;
                placed = true;
            }
            Err(error) => report.simple("placement", "failed", Some(&error.to_string()))?,
        }
    } else {
        report.simple(
            "placement",
            "unavailable",
            Some(if profile.is_err() {
                "collision.profile"
            } else {
                "collision.prerequisite"
            }),
        )?;
    }
    let bundle = if placed {
        match capture
            .as_ref()
            .expect("placed capture")
            .assemble_map_bundle(*profile.as_ref().expect("placed profile"), cancelled)
        {
            Ok(Some(bundle)) => {
                let tile_bytes: usize = bundle
                    .map()
                    .tiles()
                    .iter()
                    .map(|tile| tile.bytes().len())
                    .sum();
                let model_bytes: usize = bundle
                    .models()
                    .iter()
                    .map(|model| model.bytes().len())
                    .sum();
                let serialized_bytes = tile_bytes + model_bytes + bundle.map().tree_bytes().len();
                report.record("assembly","passed",None,json!({"tree_bytes":bundle.map().tree_bytes().len(),"tiles":bundle.map().tiles().len(),"models":bundle.models().len(),"model_bytes":model_bytes,"tile_bytes":tile_bytes,"serialized_bytes":serialized_bytes}),json!({}),Value::Null,Value::Null)?;
                Some(bundle)
            }
            Ok(None) => {
                report.simple("assembly", "unavailable", Some("collision.empty_map"))?;
                None
            }
            Err(error) => {
                report.simple("assembly", "failed", Some(&bundle_code(&error)))?;
                None
            }
        }
    } else {
        report.simple("assembly", "unavailable", Some("collision.prerequisite"))?;
        None
    };
    let view = if let Some(bundle) = &bundle {
        match PreparedTileCollision::from_source(
            bundle,
            &[TileId {
                x: selected.x,
                y: selected.y,
            }],
            cancelled,
        ) {
            Ok(view) => {
                report.record("validation","passed",None,json!({}),json!({"preprocessing":hex(&view.source_version()),"inventory":hex(&view.binding().inventory),"policy":hex(&view.binding().policy)}),Value::Null,Value::Null)?;
                Some(view)
            }
            Err(error) => {
                report.simple("validation", "failed", Some(&prepare_code(&error)))?;
                None
            }
        }
    } else {
        report.simple("validation", "unavailable", Some("collision.prerequisite"))?;
        None
    };
    let mut completed = 0;
    if let Some(view) = &view {
        // Nine fixed public-world positions within the requested terrain grid tile.
        // These are observations; a miss is not a terrain or gameplay failure.
        for (index, (dx, dy)) in [0.25f32, 0.5, 0.75]
            .into_iter()
            .flat_map(|x| [0.25f32, 0.5, 0.75].into_iter().map(move |y| (x, y)))
            .enumerate()
        {
            let point = [
                (32.0 - f32::from(selected.x) - dx) * (1600.0 / 3.0),
                (32.0 - f32::from(selected.y) - dy) * (1600.0 / 3.0),
                2000.0,
            ]
            .map(f32::to_bits);
            let result = view.query_height(
                HeightRequest {
                    snapshot: view.binding(),
                    ticket: index as u64 + 1,
                    position: point,
                    max_search: 10000f32.to_bits(),
                },
                deadline,
            );
            let (status, code, value, result_name) = match result {
                Ok(reply) => {
                    completed += 1;
                    (
                        "passed",
                        None,
                        Some(reply.height),
                        if f32::from_bits(reply.height) == -200000.0 {
                            "miss"
                        } else {
                            "height"
                        },
                    )
                }
                Err(error) => (
                    "failed",
                    Some(if cancelled() {
                        "collision.timeout"
                    } else {
                        validation_code(error)
                    }),
                    None,
                    "error",
                ),
            };
            report.record("probe",status,code,json!({}),json!({}),json!({"index":index,"kind":"height","from_bits":point,"to_bits":null,"result":result_name,"value_bits":value}),Value::Null)?;
            if cancelled() {
                break;
            }
        }
    } else {
        report.simple("probe", "unavailable", Some("collision.prerequisite"))?;
    }
    let composed = if let (Some(terrain), Some(bundle)) = (&terrain, &bundle) {
        match PreparedSourceHeight::from_source(terrain, bundle, cancelled) {
            Ok(view) => {
                report.record(
                    "composition",
                    "passed",
                    None,
                    json!({}),
                    json!({"composition":hex(&view.binding().revision)}),
                    Value::Null,
                    Value::Null,
                )?;
                Some(view)
            }
            Err(error) => {
                report.simple("composition", "failed", Some(&composition_code(&error)))?;
                None
            }
        }
    } else {
        report.simple("composition", "unavailable", Some("collision.prerequisite"))?;
        None
    };
    let mut static_completed = 0;
    if let Some(view) = &composed {
        for (index, (dx, dy)) in [0.25f32, 0.5, 0.75]
            .into_iter()
            .flat_map(|x| [0.25f32, 0.5, 0.75].into_iter().map(move |y| (x, y)))
            .enumerate()
        {
            let point = [
                (32.0 - f32::from(selected.x) - dx) * (1600.0 / 3.0),
                (32.0 - f32::from(selected.y) - dy) * (1600.0 / 3.0),
                2000.0,
            ]
            .map(f32::to_bits);
            let result = view.query(
                StaticHeightRequest {
                    binding: view.binding(),
                    ticket: index as u64 + 1,
                    position: point,
                    max_search: 10000f32.to_bits(),
                },
                deadline,
            );
            let (status, code, value, result_name) = match result {
                Ok(reply) => {
                    static_completed += 1;
                    (
                        "passed",
                        None,
                        Some(reply.height),
                        if f32::from_bits(reply.height) == -200000.0 {
                            "miss"
                        } else {
                            "height"
                        },
                    )
                }
                Err(error) => (
                    "failed",
                    Some(if cancelled() {
                        "collision.timeout"
                    } else {
                        validation_code(error)
                    }),
                    None,
                    "error",
                ),
            };
            report.record("static_probe",status,code,json!({}),json!({}),
                json!({"index":index,"kind":"static_height","from_bits":point,"to_bits":null,"result":result_name,"value_bits":value}),Value::Null)?;
            if cancelled() {
                break;
            }
        }
        match view.verify_unchanged(cancelled) {
            Ok(()) => report.simple("composition_freshness", "passed", None)?,
            Err(error) => report.simple(
                "composition_freshness",
                "failed",
                Some(&composition_code(&error)),
            )?,
        }
    } else {
        report.simple(
            "static_probe",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
        report.simple(
            "composition_freshness",
            "unavailable",
            Some("collision.prerequisite"),
        )?;
    }
    if let Some(capture) = &capture {
        match capture.verify_unchanged(cancelled) {
            Ok(()) => report.simple("freshness", "passed", None)?,
            Err(error) => {
                report.simple("freshness", "failed", Some(&error.diagnostic().category))?
            }
        }
    } else {
        report.simple("freshness", "unavailable", Some("collision.prerequisite"))?;
    }
    let failed = report.failed;
    report.record(
        "complete",
        if failed > 0 {
            "failed"
        } else if completed == 9 && static_completed == 9 {
            "passed"
        } else {
            "unavailable"
        },
        if failed == 0 && (completed != 9 || static_completed != 9) {
            Some("collision.prerequisite")
        } else {
            None
        },
        json!({"completed_probes":completed,"completed_static_probes":static_completed,"failed_stages":failed}),
        json!({}),
        Value::Null,
        Value::Null,
    )
}
fn composition_code(error: &CompositionError) -> String {
    match error {
        CompositionError::Cancelled => "collision.cancelled".into(),
        CompositionError::TerrainSource => "terrain.source_changed".into(),
        CompositionError::Collision(error) => prepare_code(error),
        CompositionError::Validation(error) => validation_code(*error).into(),
    }
}
fn prepare_code(error: &PrepareError) -> String {
    match error {
        PrepareError::Cancelled => "collision.cancelled".into(),
        PrepareError::Source(error) => bundle_code(error),
        PrepareError::Validation(error) => validation_code(*error).into(),
    }
}
fn validation_code(error: super::super::Error) -> &'static str {
    use super::super::Error;
    match error {
        Error::Budget => "collision.budget",
        Error::Deadline => "collision.deadline",
        Error::Identity => "collision.identity",
        Error::Missing => "collision.missing",
        Error::Unexpected => "collision.unexpected",
        Error::Duplicate => "collision.duplicate",
        Error::Dependency => "collision.dependency",
        Error::Profile => "collision.profile",
        Error::Framing => "collision.framing",
        Error::Format => "collision.format",
        Error::Unsupported => "collision.unsupported",
        Error::Hash => "collision.hash",
        Error::Io => "collision.io",
        Error::OutsideTile => "collision.outside_tile",
    }
}

fn bundle_code(error: &source::MapBundleError) -> String {
    match error {
        source::MapBundleError::Source(error) => error.to_string(),
        source::MapBundleError::Map(error) => match error {
            source::MapProductError::Source(error) => error.to_string(),
            source::MapProductError::Bounds(error) => match error {
                source::SpawnBoundsError::Source(error) => error.to_string(),
                source::SpawnBoundsError::Preparation(error) => error.to_string(),
                _ => error.to_string(),
            },
            source::MapProductError::Index(error) => error.to_string(),
            source::MapProductError::Bih(error) => error.to_string(),
            _ => error.to_string(),
        },
        source::MapBundleError::Model(error) => match error {
            benilla_native_vmap::ModelWriteError::Invalid => "vmap.model_write.invalid".into(),
            benilla_native_vmap::ModelWriteError::Limit => "vmap.model_write.limit".into(),
            benilla_native_vmap::ModelWriteError::Allocation => {
                "vmap.model_write.allocation".into()
            }
            benilla_native_vmap::ModelWriteError::Cancelled => "vmap.model_write.cancelled".into(),
            benilla_native_vmap::ModelWriteError::Bih(error) => error.to_string(),
        },
        _ => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_selection_and_dynamic_error_text_are_redacted() {
        for args in [
            vec![],
            vec![OsString::from("/private/secret")],
            vec![
                "--data",
                "/private/secret",
                "--map",
                "1000",
                "--tile",
                "0,0",
                "--timeout-seconds",
                "2",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
        ] {
            let mut bytes = Vec::new();
            run(&args, &mut bytes).unwrap();
            let text = String::from_utf8(bytes).unwrap();
            assert!(!text.contains("secret"));
            let rows: Vec<Value> = text
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0]["code"], "collision.arguments");
            assert_eq!(rows[1]["gameplay_admitted"], false);
        }
        assert_eq!(
            safe_code("secret/error/filename"),
            "collision.unknown_error"
        );
        let codes: Vec<String> = serde_json::from_str(CODES).unwrap();
        let unique: std::collections::BTreeSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len());
        for code in codes {
            assert_eq!(safe_code(&code), code);
        }
    }
    #[test]
    fn expired_budget_preserves_stage_report_without_source_read() {
        let args: Vec<OsString> = [
            "--data",
            "/private/nonexistent",
            "--map",
            "0",
            "--tile",
            "48,32",
            "--timeout-seconds",
            "1",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let mut bytes = Vec::new();
        run_with_budget(&args, &mut bytes, Some(Duration::ZERO)).unwrap();
        let rows: Vec<Value> = String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[1]["code"], "collision.timeout");
        assert_eq!(rows[2]["source_stage"], "OpenSource");
        assert_eq!(rows[2]["code"], "collision.timeout");
        assert_eq!(rows.last().unwrap()["metrics"]["failed_stages"], 2);
        for row in &rows[3..rows.len() - 1] {
            assert_eq!(row["status"], "unavailable");
        }
    }
    #[test]
    fn output_is_flushed_and_bounded_and_write_errors_propagate() {
        #[derive(Default)]
        struct Sink {
            bytes: Vec<u8>,
            flushes: usize,
        }
        impl Write for Sink {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                self.flushes += 1;
                Ok(())
            }
        }
        let mut sink = Sink::default();
        let mut r = Report {
            output: &mut sink,
            selection: Selection::default(),
            start: Instant::now(),
            bytes: 0,
            lines: 0,
            failed: 0,
        };
        for _ in 0..32 {
            r.simple("probe", "unavailable", None).unwrap();
        }
        assert!(r.simple("probe", "unavailable", None).is_err());
        assert_eq!(sink.flushes, 32);
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("private error"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(run(&[], &mut Broken).is_err());
    }
}
